import time

import pytest


pytestmark = pytest.mark.api


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
    marker = unique_id("bootstrap")
    root = f"Boot-{marker}"
    create_bookmark(
        api_client,
        id=marker,
        title=f"Bootstrap {marker}",
        url=f"{marker}.example.test",
        folder=f"{root} / Child",
    )

    response = api_client.get("/api/bootstrap")
    result = response.json()

    assert response.status == 200
    assert any(item["id"] == marker for item in result["bookmarks"])
    assert any(item["folder_name"] == root for item in result["folder_orders"])


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
                "not a bookmark",
            ]
        },
    )
    result = response.json()

    assert response.status == 200
    assert result["imported_count"] == 2
    assert result["imported_ids"] == [f"{marker}-a", f"{marker}-b"]
    assert result["duplicate_count"] == 2
    assert result["skipped_count"] == 2
    assert result["total_count"] >= 3


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
    marker = unique_id("export")
    create_bookmark(
        api_client,
        id=marker,
        title=f"<Alpha & Co {marker}>",
        url=f"https://alpha-{marker}.test/?q=1&x=2",
        folder=f"Dev / <Tools {marker}>",
    )

    response = api_client.get("/api/bookmarks/export")
    body = response.text()

    assert response.status == 200
    assert "text/html" in response.headers.get("Content-Type", "")
    assert "NETSCAPE-Bookmark-file-1" in body
    assert f"&lt;Alpha &amp; Co {marker}&gt;" in body
    assert f'HREF="https://alpha-{marker}.test/?q=1&amp;x=2"' in body
    assert f"&lt;Tools {marker}&gt;" in body
    assert "attachment;" in response.headers.get("Content-Disposition", "")


def test_webdav_config_updates_metadata_without_password(api_client):
    marker = unique_id("webdav")
    response = api_client.post(
        "/api/webdav/config",
        {
            "webdav_url": f"https://dav-{marker}.example.test",
            "username": f"user-{marker}",
            "remote_dir": f"Linkwise/{marker}",
            "filename": f"backup-{marker}.html",
        },
    )
    result = response.json()

    assert response.status == 200
    assert result["status"] == "success"
    assert result["config"]["webdav_url"] == f"https://dav-{marker}.example.test"
    assert result["config"]["username"] == f"user-{marker}"
    assert result["config"]["remote_dir"] == f"Linkwise/{marker}"
    assert result["config"]["filename"] == f"backup-{marker}.html"
    assert "password" not in result["config"]


def test_webdav_config_requires_secret_for_password(api_client):
    marker = unique_id("webdav-secret")
    response = api_client.post(
        "/api/webdav/config",
        {
            "webdav_url": f"https://dav-{marker}.example.test",
            "username": f"user-{marker}",
            "password": "secret-password",
            "remote_dir": "Linkwise",
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
