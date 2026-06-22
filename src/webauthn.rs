use crate::{auth, crypto};
use js_sys::wasm_bindgen::JsCast;
use js_sys::{Array, Object, Promise, Reflect, Uint8Array};
use serde::Deserialize;
use wasm_bindgen_futures::JsFuture;
use worker::{Error, Result};

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicKeyCredentialPayload {
    pub id: String,
    pub raw_id: String,
    #[serde(rename = "type")]
    pub credential_type: String,
    pub response: AuthenticatorResponsePayload,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticatorResponsePayload {
    #[serde(rename = "clientDataJSON", alias = "clientDataJson")]
    pub client_data_json: String,
    pub attestation_object: Option<String>,
    pub authenticator_data: Option<String>,
    pub signature: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClientData {
    #[serde(rename = "type")]
    client_type: String,
    challenge: String,
    origin: String,
}

#[derive(Debug, Clone)]
pub struct RegistrationVerification {
    pub challenge: String,
    pub credential_id: String,
    pub public_key_jwk: String,
    pub sign_count: i64,
}

#[derive(Debug, Clone)]
pub struct LoginVerification {
    pub challenge: String,
    pub credential_id: String,
    pub sign_count: i64,
}

#[derive(Debug, Clone)]
struct ParsedAuthenticatorData {
    rp_id_hash: Vec<u8>,
    flags: u8,
    sign_count: i64,
    credential_id: Option<Vec<u8>>,
    public_key_jwk: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum CborValue {
    Integer(i64),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<CborValue>),
    Map(Vec<(CborValue, CborValue)>),
    Bool(bool),
    Null,
}

pub fn verify_registration_payload(
    payload: &PublicKeyCredentialPayload,
    expected_origin: &str,
    rp_id: &str,
) -> Result<RegistrationVerification> {
    if payload.credential_type != "public-key" {
        return Err(Error::RustError("invalid credential type".to_string()));
    }

    let client_data_json = auth::base64url_decode(&payload.response.client_data_json)?;
    let client_data = parse_client_data(&client_data_json, "webauthn.create", expected_origin)?;
    let attestation_object = payload
        .response
        .attestation_object
        .as_ref()
        .ok_or_else(|| Error::RustError("missing attestation object".to_string()))
        .and_then(|value| auth::base64url_decode(value))?;
    let auth_data = attestation_auth_data(&attestation_object)?;
    let parsed = parse_authenticator_data(&auth_data, true)?;

    validate_rp_id_hash(&parsed.rp_id_hash, rp_id)?;
    validate_user_present(parsed.flags)?;

    let credential_id = parsed
        .credential_id
        .ok_or_else(|| Error::RustError("missing credential id".to_string()))?;
    let public_key_jwk = parsed
        .public_key_jwk
        .ok_or_else(|| Error::RustError("missing public key".to_string()))?;

    Ok(RegistrationVerification {
        challenge: client_data.challenge,
        credential_id: auth::base64url_encode(&credential_id),
        public_key_jwk,
        sign_count: parsed.sign_count,
    })
}

pub async fn verify_login_payload(
    payload: &PublicKeyCredentialPayload,
    expected_origin: &str,
    rp_id: &str,
    public_key_jwk: &str,
) -> Result<LoginVerification> {
    if payload.credential_type != "public-key" {
        return Err(Error::RustError("invalid credential type".to_string()));
    }

    let client_data_json = auth::base64url_decode(&payload.response.client_data_json)?;
    let client_data = parse_client_data(&client_data_json, "webauthn.get", expected_origin)?;
    let authenticator_data = payload
        .response
        .authenticator_data
        .as_ref()
        .ok_or_else(|| Error::RustError("missing authenticator data".to_string()))
        .and_then(|value| auth::base64url_decode(value))?;
    let signature = payload
        .response
        .signature
        .as_ref()
        .ok_or_else(|| Error::RustError("missing signature".to_string()))
        .and_then(|value| auth::base64url_decode(value))?;
    let parsed = parse_authenticator_data(&authenticator_data, false)?;

    validate_rp_id_hash(&parsed.rp_id_hash, rp_id)?;
    validate_user_present(parsed.flags)?;

    let mut signed_data = authenticator_data;
    signed_data.extend(crypto::sha256_bytes(&client_data_json));

    if !verify_es256_signature(public_key_jwk, &signature, &signed_data).await? {
        return Err(Error::RustError("invalid WebAuthn signature".to_string()));
    }

    Ok(LoginVerification {
        challenge: client_data.challenge,
        credential_id: credential_id_from_payload(payload)?,
        sign_count: parsed.sign_count,
    })
}

fn parse_client_data(
    bytes: &[u8],
    expected_type: &str,
    expected_origin: &str,
) -> Result<ClientData> {
    let client_data = serde_json::from_slice::<ClientData>(bytes)
        .map_err(|_| Error::RustError("invalid client data JSON".to_string()))?;

    if client_data.client_type != expected_type {
        return Err(Error::RustError(
            "invalid WebAuthn client data type".to_string(),
        ));
    }

    if client_data.origin != expected_origin {
        return Err(Error::RustError("invalid WebAuthn origin".to_string()));
    }

    Ok(client_data)
}

fn attestation_auth_data(attestation_object: &[u8]) -> Result<Vec<u8>> {
    let mut parser = CborParser::new(attestation_object);
    let value = parser.parse_value()?;
    let CborValue::Map(entries) = value else {
        return Err(Error::RustError(
            "attestation object is not a map".to_string(),
        ));
    };

    for (key, value) in entries {
        if key == CborValue::Text("authData".to_string()) {
            if let CborValue::Bytes(bytes) = value {
                return Ok(bytes);
            }
        }
    }

    Err(Error::RustError(
        "attestation authData is missing".to_string(),
    ))
}

fn parse_authenticator_data(
    bytes: &[u8],
    expect_attested_data: bool,
) -> Result<ParsedAuthenticatorData> {
    if bytes.len() < 37 {
        return Err(Error::RustError(
            "authenticator data is too short".to_string(),
        ));
    }

    let rp_id_hash = bytes[0..32].to_vec();
    let flags = bytes[32];
    let sign_count = u32::from_be_bytes([bytes[33], bytes[34], bytes[35], bytes[36]]) as i64;
    let mut credential_id = None;
    let mut public_key_jwk = None;

    if expect_attested_data {
        if bytes.len() < 55 {
            return Err(Error::RustError(
                "attested credential data is too short".to_string(),
            ));
        }

        let credential_id_len = u16::from_be_bytes([bytes[53], bytes[54]]) as usize;
        let credential_start = 55;
        let credential_end = credential_start + credential_id_len;

        if bytes.len() <= credential_end {
            return Err(Error::RustError("credential id is truncated".to_string()));
        }

        credential_id = Some(bytes[credential_start..credential_end].to_vec());
        let mut parser = CborParser::new(&bytes[credential_end..]);
        let cose_key = parser.parse_value()?;
        public_key_jwk = Some(cose_key_to_jwk(&cose_key)?);
    }

    Ok(ParsedAuthenticatorData {
        rp_id_hash,
        flags,
        sign_count,
        credential_id,
        public_key_jwk,
    })
}

fn validate_rp_id_hash(actual: &[u8], rp_id: &str) -> Result<()> {
    let expected = crypto::sha256_bytes(rp_id.as_bytes());

    if actual == expected.as_slice() {
        Ok(())
    } else {
        Err(Error::RustError("invalid RP ID hash".to_string()))
    }
}

fn validate_user_present(flags: u8) -> Result<()> {
    if flags & 0x01 == 0x01 {
        Ok(())
    } else {
        Err(Error::RustError(
            "user presence flag is missing".to_string(),
        ))
    }
}

pub fn credential_id_from_payload(payload: &PublicKeyCredentialPayload) -> Result<String> {
    let raw_id = auth::base64url_decode(&payload.raw_id)?;
    let encoded = auth::base64url_encode(&raw_id);

    if encoded == payload.id {
        Ok(encoded)
    } else {
        Ok(payload.id.clone())
    }
}

fn cose_key_to_jwk(value: &CborValue) -> Result<String> {
    let CborValue::Map(entries) = value else {
        return Err(Error::RustError("COSE key is not a map".to_string()));
    };
    let mut key_type = None;
    let mut alg = None;
    let mut crv = None;
    let mut x = None;
    let mut y = None;

    for (key, value) in entries {
        let CborValue::Integer(key) = key else {
            continue;
        };

        match *key {
            1 => key_type = cbor_integer(value),
            3 => alg = cbor_integer(value),
            -1 => crv = cbor_integer(value),
            -2 => x = cbor_bytes(value),
            -3 => y = cbor_bytes(value),
            _ => {}
        }
    }

    if key_type != Some(2) || alg != Some(-7) || crv != Some(1) {
        return Err(Error::RustError(
            "unsupported WebAuthn public key".to_string(),
        ));
    }

    let x = x.ok_or_else(|| Error::RustError("COSE key x is missing".to_string()))?;
    let y = y.ok_or_else(|| Error::RustError("COSE key y is missing".to_string()))?;

    if x.len() != 32 || y.len() != 32 {
        return Err(Error::RustError(
            "invalid P-256 public key length".to_string(),
        ));
    }

    serde_json::json!({
        "kty": "EC",
        "crv": "P-256",
        "alg": "ES256",
        "x": auth::base64url_encode(&x),
        "y": auth::base64url_encode(&y),
        "key_ops": ["verify"],
        "ext": true
    })
    .to_string()
    .pipe(Ok)
}

fn cbor_integer(value: &CborValue) -> Option<i64> {
    match value {
        CborValue::Integer(value) => Some(*value),
        _ => None,
    }
}

fn cbor_bytes(value: &CborValue) -> Option<Vec<u8>> {
    match value {
        CborValue::Bytes(value) => Some(value.clone()),
        _ => None,
    }
}

async fn verify_es256_signature(
    public_key_jwk: &str,
    der_signature: &[u8],
    data: &[u8],
) -> Result<bool> {
    let raw_signature = der_to_raw_ecdsa_signature(der_signature)?;
    let global = js_sys::global();
    let crypto = Reflect::get(&global, &"crypto".into())
        .map_err(|_| Error::RustError("crypto global is unavailable".to_string()))?;
    let subtle = Reflect::get(&crypto, &"subtle".into())
        .map_err(|_| Error::RustError("crypto.subtle is unavailable".to_string()))?;
    let import_key = Reflect::get(&subtle, &"importKey".into())
        .map_err(|_| Error::RustError("crypto.subtle.importKey is unavailable".to_string()))?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| Error::RustError("crypto.subtle.importKey is not callable".to_string()))?;
    let verify = Reflect::get(&subtle, &"verify".into())
        .map_err(|_| Error::RustError("crypto.subtle.verify is unavailable".to_string()))?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| Error::RustError("crypto.subtle.verify is not callable".to_string()))?;
    let raw_public_key = jwk_to_raw_p256_public_key(public_key_jwk)?;
    let import_algorithm = Object::new();

    Reflect::set(&import_algorithm, &"name".into(), &"ECDSA".into())
        .map_err(|_| Error::RustError("failed to set import algorithm".to_string()))?;
    Reflect::set(&import_algorithm, &"namedCurve".into(), &"P-256".into())
        .map_err(|_| Error::RustError("failed to set import curve".to_string()))?;

    let key_usages = Array::new();
    key_usages.push(&"verify".into());

    let import_promise = import_key
        .call5(
            &subtle,
            &"raw".into(),
            &Uint8Array::from(raw_public_key.as_slice()),
            &import_algorithm,
            &false.into(),
            &key_usages,
        )
        .map_err(|_| Error::RustError("failed to import public key".to_string()))?
        .dyn_into::<Promise>()
        .map_err(|_| Error::RustError("importKey did not return a promise".to_string()))?;
    let crypto_key = JsFuture::from(import_promise)
        .await
        .map_err(|_| Error::RustError("public key import failed".to_string()))?;

    let verify_algorithm = Object::new();
    let hash = Object::new();
    Reflect::set(&verify_algorithm, &"name".into(), &"ECDSA".into())
        .map_err(|_| Error::RustError("failed to set verify algorithm".to_string()))?;
    Reflect::set(&hash, &"name".into(), &"SHA-256".into())
        .map_err(|_| Error::RustError("failed to set verify hash".to_string()))?;
    Reflect::set(&verify_algorithm, &"hash".into(), &hash)
        .map_err(|_| Error::RustError("failed to attach verify hash".to_string()))?;

    let signature = Uint8Array::from(raw_signature.as_slice());
    let signed_data = Uint8Array::from(data);
    let verify_promise = verify
        .call4(
            &subtle,
            &verify_algorithm,
            &crypto_key,
            &signature,
            &signed_data,
        )
        .map_err(|_| Error::RustError("signature verification failed".to_string()))?
        .dyn_into::<Promise>()
        .map_err(|_| Error::RustError("verify did not return a promise".to_string()))?;
    let verified = JsFuture::from(verify_promise)
        .await
        .map_err(|_| Error::RustError("signature verification rejected".to_string()))?;

    Ok(verified.as_bool().unwrap_or(false))
}

fn jwk_to_raw_p256_public_key(public_key_jwk: &str) -> Result<Vec<u8>> {
    let jwk = serde_json::from_str::<serde_json::Value>(public_key_jwk)
        .map_err(|_| Error::RustError("invalid stored JWK".to_string()))?;
    let x = jwk
        .get("x")
        .and_then(|value| value.as_str())
        .ok_or_else(|| Error::RustError("stored JWK x is missing".to_string()))
        .and_then(auth::base64url_decode)?;
    let y = jwk
        .get("y")
        .and_then(|value| value.as_str())
        .ok_or_else(|| Error::RustError("stored JWK y is missing".to_string()))
        .and_then(auth::base64url_decode)?;

    if x.len() != 32 || y.len() != 32 {
        return Err(Error::RustError("invalid stored P-256 key length".to_string()));
    }

    let mut raw = Vec::with_capacity(65);
    raw.push(0x04);
    raw.extend(x);
    raw.extend(y);
    Ok(raw)
}

fn der_to_raw_ecdsa_signature(signature: &[u8]) -> Result<Vec<u8>> {
    if signature.len() < 8 || signature[0] != 0x30 {
        return Err(Error::RustError("invalid ECDSA signature".to_string()));
    }

    let mut index = 2;
    if signature[1] & 0x80 != 0 {
        let length_bytes = (signature[1] & 0x7f) as usize;
        if length_bytes != 1 || signature.len() < 3 {
            return Err(Error::RustError(
                "unsupported ECDSA signature length".to_string(),
            ));
        }
        index = 3;
    }

    let r = read_der_integer(signature, &mut index)?;
    let s = read_der_integer(signature, &mut index)?;
    let mut raw = Vec::with_capacity(64);
    raw.extend(normalize_ecdsa_integer(&r)?);
    raw.extend(normalize_ecdsa_integer(&s)?);
    Ok(raw)
}

fn read_der_integer(signature: &[u8], index: &mut usize) -> Result<Vec<u8>> {
    if signature.get(*index).copied() != Some(0x02) {
        return Err(Error::RustError("invalid ECDSA integer".to_string()));
    }
    *index += 1;
    let len = *signature
        .get(*index)
        .ok_or_else(|| Error::RustError("truncated ECDSA integer".to_string()))?
        as usize;
    *index += 1;
    let end = *index + len;

    if end > signature.len() {
        return Err(Error::RustError(
            "truncated ECDSA integer value".to_string(),
        ));
    }

    let value = signature[*index..end].to_vec();
    *index = end;
    Ok(value)
}

fn normalize_ecdsa_integer(value: &[u8]) -> Result<Vec<u8>> {
    let trimmed = if value.len() > 32 && value[0] == 0 {
        &value[1..]
    } else {
        value
    };

    if trimmed.len() > 32 {
        return Err(Error::RustError("ECDSA integer is too large".to_string()));
    }

    let mut output = vec![0; 32 - trimmed.len()];
    output.extend(trimmed);
    Ok(output)
}

struct CborParser<'a> {
    bytes: &'a [u8],
    index: usize,
}

impl<'a> CborParser<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, index: 0 }
    }

    fn parse_value(&mut self) -> Result<CborValue> {
        let first = self.read_u8()?;
        let major = first >> 5;
        let additional = first & 0x1f;

        match major {
            0 => Ok(CborValue::Integer(self.read_len(additional)? as i64)),
            1 => Ok(CborValue::Integer(-1 - self.read_len(additional)? as i64)),
            2 => {
                let len = self.read_len(additional)? as usize;
                Ok(CborValue::Bytes(self.read_bytes(len)?.to_vec()))
            }
            3 => {
                let len = self.read_len(additional)? as usize;
                let bytes = self.read_bytes(len)?;
                String::from_utf8(bytes.to_vec())
                    .map(CborValue::Text)
                    .map_err(|_| Error::RustError("invalid CBOR text".to_string()))
            }
            4 => {
                let len = self.read_len(additional)? as usize;
                let mut items = Vec::with_capacity(len);
                for _ in 0..len {
                    items.push(self.parse_value()?);
                }
                Ok(CborValue::Array(items))
            }
            5 => {
                let len = self.read_len(additional)? as usize;
                let mut entries = Vec::with_capacity(len);
                for _ in 0..len {
                    let key = self.parse_value()?;
                    let value = self.parse_value()?;
                    entries.push((key, value));
                }
                Ok(CborValue::Map(entries))
            }
            7 => match additional {
                20 => Ok(CborValue::Bool(false)),
                21 => Ok(CborValue::Bool(true)),
                22 => Ok(CborValue::Null),
                _ => Err(Error::RustError("unsupported CBOR primitive".to_string())),
            },
            _ => Err(Error::RustError("unsupported CBOR value".to_string())),
        }
    }

    fn read_len(&mut self, additional: u8) -> Result<u64> {
        match additional {
            0..=23 => Ok(additional as u64),
            24 => Ok(self.read_u8()? as u64),
            25 => Ok(u16::from_be_bytes([self.read_u8()?, self.read_u8()?]) as u64),
            26 => Ok(u32::from_be_bytes([
                self.read_u8()?,
                self.read_u8()?,
                self.read_u8()?,
                self.read_u8()?,
            ]) as u64),
            _ => Err(Error::RustError("unsupported CBOR length".to_string())),
        }
    }

    fn read_u8(&mut self) -> Result<u8> {
        let value = *self
            .bytes
            .get(self.index)
            .ok_or_else(|| Error::RustError("unexpected end of CBOR".to_string()))?;
        self.index += 1;
        Ok(value)
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self.index + len;
        if end > self.bytes.len() {
            return Err(Error::RustError("unexpected end of CBOR bytes".to_string()));
        }

        let value = &self.bytes[self.index..end];
        self.index = end;
        Ok(value)
    }
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}
