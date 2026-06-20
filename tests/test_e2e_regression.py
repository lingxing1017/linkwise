import json
import time
import urllib.request

import pytest


pytestmark = pytest.mark.e2e


def unique_id(label):
    return f"e2e-{label}-{time.time_ns()}"


def seed_bookmark(base_url, payload):
    request = urllib.request.Request(
        f"{base_url}/api/bookmarks",
        data=json.dumps(payload).encode("utf-8"),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(request, timeout=10) as response:
        return json.loads(response.read().decode("utf-8"))


def bookmark_row(page, title):
    return page.locator(".bookmark-row", has=page.locator(".bookmark-title", has_text=title))


def confirm_dialog(page):
    page.locator("#app-dialog-confirm").click()


def open_add_bookmark(page):
    page.locator(".floating-add-btn").click()
    page.get_by_title("添加书签").click()


def test_create_and_search_bookmark(page, live_server):
    marker = unique_id("create")
    title = f"Worker Docs {marker}"
    url = f"https://{marker}.example.test"

    page.goto(live_server)
    page.locator("#cards-wrapper").wait_for()

    open_add_bookmark(page)
    page.locator("#title").fill(title)
    page.locator("#url").fill(url)
    page.locator("#folder").fill(f"E2E / {marker}")
    page.locator("#bookmark-form .btn-save").click()

    bookmark_row(page, title).wait_for()
    page.locator("#search").fill(marker)
    assert bookmark_row(page, title).count() == 1


def test_duplicate_url_can_open_original_for_edit(page, live_server):
    marker = unique_id("duplicate")
    title = f"Original Site {marker}"
    url = f"https://{marker}.example.test"

    seed_bookmark(
        live_server,
        {"id": marker, "title": title, "url": url, "folder": "E2E"},
    )

    page.goto(live_server)
    page.locator("#search").fill(marker)
    bookmark_row(page, title).wait_for()

    open_add_bookmark(page)
    page.locator("#title").fill(f"Duplicate Site {marker}")
    page.locator("#url").fill(url)
    page.locator("#bookmark-form .btn-save").click()

    page.locator("#app-dialog-title", has_text="书签已存在").wait_for()
    confirm_dialog(page)

    assert page.locator("#box-title").inner_text() == "编辑书签"
    assert page.locator("#bookmark-id").input_value() == marker
    assert page.locator("#title").input_value() == title


def test_bulk_move_and_delete_selected_bookmarks(page, live_server):
    marker = unique_id("bulk")
    alpha = f"Alpha {marker}"
    beta = f"Beta {marker}"

    seed_bookmark(live_server, {"id": f"{marker}-a", "title": alpha, "url": f"https://a-{marker}.test", "folder": f"Inbox / {marker}"})
    seed_bookmark(live_server, {"id": f"{marker}-b", "title": beta, "url": f"https://b-{marker}.test", "folder": f"Inbox / {marker}"})

    page.goto(live_server)
    page.locator("#search").fill(marker)
    bookmark_row(page, alpha).wait_for()
    bookmark_row(page, alpha).locator(".bookmark-select input").check()
    bookmark_row(page, beta).locator(".bookmark-select input").check()
    assert page.locator("#bulk-selected-count").inner_text() == "2"

    page.locator(".bulk-mini-btn", has_text="移动").click()
    page.locator("#bulk-move-folder").fill(f"Archive / {marker}")
    page.locator("#bulk-move-overlay .btn-save").click()
    confirm_dialog(page)
    page.locator("#app-dialog-title", has_text="移动完成").wait_for()
    confirm_dialog(page)

    bookmark_row(page, alpha).locator(".bookmark-select input").check()
    page.locator(".bulk-mini-btn.danger", has_text="删除").click()
    confirm_dialog(page)
    page.locator("#app-dialog-title", has_text="删除完成").wait_for()
    confirm_dialog(page)

    assert bookmark_row(page, alpha).count() == 0
    assert bookmark_row(page, beta).count() == 1


def test_import_bookmarks_html_shows_last_import_view(page, live_server, tmp_path):
    marker = unique_id("import")
    import_file = tmp_path / "bookmarks.html"
    import_file.write_text(
        f"""
        <!DOCTYPE NETSCAPE-Bookmark-file-1>
        <DL><p>
            <DT><H3>E2E {marker}</H3>
            <DL><p>
                <DT><A HREF="https://python-{marker}.example.test">Python {marker}</A>
                <DT><A HREF="javascript:alert(1)">Ignored {marker}</A>
                <DT><A HREF="chrome://settings">Chrome Settings {marker}</A>
                <DT><A HREF="moz-extension://abcdef/sidebar.html">Firefox Extension {marker}</A>
                <DT><A HREF="view-source:https://example.test">Page Source {marker}</A>
            </DL><p>
            <DT><A HREF="https://loose-{marker}.example.test">Loose {marker}</A>
        </DL><p>
        """,
        encoding="utf-8",
    )

    page.goto(live_server)
    page.locator("#cards-wrapper").wait_for()

    with page.expect_file_chooser() as chooser_info:
        page.locator(".floating-add-btn").click()
        page.get_by_title("导入书签").click()
    chooser_info.value.set_files(str(import_file))

    page.locator("#app-dialog-title", has_text="导入书签").wait_for()
    confirm_dialog(page)
    page.locator("#app-dialog-title", has_text="导入完成").wait_for()
    confirm_dialog(page)

    assert page.locator("#current-folder-title").inner_text() == "本次新增"
    assert bookmark_row(page, f"Python {marker}").count() == 1
    assert bookmark_row(page, f"Loose {marker}").count() == 1
    assert bookmark_row(page, f"Ignored {marker}").count() == 0
    assert bookmark_row(page, f"Chrome Settings {marker}").count() == 0
    assert bookmark_row(page, f"Firefox Extension {marker}").count() == 0
    assert bookmark_row(page, f"Page Source {marker}").count() == 0


def test_mobile_folder_drawer_opens_and_closes(page, live_server):
    marker = unique_id("mobile")
    title = f"Mobile Site {marker}"
    seed_bookmark(live_server, {"id": marker, "title": title, "url": f"https://{marker}.test", "folder": f"Mobile / {marker}"})

    page.set_viewport_size({"width": 390, "height": 844})
    page.goto(live_server)
    page.locator("#search").fill(marker)
    bookmark_row(page, title).wait_for()

    page.locator("#folder-fab").click()
    assert "open" in page.locator(".sidebar").get_attribute("class")
    page.locator("#drawer-scrim").click()
    assert "open" not in page.locator(".sidebar").get_attribute("class")
