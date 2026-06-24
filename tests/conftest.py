import json
import os
import socket
import subprocess
import time
import urllib.error
import urllib.request
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]


def pytest_configure(config):
    config.addinivalue_line(
        "markers",
        "admin_session: run this API regression with a Passkey-authenticated session",
    )


class ApiResponse:
    def __init__(self, status, headers, body):
        self.status = status
        self.headers = headers
        self.body = body

    def json(self):
        return json.loads(self.body.decode("utf-8"))

    def text(self):
        return self.body.decode("utf-8")


class ApiClient:
    def __init__(self, base_url):
        self.base_url = base_url.rstrip("/")
        self.cookies = {}

    def request(self, method, path, payload=None, headers=None):
        request_headers = dict(headers or {})
        data = None

        if payload is not None:
            data = json.dumps(payload).encode("utf-8")
            request_headers.setdefault("Content-Type", "application/json")

        if self.cookies and "Cookie" not in request_headers:
            request_headers["Cookie"] = "; ".join(
                f"{name}={value}" for name, value in self.cookies.items()
            )

        request = urllib.request.Request(
            f"{self.base_url}{path}",
            data=data,
            headers=request_headers,
            method=method,
        )

        try:
            with urllib.request.urlopen(request, timeout=10) as response:
                return ApiResponse(response.status, response.headers, response.read())
        except urllib.error.HTTPError as exc:
            return ApiResponse(exc.code, exc.headers, exc.read())

    def get(self, path):
        return self.request("GET", path)

    def post(self, path, payload):
        return self.request("POST", path, payload)

    def delete(self, path):
        return self.request("DELETE", path)

    def with_cookie(self, name, value):
        client = ApiClient(self.base_url)
        client.cookies = {**self.cookies, name: value}
        return client


def find_free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        try:
            sock.bind(("127.0.0.1", 0))
        except PermissionError as exc:
            pytest.skip(f"localhost port binding is not permitted in this environment: {exc}")
        return sock.getsockname()[1]


def wait_for_server(url, timeout=30):
    deadline = time.time() + timeout
    last_error = None

    while time.time() < deadline:
        try:
            with urllib.request.urlopen(f"{url}/api/health", timeout=0.5) as response:
                if response.status < 500:
                    return
        except (OSError, urllib.error.URLError) as exc:
            last_error = exc
            time.sleep(0.2)

    raise RuntimeError(f"server did not start at {url}: {last_error}")


def stop_process(process):
    process.terminate()
    try:
        output, _ = process.communicate(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        output, _ = process.communicate(timeout=5)
    return output


def start_wrangler_server(tmp_path_factory, prefix, extra_vars=None, public_host="127.0.0.1"):
    local_d1_dir = tmp_path_factory.mktemp(f"{prefix}-d1")
    port = find_free_port()
    inspector_port = find_free_port()
    base_url = f"http://{public_host}:{port}"
    args = [
        "npm",
        "run",
        "dev",
        "--",
        "--ip",
        "127.0.0.1",
        "--port",
        str(port),
        "--inspector-port",
        str(inspector_port),
        "--persist-to",
        str(local_d1_dir),
    ]

    for key, value in (extra_vars or {}).items():
        if value == "__BASE_URL__":
            value = base_url
        args.extend(["--var", f"{key}:{value}"])

    process = subprocess.Popen(
        args,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )

    try:
        wait_for_server(base_url)
        return base_url, process
    except RuntimeError as exc:
        output = stop_process(process)
        raise RuntimeError(f"{exc}\n\nwrangler dev output:\n{output}") from exc


@pytest.fixture(scope="session")
def browser_name():
    return "chromium"


def launch_browser_with_virtual_authenticator(browser_name):
    pytest.importorskip("playwright.sync_api")
    from playwright.sync_api import Error, sync_playwright

    playwright = sync_playwright().start()
    browser_type = getattr(playwright, browser_name)
    launch_errors = []
    channel = os.environ.get("LINKWISE_E2E_BROWSER_CHANNEL", "chrome").strip()
    launch_attempts = [{"channel": channel}] if channel else []
    launch_attempts.append({})

    for launch_options in launch_attempts:
        try:
            browser = browser_type.launch(**launch_options)
            break
        except Error as exc:
            label = launch_options.get("channel") or "playwright-managed"
            launch_errors.append(f"{label}: {exc}")
    else:
        playwright.stop()
        pytest.skip("No usable browser found for E2E tests:\n" + "\n".join(launch_errors))

    context = browser.new_context()
    page = context.new_page()
    client = context.new_cdp_session(page)
    client.send("WebAuthn.enable")
    client.send(
        "WebAuthn.addVirtualAuthenticator",
        {
            "options": {
                "protocol": "ctap2",
                "transport": "internal",
                "hasResidentKey": True,
                "hasUserVerification": True,
                "isUserVerified": True,
                "automaticPresenceSimulation": True,
            }
        },
    )
    return playwright, browser, context, page


def register_passkey_and_get_session_cookie(base_url, setup_token, browser_name):
    playwright, browser, context, page = launch_browser_with_virtual_authenticator(browser_name)

    try:
        page.goto(base_url)
        page.locator("#cards-wrapper").wait_for()
        result = page.evaluate(
            """
            async ({ setupToken }) => {
                const optionsRes = await fetch('/api/auth/passkey/register/options', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        setup_token: setupToken,
                        name: 'API Passkey'
                    })
                });
                const options = await optionsRes.json();

                if (!optionsRes.ok) {
                    return { ok: false, status: optionsRes.status, body: options };
                }

                const credential = await navigator.credentials.create({
                    publicKey: decodeCredentialCreationOptions(options.publicKey)
                });
                const verifyRes = await fetch('/api/auth/passkey/register/verify', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        name: 'API Passkey',
                        session_max_age_seconds: 3600,
                        credential: publicKeyCredentialToJSON(credential)
                    })
                });
                const verify = await verifyRes.json();
                return { ok: verifyRes.ok && verify.ok, status: verifyRes.status, body: verify };
            }
            """,
            {"setupToken": setup_token},
        )

        if not result["ok"]:
            raise RuntimeError(f"Passkey setup failed: {result}")

        cookies = {
            cookie["name"]: cookie["value"]
            for cookie in context.cookies(base_url)
        }
        session = cookies.get("linkwise_admin_session")
        if not session:
            raise RuntimeError("Passkey setup did not issue linkwise_admin_session")
        return session
    finally:
        context.close()
        browser.close()
        playwright.stop()


@pytest.fixture(scope="session")
def live_server(tmp_path_factory):
    base_url = (
        os.environ.get("LINKWISE_API_BASE_URL", "").strip()
        or os.environ.get("LINKWISE_E2E_BASE_URL", "").strip()
    ).rstrip("/")
    if base_url:
        wait_for_server(base_url)
        yield base_url
        return

    base_url, process = start_wrangler_server(tmp_path_factory, "linkwise")

    try:
        yield base_url
    finally:
        if process.poll() is None:
            stop_process(process)


@pytest.fixture(scope="session")
def auth_setup_token():
    return "linkwise-e2e-setup-token"


@pytest.fixture(scope="session")
def authenticated_live_server(tmp_path_factory, auth_setup_token):
    base_url, process = start_wrangler_server(
        tmp_path_factory,
        "linkwise-auth",
        extra_vars={
            "LINKWISE_SETUP_TOKEN": auth_setup_token,
            "LINKWISE_SECRET": "e2e-linkwise-secret",
            "LINKWISE_AUTH_ORIGIN": "__BASE_URL__",
            "LINKWISE_AUTH_RP_ID": "localhost",
        },
        public_host="localhost",
    )

    try:
        yield base_url
    finally:
        if process.poll() is None:
            stop_process(process)


@pytest.fixture(scope="session")
def passkey_flow_live_server(tmp_path_factory, auth_setup_token):
    base_url, process = start_wrangler_server(
        tmp_path_factory,
        "linkwise-passkey-flow",
        extra_vars={
            "LINKWISE_SETUP_TOKEN": auth_setup_token,
            "LINKWISE_SECRET": "e2e-linkwise-secret",
            "LINKWISE_AUTH_ORIGIN": "__BASE_URL__",
            "LINKWISE_AUTH_RP_ID": "localhost",
        },
        public_host="localhost",
    )

    try:
        yield base_url
    finally:
        if process.poll() is None:
            stop_process(process)


@pytest.fixture
def api_client(request, live_server):
    if request.node.get_closest_marker("admin_session"):
        return request.getfixturevalue("authenticated_api_client")
    return ApiClient(live_server)


@pytest.fixture(scope="session")
def admin_session_cookie(authenticated_live_server, auth_setup_token, browser_name):
    return register_passkey_and_get_session_cookie(
        authenticated_live_server,
        auth_setup_token,
        browser_name,
    )


@pytest.fixture
def authenticated_api_client(authenticated_live_server, admin_session_cookie):
    return ApiClient(authenticated_live_server).with_cookie(
        "linkwise_admin_session",
        admin_session_cookie,
    )


@pytest.fixture
def admin_page(authenticated_live_server, admin_session_cookie, browser_name):
    playwright, browser, context, page = launch_browser_with_virtual_authenticator(browser_name)
    context.add_cookies(
        [
            {
                "name": "linkwise_admin_session",
                "value": admin_session_cookie,
                "url": authenticated_live_server,
                "httpOnly": True,
                "sameSite": "Lax",
            }
        ]
    )
    try:
        yield page
    finally:
        context.close()
        browser.close()
        playwright.stop()


@pytest.fixture
def page(live_server, browser_name):
    pytest.importorskip("playwright.sync_api")
    from playwright.sync_api import Error, sync_playwright

    with sync_playwright() as playwright:
        browser_type = getattr(playwright, browser_name)
        launch_errors = []
        channel = os.environ.get("LINKWISE_E2E_BROWSER_CHANNEL", "chrome").strip()
        launch_attempts = [{"channel": channel}] if channel else []
        launch_attempts.append({})

        for launch_options in launch_attempts:
            try:
                browser = browser_type.launch(**launch_options)
                break
            except Error as exc:
                label = launch_options.get("channel") or "playwright-managed"
                launch_errors.append(f"{label}: {exc}")
        else:
            pytest.skip("No usable browser found for E2E tests:\n" + "\n".join(launch_errors))

        context = browser.new_context()
        page = context.new_page()
        yield page
        context.close()
        browser.close()


@pytest.fixture
def authenticated_page(passkey_flow_live_server, browser_name):
    playwright, browser, context, page = launch_browser_with_virtual_authenticator(browser_name)
    try:
        yield page
    finally:
        context.close()
        browser.close()
        playwright.stop()
