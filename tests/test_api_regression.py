import sqlite3


def bookmark_payload(**overrides):
    payload = {
        "id": "b1",
        "title": "Example",
        "url": "example.com",
        "folder": " Dev / Python ",
    }
    payload.update(overrides)
    return payload


def post_bookmark(client, **overrides):
    return client.post("/api/bookmarks", json=bookmark_payload(**overrides))


def bookmark_titles(client):
    return [item["title"] for item in client.get("/api/bookmarks").get_json()]


def test_health_check_returns_ok(client, app_module):
    response = client.get("/api/health")
    result = response.get_json()

    assert response.status_code == 200
    assert result == {
        "status": "ok",
        "app": "linkwise",
        "version": app_module.APP_VERSION,
    }


def test_create_bookmark_normalizes_url_and_folder(client):
    response = post_bookmark(client)

    assert response.status_code == 200
    assert response.get_json() == {
        "status": "success",
        "id": "b1",
        "title": "Example",
        "url": "https://example.com",
        "folder": "Dev / Python",
        "total_count": 1,
    }

    bookmarks = client.get("/api/bookmarks").get_json()
    assert bookmarks[0]["folder"] == "Dev / Python"
    assert bookmarks[0]["sort_order"] == 0

    folder_orders = client.get("/api/folder-orders").get_json()
    assert {"parent_folder": "", "folder_name": "Dev", "sort_order": 0} in folder_orders
    assert {"parent_folder": "Dev", "folder_name": "Python", "sort_order": 0} in folder_orders


def test_create_bookmark_rejects_invalid_input(client):
    missing_title = post_bookmark(client, title=" ")
    invalid_url = post_bookmark(client, id="b2", url="javascript:alert(1)")

    assert missing_title.status_code == 400
    assert missing_title.get_json()["status"] == "error"
    assert missing_title.get_json()["error"] == "missing_title"
    assert missing_title.get_json()["message"] == "标题不能为空"
    assert invalid_url.status_code == 400
    assert invalid_url.get_json()["status"] == "error"
    assert invalid_url.get_json()["error"] == "invalid_url"
    assert invalid_url.get_json()["message"] == "URL 无效"


def test_duplicate_url_blocks_new_bookmark_but_allows_edit(client):
    assert post_bookmark(client, id="original", title="Original", url="https://example.com").status_code == 200

    duplicate = post_bookmark(client, id="new", title="Duplicate", url="example.com")
    edit_original = post_bookmark(client, id="original", title="Updated", url="example.com")

    assert duplicate.status_code == 409
    assert duplicate.get_json()["status"] == "duplicate"
    assert duplicate.get_json()["error"] == "duplicate_url"
    assert duplicate.get_json()["bookmark"]["id"] == "original"
    assert edit_original.status_code == 200
    assert bookmark_titles(client) == ["Updated"]


def test_bulk_import_counts_duplicates_and_skips_invalid_items(client):
    post_bookmark(client, id="existing", title="Existing", url="https://existing.test")

    response = client.post(
        "/api/bookmarks/bulk",
        json={
            "bookmarks": [
                {"id": "a", "title": "Alpha", "url": "alpha.test", "folder": "Work / A"},
                {"id": "b", "title": "Beta", "url": "https://beta.test", "folder": "Work / B"},
                {"id": "bad", "title": "Bad", "url": "javascript:bad"},
                {"id": "dup-existing", "title": "Existing again", "url": "existing.test"},
                {"id": "dup-batch", "title": "Alpha again", "url": "https://alpha.test"},
                "not a bookmark",
            ]
        },
    )

    result = response.get_json()
    assert response.status_code == 200
    assert result["imported_count"] == 2
    assert result["imported_ids"] == ["a", "b"]
    assert result["duplicate_count"] == 2
    assert result["skipped_count"] == 2
    assert result["total_count"] == 3


def test_move_reorder_and_bulk_delete_bookmarks(client):
    post_bookmark(client, id="a", title="Alpha", url="alpha.test", folder="Inbox")
    post_bookmark(client, id="b", title="Beta", url="beta.test", folder="Inbox")
    post_bookmark(client, id="c", title="Gamma", url="gamma.test", folder="Inbox")

    move = client.post("/api/bookmarks/move", json={"ids": ["a", "b"], "folder": "Archive / 2026"})
    reorder = client.post("/api/bookmarks/reorder", json={"folder": "Archive / 2026", "ids": ["b", "missing", "a", "b"]})
    delete = client.post("/api/bookmarks/delete", json={"ids": ["a", "c"]})

    assert move.get_json()["moved_count"] == 2
    assert reorder.get_json()["updated_count"] == 2
    assert delete.get_json()["deleted_count"] == 2

    remaining = client.get("/api/bookmarks").get_json()
    assert [(item["id"], item["folder"], item["sort_order"]) for item in remaining] == [
        ("b", "Archive / 2026", 0)
    ]


def test_folder_reorder_rename_move_up_and_delete(client):
    post_bookmark(client, id="a", title="Alpha", url="alpha.test", folder="Work / Python")
    post_bookmark(client, id="b", title="Beta", url="beta.test", folder="Work / JS")
    post_bookmark(client, id="c", title="Gamma", url="gamma.test", folder="Work / Python / Flask")

    reorder = client.post("/api/folders/reorder", json={"parent_folder": "Work", "folders": ["JS", "Python"]})
    rename = client.post("/api/folders/rename", json={"folder": "Work / Python", "new_folder": "Dev / Python"})
    move_up = client.post("/api/folders/move-up", json={"folder": "Dev / Python"})
    delete = client.post("/api/folders/delete", json={"folder": "Work"})

    assert reorder.get_json()["updated_count"] == 2
    assert rename.get_json()["renamed_count"] == 2
    assert move_up.get_json()["moved_count"] == 2
    assert delete.get_json()["deleted_count"] == 1

    folders = {item["id"]: item["folder"] for item in client.get("/api/bookmarks").get_json()}
    assert folders == {"a": "Dev", "c": "Dev / Flask"}


def test_export_escapes_bookmark_html(client):
    post_bookmark(client, id="a", title="<Alpha & Co>", url="https://alpha.test/?q=1&x=2", folder="Dev / <Tools>")

    response = client.get("/api/bookmarks/export")
    body = response.get_data(as_text=True)

    assert response.status_code == 200
    assert response.mimetype == "text/html"
    assert "NETSCAPE-Bookmark-file-1" in body
    assert "&lt;Alpha &amp; Co&gt;" in body
    assert 'HREF="https://alpha.test/?q=1&amp;x=2"' in body
    assert "&lt;Tools&gt;" in body


def test_webdav_config_encrypts_password_and_hides_plaintext(client, app_module):
    response = client.post(
        "/api/webdav/config",
        json={
            "webdav_url": "https://dav.example.test",
            "username": "alice",
            "password": "secret-password",
            "remote_dir": "Linkwise",
            "filename": "backup.html",
        },
    )

    result = response.get_json()
    assert response.status_code == 200
    assert result["config"]["has_password"] is True
    assert result["config"]["password_security"] == "encrypted"
    assert "password" not in result["config"]

    with sqlite3.connect(app_module.DB_FILE) as conn:
        rows = dict(conn.execute("SELECT key, value FROM settings").fetchall())

    assert rows["webdav_password_ciphertext"] != "secret-password"
    assert "webdav_password" not in rows
