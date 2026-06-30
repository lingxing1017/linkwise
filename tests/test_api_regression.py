import time

import pytest


pytestmark = pytest.mark.api
requires_admin_session = pytest.mark.admin_session


def unique_id(label):
    return f"api-{label}-{time.time_ns()}"


def bookmark_payload(**overrides):
    marker = unique_id("bookmark")
    payload = {
        "id": marker,
        "title": f"Example {marker}",
        "url": f"{marker}.example.test",
        "folder": f" Dev / {marker} ",
    }
    payload.update(overrides)
    return payload


def create_bookmark(api_client, **overrides):
    response = api_client.post("/api/bookmarks", bookmark_payload(**overrides))
    assert response.status == 200
    return response.json()


def create_app_device_token(api_client, label="app-token"):
    response = api_client.post("/api/auth/app-devices", {"name": unique_id(label)})
    assert response.status == 200
    return response.json()["token"]


def bearer_client(api_client, token):
    client = api_client.__class__(api_client.base_url)
    return client, {"Authorization": f"Bearer {token}"}


def bookmark_titles(api_client, marker):
    response = api_client.get("/api/bookmarks")
    assert response.status == 200
    return [item["title"] for item in response.json() if marker in item["title"]]


def test_health_check_returns_ok(api_client):
    response = api_client.get("/api/health")
    result = response.json()

    assert response.status == 200
    assert result["status"] == "success"
    assert result["app"] == "linkwise"
    assert result["version"]


def test_bootstrap_returns_bookmarks_and_folder_orders(api_client):
    response = api_client.get("/api/bootstrap")
    result = response.json()

    assert response.status == 200
    assert isinstance(result["bookmarks"], list)
    assert isinstance(result["folder_orders"], list)


def test_auth_status_returns_public_read_state(api_client):
    response = api_client.get("/api/auth/status")
    result = response.json()

    assert response.status == 200
    assert result["public_read"] is True
    assert result["admin_unlocked"] is False
    assert "admin_initialized" in result


@pytest.mark.parametrize(
    "path",
    [
        "/api/health",
        "/api/bootstrap",
        "/api/bookmarks",
        "/api/folder-orders",
        "/api/bookmarks/export",
        "/api/auth/status",
    ],
)
def test_public_read_apis_remain_accessible(api_client, path):
    response = api_client.get(path)

    assert response.status == 200


def test_setup_token_failures_are_rate_limited(api_client):
    marker = unique_id("setup-rate")
    headers = {"CF-Connecting-IP": f"198.51.100.{int(time.time_ns() % 200) + 1}"}
    payload = {"setup_token": f"wrong-token-{marker}", "name": "Bad Setup"}

    for _ in range(5):
        response = api_client.request(
            "POST",
            "/api/auth/passkey/register/options",
            payload,
            headers=headers,
        )
        result = response.json()

        assert response.status == 403
        assert result["status"] == "error"
        assert result["error"] in {"auth_required", "setup_not_allowed"}

    response = api_client.request(
        "POST",
        "/api/auth/passkey/register/options",
        payload,
        headers=headers,
    )
    result = response.json()

    assert response.status == 429
    assert result["status"] == "error"
    assert result["error"] == "rate_limited"


@pytest.mark.parametrize(
    ("method", "path"),
    [
        ("POST", "/api/auth/logout"),
        ("DELETE", "/api/auth/passkeys/missing"),
        ("DELETE", "/api/auth/sessions/missing"),
        ("POST", "/api/auth/sessions/missing/revoke"),
        ("POST", "/api/auth/sessions/revoke-all"),
        ("POST", "/api/auth/app-devices"),
        ("DELETE", "/api/auth/app-devices/missing"),
        ("POST", "/api/auth/app-devices/missing/revoke"),
        ("POST", "/api/auth/app-devices/revoke-all"),
    ],
)
def test_auth_mutations_require_json_content_type(api_client, method, path):
    response = api_client.request(
        method,
        path,
        payload=None,
        headers={"Origin": api_client.base_url},
    )
    result = response.json()

    assert response.status in {400, 403}
    assert result["status"] == "error"
    assert result["error"] == "invalid_content_type"


@pytest.mark.parametrize(
    ("method", "path", "payload"),
    [
        ("POST", "/api/auth/logout", {}),
        ("DELETE", "/api/auth/passkeys/missing", {}),
        ("DELETE", "/api/auth/sessions/missing", {}),
        ("POST", "/api/auth/sessions/missing/revoke", {}),
        ("POST", "/api/auth/sessions/revoke-all", {}),
        ("POST", "/api/auth/app-devices", {}),
        ("DELETE", "/api/auth/app-devices/missing", {}),
        ("POST", "/api/auth/app-devices/missing/revoke", {}),
        ("POST", "/api/auth/app-devices/revoke-all", {}),
    ],
)
def test_auth_mutations_reject_cross_origin(api_client, method, path, payload):
    response = api_client.request(
        method,
        path,
        payload,
        headers={"Origin": "https://evil.example.test"},
    )
    result = response.json()

    assert response.status == 403
    assert result["status"] == "error"
    assert result["error"] == "invalid_origin"


@pytest.mark.parametrize(
    "path",
    [
        "/api/bookmarks",
        "/api/bookmarks/bulk",
        "/api/bookmarks/move",
        "/api/bookmarks/reorder",
        "/api/bookmarks/delete",
        "/api/folders/reorder",
        "/api/folders/move-up",
        "/api/folders/rename",
        "/api/folders/delete",
        "/api/webdav/config",
        "/api/auth/passkey/register/options",
        "/api/auth/passkey/register/verify",
        "/api/auth/passkey/login/options",
        "/api/auth/passkey/login/verify",
    ],
)
def test_post_write_apis_require_json(api_client, path):
    response = api_client.request(
        "POST",
        path,
        payload=None,
        headers={"Origin": api_client.base_url},
    )
    result = response.json()

    assert response.status in {400, 403}
    assert result["status"] == "error"
    assert result["error"] == "invalid_content_type"


def test_invalid_content_type_returns_400(api_client):
    response = api_client.request(
        "POST",
        "/api/auth/passkey/login/options",
        payload=None,
        headers={"Origin": api_client.base_url},
    )
    result = response.json()

    assert response.status == 400
    assert result["status"] == "error"
    assert result["error"] == "invalid_content_type"


@pytest.mark.parametrize(
    ("method", "path", "payload"),
    [
        ("POST", "/api/bookmarks", bookmark_payload()),
        ("POST", "/api/bookmarks/bulk", {"bookmarks": [bookmark_payload()]}),
        ("POST", "/api/bookmarks/move", {"ids": ["missing"], "folder": "Archive"}),
        ("POST", "/api/bookmarks/reorder", {"folder": "Archive", "ids": ["missing"]}),
        ("POST", "/api/bookmarks/delete", {"ids": ["missing"]}),
        ("DELETE", "/api/bookmarks/missing", None),
        ("POST", "/api/folders/reorder", {"parent_folder": "", "folders": ["A"]}),
        ("POST", "/api/folders/move-up", {"folder": "A"}),
        ("POST", "/api/folders/rename", {"folder": "A", "new_folder": "B"}),
        ("POST", "/api/folders/delete", {"folder": "A"}),
        ("GET", "/api/webdav/config", None),
        ("POST", "/api/webdav/config", {"webdav_url": "https://dav.example.test"}),
    ],
)
def test_management_apis_require_admin_session(api_client, method, path, payload):
    response = api_client.request(method, path, payload)
    result = response.json()

    assert response.status == 401
    assert result["status"] == "error"
    assert result["error"] == "admin_session_required"


@pytest.mark.parametrize(
    ("method", "path", "payload"),
    [
        ("GET", "/api/auth/app-devices", None),
        ("POST", "/api/auth/app-devices", {"name": "Phone"}),
        ("DELETE", "/api/auth/app-devices/missing", {}),
        ("POST", "/api/auth/app-devices/missing/revoke", {}),
        ("POST", "/api/auth/app-devices/revoke-all", {}),
    ],
)
def test_app_device_management_requires_admin_session(api_client, method, path, payload):
    response = api_client.request(method, path, payload)
    result = response.json()

    assert response.status == 401
    assert result["status"] == "error"
    assert result["error"] == "admin_session_required"


@requires_admin_session
def test_app_device_token_is_returned_only_on_create(api_client):
    marker = unique_id("app-device")
    create = api_client.post("/api/auth/app-devices", {"name": f"Phone {marker}"})
    created = create.json()

    assert create.status == 200
    assert created["ok"] is True
    assert created["token"].startswith("lwapp_")
    assert created["device"]["name"] == f"Phone {marker}"
    assert created["device"]["token_prefix"] == created["token"][:14]

    listed = api_client.get("/api/auth/app-devices")
    result = listed.json()
    device = next(item for item in result["devices"] if item["id"] == created["device"]["id"])

    assert listed.status == 200
    assert result["ok"] is True
    assert "token" not in device
    assert device["token_prefix"] == created["device"]["token_prefix"]


@requires_admin_session
def test_app_device_revoke_marks_device_revoked(api_client):
    create = api_client.post("/api/auth/app-devices", {"name": unique_id("revoke-device")})
    device_id = create.json()["device"]["id"]

    revoke = api_client.post(f"/api/auth/app-devices/{device_id}/revoke", {})
    result = revoke.json()

    assert revoke.status == 200
    assert result["ok"] is True
    assert result["revoked_device_id"] == device_id

    listed = api_client.get("/api/auth/app-devices").json()
    device = next(item for item in listed["devices"] if item["id"] == device_id)
    assert device["revoked_at"]


@requires_admin_session
def test_revoked_app_device_can_be_deleted(api_client):
    create = api_client.post("/api/auth/app-devices", {"name": unique_id("delete-device")})
    device_id = create.json()["device"]["id"]

    revoke = api_client.post(f"/api/auth/app-devices/{device_id}/revoke", {})
    assert revoke.status == 200

    delete = api_client.request("DELETE", f"/api/auth/app-devices/{device_id}", {})
    result = delete.json()

    assert delete.status == 200
    assert result["ok"] is True
    assert result["deleted_device_id"] == device_id

    listed = api_client.get("/api/auth/app-devices").json()
    assert all(item["id"] != device_id for item in listed["devices"])


@requires_admin_session
def test_active_app_device_cannot_be_deleted(api_client):
    create = api_client.post("/api/auth/app-devices", {"name": unique_id("active-device")})
    device_id = create.json()["device"]["id"]

    delete = api_client.request("DELETE", f"/api/auth/app-devices/{device_id}", {})
    result = delete.json()

    assert delete.status == 409
    assert result["status"] == "error"
    assert result["error"] == "not_revoked"


@requires_admin_session
def test_active_admin_session_cannot_be_deleted(api_client):
    sessions = api_client.get("/api/auth/sessions").json()["sessions"]
    current_session = next(item for item in sessions if item["current"])

    delete = api_client.request("DELETE", f"/api/auth/sessions/{current_session['id']}", {})
    result = delete.json()

    assert delete.status == 409
    assert result["status"] == "error"
    assert result["error"] == "not_inactive"


@requires_admin_session
def test_app_device_revoke_all_marks_all_devices_revoked(api_client):
    first = api_client.post("/api/auth/app-devices", {"name": unique_id("revoke-all-a")}).json()
    second = api_client.post("/api/auth/app-devices", {"name": unique_id("revoke-all-b")}).json()
    device_ids = {first["device"]["id"], second["device"]["id"]}

    revoke = api_client.post("/api/auth/app-devices/revoke-all", {})
    result = revoke.json()

    assert revoke.status == 200
    assert result["ok"] is True

    listed = api_client.get("/api/auth/app-devices").json()
    devices = [item for item in listed["devices"] if item["id"] in device_ids]
    assert {item["id"] for item in devices} == device_ids
    assert all(item["revoked_at"] for item in devices)


@requires_admin_session
def test_app_bearer_can_write_bookmarks(api_client):
    token = create_app_device_token(api_client, "bearer-bookmark")
    app_client, headers = bearer_client(api_client, token)
    marker = unique_id("bearer-create")
    response = app_client.request(
        "POST",
        "/api/bookmarks",
        bookmark_payload(id=marker, title=f"Bearer {marker}", url=f"{marker}.example.test"),
        headers=headers,
    )
    result = response.json()

    assert response.status == 200
    assert result["status"] == "success"
    assert result["id"] == marker
    assert bookmark_titles(app_client, marker) == [f"Bearer {marker}"]


@requires_admin_session
def test_app_bearer_can_write_folders(api_client):
    token = create_app_device_token(api_client, "bearer-folder")
    app_client, headers = bearer_client(api_client, token)
    marker = unique_id("bearer-folder")
    create = app_client.request(
        "POST",
        "/api/bookmarks",
        bookmark_payload(id=f"{marker}-a", folder=f"Inbox / {marker}", url=f"{marker}.example.test"),
        headers=headers,
    )
    rename = app_client.request(
        "POST",
        "/api/folders/rename",
        {"folder": f"Inbox / {marker}", "new_folder": f"Archive / {marker}"},
        headers=headers,
    )

    assert create.status == 200
    assert rename.status == 200
    assert rename.json()["renamed_count"] == 1


@requires_admin_session
def test_revoked_app_bearer_cannot_write(api_client):
    create = api_client.post("/api/auth/app-devices", {"name": unique_id("bearer-revoked")}).json()
    token = create["token"]
    api_client.request("DELETE", f"/api/auth/app-devices/{create['device']['id']}", {})
    app_client, headers = bearer_client(api_client, token)
    response = app_client.request("POST", "/api/bookmarks", bookmark_payload(), headers=headers)
    result = response.json()

    assert response.status == 401
    assert result["status"] == "error"
    assert result["error"] == "app_session_required"


@requires_admin_session
@pytest.mark.parametrize(
    ("method", "path", "payload"),
    [
        ("GET", "/api/webdav/config", None),
        ("POST", "/api/webdav/config", {"webdav_url": "https://dav.example.test"}),
        ("GET", "/api/auth/app-devices", None),
        ("POST", "/api/auth/app-devices", {"name": "Nested"}),
    ],
)
def test_app_bearer_cannot_access_admin_only_apis(api_client, method, path, payload):
    token = create_app_device_token(api_client, "bearer-boundary")
    app_client, headers = bearer_client(api_client, token)
    response = app_client.request(method, path, payload, headers=headers)
    result = response.json()

    assert response.status == 401
    assert result["status"] == "error"
    assert result["error"] == "admin_session_required"


@requires_admin_session
def test_mixed_cookie_and_app_bearer_is_rejected(api_client):
    token = create_app_device_token(api_client, "bearer-mixed")
    response = api_client.request(
        "POST",
        "/api/bookmarks",
        bookmark_payload(),
        headers={"Authorization": f"Bearer {token}"},
    )
    result = response.json()

    assert response.status == 400
    assert result["status"] == "error"
    assert result["error"] == "mixed_auth_not_allowed"


@requires_admin_session
def test_create_bookmark_normalizes_url_and_folder(api_client):
    marker = unique_id("create")
    root = f"Root-{marker}"
    response = api_client.post(
        "/api/bookmarks",
        {
            "id": marker,
            "title": f"Example {marker}",
            "url": f"{marker}.example.test",
            "folder": f" {root} / Child ",
        },
    )
    result = response.json()

    assert response.status == 200
    assert result["status"] == "success"
    assert result["id"] == marker
    assert result["url"] == f"https://{marker}.example.test"
    assert result["folder"] == f"{root} / Child"

    bookmarks = api_client.get("/api/bookmarks").json()
    saved = next(item for item in bookmarks if item["id"] == marker)
    assert saved["folder"] == f"{root} / Child"
    assert saved["sort_order"] == 0

    folder_orders = api_client.get("/api/folder-orders").json()
    assert any(row["parent_folder"] == "" and row["folder_name"] == root for row in folder_orders)
    assert {"parent_folder": root, "folder_name": "Child", "sort_order": 0} in folder_orders


@pytest.mark.parametrize(
    ("raw_url", "normalized_url"),
    [
        ("example.com:8080/path", "https://example.com:8080/path"),
        ("localhost:3000", "https://localhost:3000"),
        ("https://user:pass@example.com", "https://user:pass@example.com"),
    ],
)
@requires_admin_session
def test_create_bookmark_allows_port_and_userinfo_urls(api_client, raw_url, normalized_url):
    marker = unique_id("url")
    response = api_client.post(
        "/api/bookmarks",
        {
            "id": marker,
            "title": f"URL {marker}",
            "url": raw_url,
            "folder": "API",
        },
    )

    assert response.status == 200
    assert response.json()["url"] == normalized_url


@pytest.mark.parametrize(
    "bad_url",
    [
        "javascript:alert(1)",
        "CHROME://settings",
        "chrome-extension://abcdef/options.html",
        "moz-extension://abcdef/sidebar.html",
        "edge://favorites",
        "file:///Users/example/bookmarks.html",
        "view-source:https://example.com",
        "example.com:abc/path",
        "https://example.com:abc",
    ],
)
@requires_admin_session
def test_create_bookmark_rejects_internal_url_schemes(api_client, bad_url):
    marker = unique_id("bad-url")
    response = api_client.post(
        "/api/bookmarks",
        {
            "id": marker,
            "title": f"Bad URL {marker}",
            "url": bad_url,
            "folder": "API",
        },
    )
    result = response.json()

    assert response.status == 400
    assert result["status"] == "error"
    assert result["error"] == "invalid_url"
    assert result["message"] == "URL 无效"


@requires_admin_session
def test_create_bookmark_rejects_missing_title(api_client):
    response = api_client.post(
        "/api/bookmarks",
        {
            "id": unique_id("missing-title"),
            "title": " ",
            "url": "https://missing-title.example.test",
            "folder": "API",
        },
    )
    result = response.json()

    assert response.status == 400
    assert result["status"] == "error"
    assert result["error"] == "missing_title"
    assert result["message"] == "标题不能为空"


@requires_admin_session
def test_duplicate_url_blocks_new_bookmark_but_allows_edit(api_client):
    marker = unique_id("duplicate")
    url = f"https://{marker}.example.test"

    assert create_bookmark(api_client, id=marker, title=f"Original {marker}", url=url)["status"] == "success"

    duplicate = api_client.post(
        "/api/bookmarks",
        {
            "id": f"{marker}-new",
            "title": f"Duplicate {marker}",
            "url": url,
            "folder": "API",
        },
    )
    edit_original = api_client.post(
        "/api/bookmarks",
        {
            "id": marker,
            "title": f"Updated {marker}",
            "url": url,
            "folder": "API",
        },
    )

    assert duplicate.status == 409
    assert duplicate.json()["status"] == "error"
    assert duplicate.json()["error"] == "duplicate_url"
    assert duplicate.json()["bookmark"]["id"] == marker
    assert edit_original.status == 200
    assert bookmark_titles(api_client, marker) == [f"Updated {marker}"]


@requires_admin_session
def test_bulk_import_counts_duplicates_and_skips_invalid_items(api_client):
    marker = unique_id("bulk")
    create_bookmark(
        api_client,
        id=f"{marker}-existing",
        title=f"Existing {marker}",
        url=f"https://existing-{marker}.test",
        folder="API",
    )

    response = api_client.post(
        "/api/bookmarks/bulk",
        {
            "bookmarks": [
                {"id": f"{marker}-a", "title": f"Alpha {marker}", "url": f"alpha-{marker}.test", "folder": "Work / A"},
                {"id": f"{marker}-b", "title": f"Beta {marker}", "url": f"https://beta-{marker}.test", "folder": "Work / B"},
                {"id": f"{marker}-bad", "title": f"Bad {marker}", "url": "javascript:bad"},
                {"id": f"{marker}-dup-existing", "title": f"Existing again {marker}", "url": f"existing-{marker}.test"},
                {"id": f"{marker}-dup-batch", "title": f"Alpha again {marker}", "url": f"https://alpha-{marker}.test"},
            ]
        },
    )
    result = response.json()

    assert response.status == 200
    assert result["imported_count"] == 2
    assert result["imported_ids"] == [f"{marker}-a", f"{marker}-b"]
    assert result["duplicate_count"] == 2
    assert result["skipped_count"] == 1
    assert result["total_count"] >= 3


@requires_admin_session
def test_move_reorder_and_bulk_delete_bookmarks(api_client):
    marker = unique_id("move")
    create_bookmark(api_client, id=f"{marker}-a", title=f"Alpha {marker}", url=f"alpha-{marker}.test", folder=f"Inbox / {marker}")
    create_bookmark(api_client, id=f"{marker}-b", title=f"Beta {marker}", url=f"beta-{marker}.test", folder=f"Inbox / {marker}")
    create_bookmark(api_client, id=f"{marker}-c", title=f"Gamma {marker}", url=f"gamma-{marker}.test", folder=f"Inbox / {marker}")

    target_folder = f"Archive / {marker}"
    move = api_client.post("/api/bookmarks/move", {"ids": [f"{marker}-a", f"{marker}-b"], "folder": target_folder})
    reorder = api_client.post("/api/bookmarks/reorder", {"folder": target_folder, "ids": [f"{marker}-b", "missing", f"{marker}-a", f"{marker}-b"]})
    delete = api_client.post("/api/bookmarks/delete", {"ids": [f"{marker}-a", f"{marker}-c"]})

    assert move.json()["moved_count"] == 2
    assert reorder.json()["updated_count"] == 2
    assert delete.json()["deleted_count"] == 2

    remaining = [
        item
        for item in api_client.get("/api/bookmarks").json()
        if item["id"] in {f"{marker}-a", f"{marker}-b", f"{marker}-c"}
    ]
    assert [(item["id"], item["folder"], item["sort_order"]) for item in remaining] == [
        (f"{marker}-b", target_folder, 0)
    ]


@requires_admin_session
def test_folder_reorder_rename_move_up_and_delete(api_client):
    marker = unique_id("folder")
    create_bookmark(api_client, id=f"{marker}-a", title=f"Alpha {marker}", url=f"alpha-{marker}.test", folder=f"Work / {marker} / Python")
    create_bookmark(api_client, id=f"{marker}-b", title=f"Beta {marker}", url=f"beta-{marker}.test", folder=f"Work / {marker} / JS")
    create_bookmark(api_client, id=f"{marker}-c", title=f"Gamma {marker}", url=f"gamma-{marker}.test", folder=f"Work / {marker} / Python / Flask")

    parent = f"Work / {marker}"
    reorder = api_client.post("/api/folders/reorder", {"parent_folder": parent, "folders": ["JS", "Python"]})
    rename = api_client.post("/api/folders/rename", {"folder": f"{parent} / Python", "new_folder": f"Dev / {marker} / Python"})
    move_up = api_client.post("/api/folders/move-up", {"folder": f"Dev / {marker} / Python"})
    delete = api_client.post("/api/folders/delete", {"folder": parent})

    assert reorder.json()["updated_count"] == 2
    assert rename.json()["renamed_count"] == 2
    assert move_up.json()["moved_count"] == 2
    assert delete.json()["deleted_count"] == 1

    folders = {
        item["id"]: item["folder"]
        for item in api_client.get("/api/bookmarks").json()
        if item["id"] in {f"{marker}-a", f"{marker}-b", f"{marker}-c"}
    }
    assert folders == {
        f"{marker}-a": f"Dev / {marker}",
        f"{marker}-c": f"Dev / {marker} / Flask",
    }


def test_export_escapes_bookmark_html(api_client):
    response = api_client.get("/api/bookmarks/export")
    body = response.text()

    assert response.status == 200
    assert "text/html" in response.headers.get("Content-Type", "")
    assert "NETSCAPE-Bookmark-file-1" in body
    assert "attachment;" in response.headers.get("Content-Disposition", "")


@requires_admin_session
def test_webdav_config_updates_metadata_without_password(api_client):
    marker = unique_id("webdav")
    response = api_client.post(
        "/api/webdav/config",
        {
            "webdav_url": f"https://dav-{marker}.example.test",
            "username": f"user-{marker}",
            "filename": f"backup-{marker}.html",
        },
    )
    result = response.json()

    assert response.status == 200
    assert result["status"] == "success"
    assert result["config"]["webdav_url"] == f"https://dav-{marker}.example.test"
    assert result["config"]["username"] == f"user-{marker}"
    assert "remote_dir" not in result["config"]
    assert result["config"]["filename"] == f"backup-{marker}.html"
    assert "password" not in result["config"]


@requires_admin_session
def test_webdav_config_requires_secret_for_password(api_client):
    marker = unique_id("webdav-secret")
    response = api_client.post(
        "/api/webdav/config",
        {
            "webdav_url": f"https://dav-{marker}.example.test",
            "username": f"user-{marker}",
            "password": "secret-password",
            "filename": "backup.html",
        },
    )

    if response.status == 200:
        result = response.json()
        assert result["config"]["has_password"] is True
        assert result["config"]["password_security"] == "worker_secret_hash"
        assert "password" not in result["config"]
        return

    result = response.json()
    assert response.status == 500
    assert result["status"] == "error"
    assert "LINKWISE_SECRET" in result["message"]
