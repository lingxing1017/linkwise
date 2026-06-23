window.openBox = function() {
    if (!requireAdminUiAction()) return;

    boxTitle.innerText = '创建新书签';
    document.getElementById('bookmark-id').value = '';
    form.reset();
    openOverlay(overlay);
    closeFloatingMenu();
};

window.openImportFile = function() {
    if (!requireAdminUiAction()) return;

    closeFloatingMenu();
    document.getElementById('file-import').click();
};

window.closeBox = function() {
    closeOverlay(overlay);
};

window.editItem = function(event, id) {
    event.stopPropagation();
    if (!requireAdminUiAction()) return;

    const target = bookmarks.find((bookmark) => String(bookmark.id) === String(id));
    if (!target) return;

    fillBookmarkForm(target);
};

function fillBookmarkForm(bookmark) {
    boxTitle.innerText = '编辑书签';
    document.getElementById('bookmark-id').value = bookmark.id;
    document.getElementById('title').value = bookmark.title || '';
    document.getElementById('url').value = bookmark.url || '';
    document.getElementById('folder').value = bookmark.folder || '';
    openOverlay(overlay);
}

window.removeItem = async function(event, id) {
    event.stopPropagation();
    if (!requireAdminUiAction()) return;

    const shouldDelete = await showConfirm('确认删除这个书签吗？', {
        title: '删除书签',
        confirmText: '删除',
        danger: true
    });

    if (!shouldDelete) return;

    try {
        const res = await fetch(`${API_BASE}/bookmarks/${encodeURIComponent(id)}`, {
            method: 'DELETE'
        });

        const result = await parseApiJson(res, '删除失败');

        if (!res.ok || result.status !== 'success') {
            await showMessage(result.message || '删除失败', '删除失败');
            return;
        }

        await fetchList();
    } catch (err) {
        console.error('删除失败:', err);
        await showMessage('删除失败，请检查后端服务', '删除失败');
    }
};

window.toggleFloatingMenu = function(event) {
    event.stopPropagation();
    if (!requireAdminUiAction()) return;

    if (!floatingMenu) return;

    const shouldOpen = !floatingMenu.classList.contains('open');
    floatingMenu.classList.toggle('open', shouldOpen);
    floatingMenu.classList.remove('hover');
    closeTopbarMoreMenu();

    if (event.currentTarget) {
        event.currentTarget.blur();
    }
};

function closeFloatingMenu() {
    if (floatingMenu) {
        floatingMenu.classList.remove('open', 'hover');
    }
}

window.toggleTopbarMoreMenu = function(event) {
    event.stopPropagation();

    if (!topbarMoreMenu) return;

    const shouldOpen = !topbarMoreMenu.classList.contains('open');
    topbarMoreMenu.classList.toggle('open', shouldOpen);
    closeFloatingMenu();

    if (event.currentTarget) {
        event.currentTarget.blur();
    }
};

function closeTopbarMoreMenu() {
    if (topbarMoreMenu) {
        topbarMoreMenu.classList.remove('open');
    }

    if (densityMenu) {
        densityMenu.classList.remove('submenu-open');
    }
}
