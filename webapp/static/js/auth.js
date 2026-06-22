function isAdminUnlocked() {
    return Boolean(authState && authState.admin_unlocked);
}

function isReadOnlyMode() {
    return !isAdminUnlocked();
}

function getAuthActionLabel() {
    if (authState.admin_unlocked) {
        if (authState.admin_session_expires_at) {
            const remaining = Math.max(0, authState.admin_session_expires_at - Math.floor(Date.now() / 1000));
            const minutes = Math.floor(remaining / 60);
            const seconds = remaining % 60;
            return `管理模式 · ${minutes}:${String(seconds).padStart(2, '0')}`;
        }

        return '管理模式';
    }

    return authState.admin_initialized ? '解锁管理模式' : '初始化管理权限';
}

function syncAuthUi() {
    document.body.classList.toggle('readonly-mode', isReadOnlyMode());

    if (authActionButton) {
        authActionButton.textContent = getAuthActionLabel();
    }

    if (isReadOnlyMode()) {
        selectedBookmarkIds.clear();
        closeFloatingMenu();
        closeBulkMoveDialog();
    }

    renderFolders();
    renderCards();
    updateBulkMoveBar();
}

async function refreshAuthStatus() {
    try {
        const res = await fetch(`${API_BASE}/auth/status`);
        const status = await parseApiJson(res, '认证状态获取失败');

        if (res.ok) {
            authState = {
                ...authState,
                ...status
            };
        }
    } catch (err) {
        console.error('认证状态获取失败:', err);
        authState = {
            ...authState,
            admin_initialized: false,
            admin_unlocked: false
        };
    }

    syncAuthUi();
}

window.handleAuthAction = async function() {
    if (authState.admin_unlocked) {
        await showMessage('管理模式已解锁。', '管理模式');
        return;
    }

    await showMessage('Passkey 初始化和解锁界面将在下一阶段接入。', getAuthActionLabel());
};
