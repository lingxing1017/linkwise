function getSelectedBookmarkIds() {
    if (!isAdminUnlocked()) return [];

    return Array.from(selectedBookmarkIds);
}

function getBookmarkIdsInFolder(folderPath) {
    return bookmarks
        .filter((bookmark) => {
            const folder = String(bookmark.folder || '').trim();
            return folder === folderPath || folder.startsWith(folderPath + ' / ');
        })
        .map(bookmark => String(bookmark.id || ''));
}

function getFolderRowBookmarkIds(folderPath) {
    const ids = getBookmarkIdsInFolder(folderPath);

    if (selectedFolder === LAST_IMPORT_VIEW) {
        return ids.filter(id => lastImportedBookmarkIds.has(id));
    }

    return ids;
}

function getCurrentPageSelectableBookmarkIds() {
    if (!isAdminUnlocked()) return [];

    const ids = new Set(getVisibleBookmarks().map(bookmark => String(bookmark.id || '')));

    for (const folder of getChildFolders()) {
        for (const id of getFolderRowBookmarkIds(folder.path)) {
            ids.add(id);
        }
    }

    return Array.from(ids);
}

function updateBulkMoveBar() {
    if (!isAdminUnlocked()) {
        selectedBookmarkIds.clear();
    }

    const count = selectedBookmarkIds.size;
    const currentPageIds = getCurrentPageSelectableBookmarkIds();
    const currentPageSelectedCount = currentPageIds.filter(id => selectedBookmarkIds.has(id)).length;

    if (bulkSelectedCount) {
        bulkSelectedCount.textContent = count;
    }

    if (bulkMiniSelectedCount) {
        bulkMiniSelectedCount.textContent = count;
    }

    if (bookmarkListHeader) {
        setClassVisible(bookmarkListHeader, 'show', true);
    }

    if (bulkSelectAll) {
        bulkSelectAll.checked = currentPageIds.length > 0 && currentPageSelectedCount === currentPageIds.length;
        bulkSelectAll.indeterminate = currentPageSelectedCount > 0 && currentPageSelectedCount < currentPageIds.length;
        bulkSelectAll.disabled = currentPageIds.length === 0;
    }

    if (bulkMiniSelectAll) {
        bulkMiniSelectAll.checked = currentPageIds.length > 0 && currentPageSelectedCount === currentPageIds.length;
        bulkMiniSelectAll.indeterminate = currentPageSelectedCount > 0 && currentPageSelectedCount < currentPageIds.length;
        bulkMiniSelectAll.disabled = currentPageIds.length === 0;
    }

    if (bulkMoveFolder) {
        bulkMoveFolder.disabled = count === 0;
    }

    if (bulkMoveButton) {
        bulkMoveButton.disabled = count === 0;
    }

    if (bulkExportButton) {
        bulkExportButton.disabled = count === 0;
    }

    if (bulkDeleteButton) {
        bulkDeleteButton.disabled = count === 0;
    }

    updateBulkMiniBar();
}

function updateBulkMiniBar() {
    if (!bulkMiniBar) return;

    const count = selectedBookmarkIds.size;
    setClassVisible(bulkMiniBar, 'show', isAdminUnlocked() && count > 0);
}

window.clearBookmarkSelection = function() {
    selectedBookmarkIds.clear();
    updateBulkMoveBar();
    renderCards();
};

function pruneBookmarkSelection() {
    const existingIds = new Set(bookmarks.map(bookmark => String(bookmark.id || '')));

    for (const id of selectedBookmarkIds) {
        if (!existingIds.has(id)) {
            selectedBookmarkIds.delete(id);
        }
    }
}

function pruneLastImportedBookmarks() {
    const existingIds = new Set(bookmarks.map(bookmark => String(bookmark.id || '')));

    for (const id of lastImportedBookmarkIds) {
        if (!existingIds.has(id)) {
            lastImportedBookmarkIds.delete(id);
        }
    }

    if (selectedFolder === LAST_IMPORT_VIEW && lastImportedBookmarkIds.size === 0) {
        selectedFolder = ALL_BOOKMARKS_VIEW;
    }
}

function hasSearchKeyword() {
    return Boolean(String(search.value || '').trim());
}
