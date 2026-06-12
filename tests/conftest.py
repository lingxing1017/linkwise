import importlib.util
import os
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
APP_FILE = ROOT / "app" / "app.py"
PYTHON = ROOT / ".venv" / "bin" / "python"


def load_app_module(db_dir, secret_file):
    os.environ["LINKWISE_DB_DIR"] = str(db_dir)
    os.environ["LINKWISE_SECRET_FILE"] = str(secret_file)

    module_name = f"linkwise_app_{uuid.uuid4().hex}"
    spec = importlib.util.spec_from_file_location(module_name, APP_FILE)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    module.app.config.update(TESTING=True)
    return module


@pytest.fixture
def app_module(tmp_path):
    secret_file = tmp_path / "linkwise_secret_key"
    secret_file.write_text("test-secret-key", encoding="utf-8")
    return load_app_module(tmp_path / "data", secret_file)


@pytest.fixture
def client(app_module):
    return app_module.app.test_client()


def find_free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        try:
            sock.bind(("127.0.0.1", 0))
        except PermissionError as exc:
            pytest.skip(f"localhost port binding is not permitted in this environment: {exc}")
        return sock.getsockname()[1]


def wait_for_server(url, timeout=10):
    deadline = time.time() + timeout
    last_error = None

    while time.time() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=0.5) as response:
                if response.status < 500:
                    return
        except (OSError, urllib.error.URLError) as exc:
            last_error = exc
            time.sleep(0.1)

    raise RuntimeError(f"server did not start at {url}: {last_error}")


@pytest.fixture(scope="session")
def browser_name():
    return "chromium"


@pytest.fixture
def live_server(tmp_path):
    port = find_free_port()
    secret_file = tmp_path / "linkwise_secret_key"
    secret_file.write_text("test-secret-key", encoding="utf-8")

    env = os.environ.copy()
    env["LINKWISE_DB_DIR"] = str(tmp_path / "data")
    env["LINKWISE_SECRET_FILE"] = str(secret_file)
    env["PORT"] = str(port)

    process = subprocess.Popen(
        [str(PYTHON), "app.py"],
        cwd=ROOT / "app",
        env=env,
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
