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

    def request(self, method, path, payload=None, headers=None):
        request_headers = dict(headers or {})
        data = None

        if payload is not None:
            data = json.dumps(payload).encode("utf-8")
            request_headers.setdefault("Content-Type", "application/json")

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


@pytest.fixture(scope="session")
def browser_name():
    return "chromium"


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

    local_d1_dir = tmp_path_factory.mktemp("linkwise-d1")
    port = find_free_port()
    process = subprocess.Popen(
        [
            "npm",
            "run",
            "dev",
            "--",
            "--ip",
            "127.0.0.1",
            "--port",
            str(port),
            "--persist-to",
            str(local_d1_dir),
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    base_url = f"http://127.0.0.1:{port}"

    try:
        wait_for_server(base_url)
        yield base_url
    except RuntimeError as exc:
        output = stop_process(process)
        raise RuntimeError(f"{exc}\n\nwrangler dev output:\n{output}") from exc
    finally:
        if process.poll() is None:
            stop_process(process)


@pytest.fixture
def api_client(live_server):
    return ApiClient(live_server)


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
