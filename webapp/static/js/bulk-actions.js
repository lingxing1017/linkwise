window.toggleVisibleBookmarkSelection = function(checked) {
    if (!requireAdminUiAction()) return;

    for (const id of getCurrentPageSelectableBookmarkIds()) {
        if (checked) {
            selectedBookmarkIds.add(id);
        } else {
            selectedBookmarkIds.delete(id);
        }
    }

    updateBulkMoveBar();
    renderCards();
};

window.moveSelectedBookmarks = async function() {
    if (!requireAdminUiAction()) return;

    const ids = getSelectedBookmarkIds();

    if (ids.length === 0) {
        await showMessage('请选择要移动的书签。');
        return;
    }

    if (moveBookmarkCount) {
        moveBookmarkCount.textContent = ids.length;
    }

    if (bulkMoveFolder) {
        bulkMoveFolder.value = '';
    }

    openOverlay(bulkMoveOverlay);
    setTimeout(() => bulkMoveFolder && bulkMoveFolder.focus(), 0);
};

window.closeBulkMoveDialog = function() {
    closeOverlay(bulkMoveOverlay);
};

window.confirmSelectedBookmarksMove = async function() {
    if (!requireAdminUiAction()) return;

    const ids = getSelectedBookmarkIds();

    if (ids.length === 0) {
        closeBulkMoveDialog();
        await showMessage('请选择要移动的书签。');
        return;
    }

    const folder = normalizeFolderPath(bulkMoveFolder ? bulkMoveFolder.value : '');
    const targetName = folder || '全部书签';
    const confirmed = await showConfirm(`确认将 ${ids.length} 个书签移动到「${targetName}」吗？`, {
        title: '移动书签',
        confirmText: '移动'
    });

    if (!confirmed) return;

    closeBulkMoveDialog();

    try {
        const res = await fetch(`${API_BASE}/bookmarks/move`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                ids,
                folder
            })
        });

        const result = await parseApiJson(res, '移动失败');

        if (!res.ok || result.status !== 'success') {
            await showMessage(result.message || '移动失败', '移动失败');
            return;
        }

        selectedBookmarkIds.clear();

        if (bulkMoveFolder) {
            bulkMoveFolder.value = '';
        }

        selectedFolder = result.folder || ALL_BOOKMARKS_VIEW;
        ensureParentFoldersExpanded(selectedFolder);
        await fetchList();
        await showMessage(`已移动 ${result.moved_count} 个书签。`, '移动完成');
    } catch (err) {
        console.error('移动书签失败:', err);
        await showMessage(`移动失败：${err.message}`, '移动失败');
    }
};

window.deleteSelectedBookmarks = async function() {
    if (!requireAdminUiAction()) return;

    const ids = getSelectedBookmarkIds();

    if (ids.length === 0) {
        await showMessage('请选择要删除的书签。');
        return;
    }

    const confirmed = await showConfirm(`确认删除选中的 ${ids.length} 个书签吗？这个操作不可恢复。`, {
        title: '删除书签',
        confirmText: '删除',
        danger: true
    });

    if (!confirmed) return;

    try {
        const res = await fetch(`${API_BASE}/bookmarks/delete`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({ ids })
        });

        const result = await parseApiJson(res, '删除失败');

        if (!res.ok || result.status !== 'success') {
            await showMessage(result.message || '删除失败', '删除失败');
            return;
        }

        selectedBookmarkIds.clear();
        await fetchList();
        await showMessage(`已删除 ${result.deleted_count} 个书签。`, '删除完成');
    } catch (err) {
        console.error('批量删除失败:', err);
        await showMessage(`删除失败：${err.message}`, '删除失败');
    }
};
