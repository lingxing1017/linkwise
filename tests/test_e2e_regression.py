import json
import time
import urllib.request

import pytest


pytestmark = pytest.mark.e2e
requires_admin_session = pytest.mark.skip(
    reason="requires Passkey-authenticated admin session test fixture"
)


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


@requires_admin_session
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


@requires_admin_session
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


@requires_admin_session
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


@requires_admin_session
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
    page.set_viewport_size({"width": 390, "height": 844})
    page.goto(live_server)
    page.locator("#cards-wrapper").wait_for()

    page.locator("#folder-fab").click()
    assert "open" in page.locator(".sidebar").get_attribute("class")
    page.locator("#drawer-scrim").click()
    assert "open" not in page.locator(".sidebar").get_attribute("class")


def test_readonly_auth_icon_replaces_management_button(page, live_server):
    page.goto(live_server)
    page.locator("#cards-wrapper").wait_for()

    assert page.locator("#brand-auth-btn").count() == 1
    assert page.locator("#auth-action-btn").count() == 0
    assert (
        page.locator(".floating-add-btn").is_hidden()
        or "readonly-mode" in page.locator("body").get_attribute("class")
    )
    assert page.locator("#bookmark-list-header").is_visible()
    assert page.locator("#bookmark-list-header .bulk-select-all").is_hidden()
    assert page.locator("#bookmark-list-header .bookmark-header-actions").is_hidden()


def test_readonly_ui_suppresses_management_controls(page, live_server):
    page.goto(live_server)
    page.locator("#cards-wrapper").wait_for()

    assert "readonly-mode" in page.locator("body").get_attribute("class")
    assert page.locator(".floating-menu").is_hidden()
    assert page.locator(".bulk-mini-bar").is_hidden()
    assert page.locator("#bookmark-list-header .bulk-select-all").is_hidden()
    assert page.locator("#bookmark-list-header .bookmark-header-actions").is_hidden()
    assert page.locator(".folder-actions").count() == 0


def test_mocked_passkey_unlock_reveals_management_ui(page, live_server):
    state = {"unlocked": False}
    calls = []

    def route_api(route):
        url = route.request.url
        calls.append(url)

        if url.endswith("/api/auth/status"):
            route.fulfill(
                status=200,
                content_type="application/json",
                body=json.dumps(
                    {
                        "public_read": True,
                        "admin_initialized": True,
                        "admin_unlocked": state["unlocked"],
                        "admin_session_expires_at": int(time.time()) + 3600 if state["unlocked"] else None,
                        "auth_configured": True,
                        "missing_config": [],
                    }
                ),
            )
            return

        if url.endswith("/api/auth/passkey/login/options"):
            route.fulfill(
                status=200,
                content_type="application/json",
                body=json.dumps(
                    {
                        "publicKey": {
                            "challenge": "AQID",
                            "rpId": "127.0.0.1",
                            "allowCredentials": [{"type": "public-key", "id": "AQID"}],
                            "timeout": 300000,
                            "userVerification": "preferred",
                        }
                    }
                ),
            )
            return

        if url.endswith("/api/auth/passkey/login/verify"):
            state["unlocked"] = True
            route.fulfill(
                status=200,
                content_type="application/json",
                body=json.dumps(
                    {
                        "ok": True,
                        "admin_unlocked": True,
                        "expires_at": int(time.time()) + 3600,
                    }
                ),
            )
            return

        route.continue_()

    page.route("**/api/**", route_api)
    page.goto(live_server)
    page.locator("#cards-wrapper").wait_for()
    page.evaluate(
        """
        () => {
            Object.defineProperty(navigator.credentials, "get", {
                configurable: true,
                value: async () => ({
                    id: "mock-credential",
                    rawId: new Uint8Array([1, 2, 3]).buffer,
                    type: "public-key",
                    response: {
                        clientDataJSON: new Uint8Array([4]).buffer,
                        authenticatorData: new Uint8Array([5]).buffer,
                        signature: new Uint8Array([6]).buffer
                    }
                })
            });
        }
        """
    )

    assert "readonly-mode" in page.locator("body").get_attribute("class")
    assert page.evaluate("() => navigator.credentials.get.toString().includes('mock-credential')")

    page.locator("#brand-auth-btn").click()
    page.locator("#app-dialog-confirm").click()
    page.wait_for_timeout(500)
    assert any(url.endswith("/api/auth/passkey/login/options") for url in calls)
    assert any(url.endswith("/api/auth/passkey/login/verify") for url in calls)
    page.wait_for_function("() => !document.body.classList.contains('readonly-mode')")

    assert page.locator(".floating-menu").is_visible()


def test_unlock_can_send_session_duration(page, live_server):
    captured = {}
    state = {"unlocked": False}

    def route_api(route):
        url = route.request.url

        if url.endswith("/api/auth/status"):
            route.fulfill(
                status=200,
                content_type="application/json",
                body=json.dumps(
                    {
                        "public_read": True,
                        "admin_initialized": True,
                        "admin_unlocked": state["unlocked"],
                        "admin_session_expires_at": int(time.time()) + 86400 if state["unlocked"] else None,
                        "auth_configured": True,
                        "missing_config": [],
                    }
                ),
            )
            return

        if url.endswith("/api/auth/passkey/login/options"):
            route.fulfill(
                status=200,
                content_type="application/json",
                body=json.dumps(
                    {
                        "publicKey": {
                            "challenge": "AQID",
                            "rpId": "127.0.0.1",
                            "allowCredentials": [],
                            "timeout": 300000,
                            "userVerification": "preferred",
                        }
                    }
                ),
            )
            return

        if url.endswith("/api/auth/passkey/login/verify"):
            captured["body"] = route.request.post_data_json
            state["unlocked"] = True
            route.fulfill(
                status=200,
                content_type="application/json",
                body=json.dumps(
                    {
                        "ok": True,
                        "admin_unlocked": True,
                        "expires_at": int(time.time()) + 86400,
                    }
                ),
            )
            return

        route.continue_()

    page.route("**/api/**", route_api)
    page.goto(live_server)
    page.locator("#cards-wrapper").wait_for()
    page.evaluate(
        """
        () => {
            Object.defineProperty(navigator.credentials, "get", {
                configurable: true,
                value: async () => ({
                    id: "mock-credential",
                    rawId: new Uint8Array([1]).buffer,
                    type: "public-key",
                    response: {
                        clientDataJSON: new Uint8Array([1]).buffer,
                        authenticatorData: new Uint8Array([2]).buffer,
                        signature: new Uint8Array([3]).buffer
                    }
                })
            });
        }
        """
    )

    page.locator("#brand-auth-btn").click()
    page.locator("#auth-session-duration").select_option("86400")
    page.locator("#app-dialog-confirm").click()
    page.wait_for_function("() => !document.body.classList.contains('readonly-mode')")

    assert captured["body"]["session_max_age_seconds"] == 86400


def test_readonly_bookmark_rows_keep_reading_layout(page, live_server):
    page.goto(live_server)
    page.locator("#cards-wrapper").wait_for()

    page.evaluate(
        """
        () => {
            authState = {
                public_read: true,
                admin_initialized: true,
                admin_unlocked: false,
                admin_session_expires_at: null,
                auth_configured: true,
                missing_config: []
            };
            bookmarks = [{
                id: "readonly-row",
                title: "Readonly Bookmark Title",
                url: "https://readonly.example.test/path",
                folder: "Other Bookmarks / android"
            }];
            selectedFolder = ALL_BOOKMARKS_VIEW;
            syncAuthUi();
        }
        """
    )

    row = page.locator(".bookmark-row", has=page.locator(".bookmark-title", has_text="Readonly Bookmark Title"))
    row.wait_for()

    assert row.locator(".bookmark-title").is_visible()
    assert row.locator(".bookmark-domain").is_visible()
    assert row.locator(".bookmark-folder").is_visible()
    assert row.locator(".bookmark-select").count() == 0
    assert row.locator(".bookmark-actions").count() == 0


def test_settings_dialog_stays_inside_viewport(page, live_server):
    page.goto(live_server)
    page.locator("#cards-wrapper").wait_for()

    page.evaluate(
        """
        () => {
            openOverlay(settingsOverlay);
            switchSettingsTab("backup");
        }
        """
    )

    box_metrics = page.locator(".settings-box").evaluate(
        """
        (node) => {
            const rect = node.getBoundingClientRect();
            const style = getComputedStyle(node);
            return {
                top: rect.top,
                bottom: rect.bottom,
                viewportHeight: window.innerHeight,
                overflowY: style.overflowY
            };
        }
        """
    )
    assert box_metrics["top"] >= 0
    assert box_metrics["bottom"] <= box_metrics["viewportHeight"]
    assert box_metrics["overflowY"] == "hidden"

    assert page.locator("#settings-tab-backup").get_attribute("aria-selected") == "true"
    assert page.locator("#settings-panel-backup").is_visible()
    assert page.locator("#settings-panel-auth").is_hidden()
    assert page.locator("#settings-panel-backup .btn-save", has_text="保存配置").is_visible()
    assert page.locator(".settings-box > .dialog-actions .btn-save").count() == 0
    assert page.locator("#settings-panel-backup").evaluate(
        "node => getComputedStyle(node).overflowY"
    ) == "auto"
    fixed_regions = page.evaluate(
        """
        () => {
            const tabs = document.querySelector(".settings-tabs").getBoundingClientRect();
            const actions = document.querySelector(".settings-box > .dialog-actions").getBoundingClientRect();
            const panel = document.querySelector("#settings-panel-backup");
            panel.scrollTop = 120;
            const nextTabs = document.querySelector(".settings-tabs").getBoundingClientRect();
            const nextActions = document.querySelector(".settings-box > .dialog-actions").getBoundingClientRect();
            return {
                tabsTop: tabs.top,
                nextTabsTop: nextTabs.top,
                actionsTop: actions.top,
                nextActionsTop: nextActions.top
            };
        }
        """
    )
    assert fixed_regions["tabsTop"] == fixed_regions["nextTabsTop"]
    assert fixed_regions["actionsTop"] == fixed_regions["nextActionsTop"]

    page.locator("#settings-tab-auth").click()
    assert page.locator("#settings-tab-auth").get_attribute("aria-selected") == "true"
    assert page.locator("#settings-panel-auth").is_visible()
    assert page.locator("#settings-panel-backup").is_hidden()

    action_button_metrics = page.locator(".auth-management-actions .btn-save").evaluate(
        """
        (node) => {
            const rect = node.getBoundingClientRect();
            const style = getComputedStyle(node);
            return {
                height: rect.height,
                radius: style.borderRadius,
                borderStyle: style.borderStyle
            };
        }
        """
    )
    assert action_button_metrics["height"] >= 36
    assert action_button_metrics["radius"] != "0px"
    assert action_button_metrics["borderStyle"] == "none"


def test_unlocked_auth_icon_tooltip_has_only_time(page, live_server):
    page.goto(live_server)
    page.locator("#cards-wrapper").wait_for()

    page.evaluate(
        """
        () => {
            authState = {
                public_read: true,
                admin_initialized: true,
                admin_unlocked: true,
                admin_session_expires_at: Math.floor(Date.now() / 1000) + 872,
                auth_configured: true,
                missing_config: []
            };
            syncAuthUi();
        }
        """
    )

    assert page.locator("#brand-auth-tooltip").inner_text() == "14:32"
    assert page.locator("#brand-auth-btn").get_attribute("aria-label") == "锁定"
    assert "unlocked" in page.locator("#brand-auth-btn").get_attribute("class")
    assert "linkwise-unlocked-" in page.locator("#brand-auth-icon").get_attribute("src")


def test_expiring_auth_icon_shows_countdown_ring(page, live_server):
    page.goto(live_server)
    page.locator("#cards-wrapper").wait_for()

    page.evaluate(
        """
        () => {
            authState = {
                public_read: true,
                admin_initialized: true,
                admin_unlocked: true,
                admin_session_expires_at: Math.floor(Date.now() / 1000) + 120,
                auth_configured: true,
                missing_config: []
            };
            syncAuthUi();
        }
        """
    )

    assert "expiring" in page.locator("#brand-auth-btn").get_attribute("class")
    assert page.locator("#brand-auth-ring").evaluate(
        "node => node.style.getPropertyValue('--auth-ring-progress')"
    ) == "144deg"
