pub const PASSWORD_AAD: &[u8] = b"linkwise-webdav-password-v1";
pub const SECRET_BINDING: &str = "LINKWISE_SECRET";

pub fn protect_webdav_password(password: &str, secret: &str) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(PASSWORD_AAD);
    hasher.update(secret.as_bytes());
    hasher.update(password.as_bytes());
    hex_encode(&hasher.finalize())
}

#[allow(dead_code)]
pub fn sha256_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex_encode(&hasher.finalize())
}

pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }

    output
}
