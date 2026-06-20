import os
import socket
import subprocess
import time
import urllib.error
import urllib.request

import pytest


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


@pytest.fixture(scope="session")
def browser_name():
    return "chromium"


@pytest.fixture(scope="session")
def live_server():
    base_url = os.environ.get("LINKWISE_E2E_BASE_URL", "").strip().rstrip("/")
    if base_url:
        wait_for_server(base_url)
        yield base_url
        return

    port = find_free_port()
    process = subprocess.Popen(
        ["npm", "run", "dev", "--", "--ip", "127.0.0.1", "--port", str(port)],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    base_url = f"http://127.0.0.1:{port}"

    try:
        wait_for_server(base_url)
        yield base_url
    finally:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


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
