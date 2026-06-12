import pytest


pytestmark = pytest.mark.e2e


def seed_bookmark(base_url, payload):
    import urllib.request
    import json

    request = urllib.request.Request(
        f"{base_url}/api/bookmarks",
        data=json.dumps(payload).encode("utf-8"),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=5) as response:
        return json.loads(response.read().decode("utf-8"))


def bookmark_row(page, title):
    return page.locator(".bookmark-row", has=page.locator(".bookmark-title", has_text=title))


def confirm_dialog(page):
    page.locator("#app-dialog-confirm").click()


def test_create_search_and_uncategorized_view(page, live_server):
    page.goto(live_server)
    page.locator(".state-text").wait_for(state="visible")
    assert "没有匹配到任何书签" in page.locator("#cards-wrapper").inner_text()

    page.locator(".floating-add-btn").click()
    page.get_by_title("添加书签").click()
    page.locator("#title").fill("Python Docs")
    page.locator("#url").fill("python.org")
    page.locator("#folder").fill("Dev / Python")
    page.locator("#bookmark-form .btn-save").click()

    bookmark_row(page, "Python Docs").wait_for()
    assert "Dev" in page.locator("#folder-list").inner_text()

    page.locator("#search").fill("python.org")
    assert bookmark_row(page, "Python Docs").count() == 1

    page.locator("#search").fill("")
    page.locator("#smart-view-list .folder-item", has_text="未分类").click()
    assert "没有匹配到任何书签" in page.locator("#cards-wrapper").inner_text()


def test_duplicate_url_can_open_original_for_edit(page, live_server):
    seed_bookmark(
        live_server,
        {"id": "original", "title": "Original Site", "url": "https://example.test", "folder": "Dev"},
    )

    page.goto(live_server)
    bookmark_row(page, "Original Site").wait_for()
    page.locator(".floating-add-btn").click()
    page.get_by_title("添加书签").click()
    page.locator("#title").fill("Duplicate Site")
    page.locator("#url").fill("example.test")
    page.locator("#bookmark-form .btn-save").click()

    page.locator("#app-dialog-title", has_text="书签已存在").wait_for()
    confirm_dialog(page)

    assert page.locator("#box-title").inner_text() == "编辑书签"
    assert page.locator("#bookmark-id").input_value() == "original"
    assert page.locator("#title").input_value() == "Original Site"


def test_bulk_move_and_delete_selected_bookmarks(page, live_server):
    seed_bookmark(live_server, {"id": "a", "title": "Alpha", "url": "alpha.test", "folder": "Inbox"})
    seed_bookmark(live_server, {"id": "b", "title": "Beta", "url": "beta.test", "folder": "Inbox"})

    page.goto(live_server)
    bookmark_row(page, "Alpha").wait_for()
    bookmark_row(page, "Alpha").locator(".bookmark-select input").check()
    bookmark_row(page, "Beta").locator(".bookmark-select input").check()
    assert page.locator("#bulk-selected-count").inner_text() == "2"

    page.locator(".bulk-mini-btn", has_text="移动").click()
    page.locator("#bulk-move-folder").fill("Archive / 2026")
    page.locator("#bulk-move-overlay .btn-save").click()
    confirm_dialog(page)
    page.locator("#app-dialog-title", has_text="移动完成").wait_for()
    confirm_dialog(page)

    assert "Archive" in page.locator("#folder-list").inner_text()
    bookmark_row(page, "Alpha").locator(".bookmark-select input").check()
    page.locator(".bulk-mini-btn.danger", has_text="删除").click()
    confirm_dialog(page)
    page.locator("#app-dialog-title", has_text="删除完成").wait_for()
    confirm_dialog(page)

    assert bookmark_row(page, "Alpha").count() == 0
    assert bookmark_row(page, "Beta").count() == 1


def test_import_bookmarks_html_shows_last_import_view(page, live_server, tmp_path):
    import_file = tmp_path / "bookmarks.html"
    import_file.write_text(
        """
        <!DOCTYPE NETSCAPE-Bookmark-file-1>
        <DL><p>
            <DT><H3>Dev</H3>
            <DL><p>
                <DT><A HREF="https://python.org">Python</A>
                <DT><A HREF="javascript:alert(1)">Ignored</A>
            </DL><p>
            <DT><A HREF="https://uncategorized.test">Loose</A>
        </DL><p>
        """,
        encoding="utf-8",
    )

    page.goto(live_server)
    page.locator(".state-text").wait_for(state="visible")

    with page.expect_file_chooser() as chooser_info:
        page.locator(".floating-add-btn").click()
        page.get_by_title("导入书签").click()
    chooser_info.value.set_files(str(import_file))

    page.locator("#app-dialog-title", has_text="导入书签").wait_for()
    confirm_dialog(page)
    page.locator("#app-dialog-title", has_text="导入完成").wait_for()
    confirm_dialog(page)

    assert page.locator("#current-folder-title").inner_text() == "本次新增"
    assert bookmark_row(page, "Python").count() == 1
    assert bookmark_row(page, "Loose").count() == 1
    assert bookmark_row(page, "Ignored").count() == 0


def test_mobile_folder_drawer_opens_and_closes(page, live_server):
    seed_bookmark(live_server, {"id": "a", "title": "Mobile Site", "url": "mobile.test", "folder": "Mobile"})

    page.set_viewport_size({"width": 390, "height": 844})
    page.goto(live_server)
    bookmark_row(page, "Mobile Site").wait_for()

    page.locator("#folder-fab").click()
    assert "open" in page.locator(".sidebar").get_attribute("class")
    page.locator("#drawer-scrim").click()
    assert "open" not in page.locator(".sidebar").get_attribute("class")
