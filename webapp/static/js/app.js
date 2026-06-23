document.addEventListener('click', function(event) {
    if (floatingMenu && !floatingMenu.contains(event.target)) {
        closeFloatingMenu();
    }

    if (topbarMoreMenu && !topbarMoreMenu.contains(event.target)) {
        closeTopbarMoreMenu();
    }

    if (
        folderSuggestionPopup &&
        !folderSuggestionPopup.contains(event.target) &&
        !getFolderSuggestionInputs().includes(event.target)
    ) {
        closeFolderSuggestions();
    }
});

if (folderSuggestionPopup) {
    folderSuggestionPopup.addEventListener('mousedown', (event) => {
        const item = event.target.closest('.folder-suggestion-item');
        if (!item || !activeFolderSuggestionInput) return;

        event.preventDefault();
        activeFolderSuggestionInput.value = item.dataset.folder || '';
        activeFolderSuggestionInput.focus();
        closeFolderSuggestions();
    });
}

if (floatingMenu) {
    floatingMenu.addEventListener('mouseenter', () => {
        if (!floatingMenu.classList.contains('open')) {
            floatingMenu.classList.add('hover');
        }
    });

    floatingMenu.addEventListener('mouseleave', () => {
        floatingMenu.classList.remove('hover');
    });
}

if (settingsOverlay) {
    settingsOverlay.addEventListener('click', (event) => {
        if (event.target === settingsOverlay) {
            closeSettings();
        }
    });
}

if (bulkMoveOverlay) {
    bulkMoveOverlay.addEventListener('click', (event) => {
        if (event.target === bulkMoveOverlay) {
            closeBulkMoveDialog();
        }
    });
}

if (renameFolderInput) {
    renameFolderInput.addEventListener('keydown', (event) => {
        if (event.key === 'Enter') {
            event.preventDefault();
            confirmFolderRename();
        }
    });
}

if (bulkMoveFolder) {
    bulkMoveFolder.addEventListener('keydown', (event) => {
        if (event.key === 'Enter') {
            event.preventDefault();
            confirmSelectedBookmarksMove();
        }
    });
}

if (bulkSelectAll) {
    bulkSelectAll.addEventListener('change', () => {
        toggleVisibleBookmarkSelection(bulkSelectAll.checked);
    });
}

if (bulkMiniSelectAll) {
    bulkMiniSelectAll.addEventListener('change', () => {
        toggleVisibleBookmarkSelection(bulkMiniSelectAll.checked);
    });
}

for (const input of getFolderSuggestionInputs()) {
    setupFolderSuggestionInput(input);
}

window.addEventListener('resize', () => {
    syncTopbarHeight();
    updateBulkMiniBar();

    if (activeFolderSuggestionInput) {
        positionFolderSuggestions(activeFolderSuggestionInput);
    }
});

window.addEventListener('scroll', () => {
    updateBulkMiniBar();

    if (activeFolderSuggestionInput) {
        positionFolderSuggestions(activeFolderSuggestionInput);
    }
}, true);

document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape' && folderSuggestionPopup && folderSuggestionPopup.classList.contains('show')) {
        closeFolderSuggestions();
        return;
    }

    if (event.key === 'Escape' && sidebar && sidebar.classList.contains('open')) {
        closeFolderDrawer();
        return;
    }

    if (event.key === 'Escape' && topbarMoreMenu && topbarMoreMenu.classList.contains('open')) {
        closeTopbarMoreMenu();
        return;
    }

    if (event.key === 'Escape' && isAppDialogOpen()) {
        closeAppDialog(false);
        return;
    }

    if (event.key === 'Escape' && isOverlayOpen(settingsOverlay)) {
        closeSettings();
        return;
    }

    if (event.key === 'Escape' && isOverlayOpen(bulkMoveOverlay)) {
        closeBulkMoveDialog();
    }
});

form.addEventListener('submit', async function(event) {
    event.preventDefault();

    if (!requireAdminUiAction()) return;

    const id = document.getElementById('bookmark-id').value || Date.now().toString();
    const title = document.getElementById('title').value.trim();
    const url = document.getElementById('url').value.trim();
    const folder = document.getElementById('folder').value.trim();

    try {
        const res = await fetch(`${API_BASE}/bookmarks`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                id,
                title,
                url,
                folder
            })
        });

        const result = await parseApiJson(res, '保存失败');

        if (res.status === 409 && result.error === 'duplicate_url') {
            const existing = result.bookmark;
            const shouldEdit = await showConfirm('这个 URL 已存在，要编辑原书签吗？', {
                title: '书签已存在',
                confirmText: '编辑原书签'
            });

            if (shouldEdit && existing) {
                fillBookmarkForm(existing);
            }

            return;
        }

        if (!res.ok || result.status !== 'success') {
            await showMessage(result.message || '保存失败', '保存失败');
            return;
        }

        closeBox();
        await fetchList();
    } catch (err) {
        console.error('保存失败:', err);
        await showMessage('保存失败，请检查后端服务', '保存失败');
    }
});

search.addEventListener('input', renderCards);

if (darkModeQuery) {
    darkModeQuery.addEventListener('change', syncAuthUi);
}

applyBookmarkDensity(getStoredBookmarkDensity());
syncTopbarHeight();

(async function initApp() {
    await refreshAuthStatus();
    await fetchList();
})();
