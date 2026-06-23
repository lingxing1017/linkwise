async function loadWebdavConfig() {
    const res = await fetch(`${API_BASE}/webdav/config`);
    const result = await parseApiJson(res, '获取设置失败');

    if (!res.ok || result.status !== 'success') {
        throw new Error(result.message || '获取设置失败');
    }

    const config = result.config || {};

    if (webdavUrlInput) {
        webdavUrlInput.value = config.webdav_url || '';
    }

    if (webdavUsernameInput) {
        webdavUsernameInput.value = config.username || '';
    }

    if (webdavPasswordInput) {
        webdavPasswordInput.value = '';
        webdavPasswordInput.placeholder = config.has_password
            ? '不填写则保留已保存密码'
            : 'WebDAV 密码或 App Password';
    }

    if (webdavRemoteDirInput) {
        webdavRemoteDirInput.value = config.remote_dir || '';
    }

    if (webdavFilenameInput) {
        webdavFilenameInput.value = config.filename || 'linkwise-bookmarks.html';
    }
}

function getWebdavConfigPayload() {
    return {
        webdav_url: webdavUrlInput ? webdavUrlInput.value.trim() : '',
        username: webdavUsernameInput ? webdavUsernameInput.value.trim() : '',
        password: webdavPasswordInput ? webdavPasswordInput.value : '',
        remote_dir: webdavRemoteDirInput ? webdavRemoteDirInput.value.trim() : '',
        filename: webdavFilenameInput ? webdavFilenameInput.value.trim() : ''
    };
}

window.openSettings = async function() {
    if (!requireAdminUiAction()) return;

    openOverlay(settingsOverlay);
    closeFloatingMenu();

    try {
        await Promise.all([
            loadWebdavConfig(),
            loadAuthManagement()
        ]);
    } catch (err) {
        console.error('获取设置失败:', err);
        await showMessage(`获取设置失败：${err.message}`, '设置');
    }
};

window.closeSettings = function() {
    closeOverlay(settingsOverlay);
};

window.saveWebdavConfig = async function() {
    if (!requireAdminUiAction()) return;

    try {
        const res = await fetch(`${API_BASE}/webdav/config`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify(getWebdavConfigPayload())
        });

        const result = await parseApiJson(res, '保存设置失败');

        if (!res.ok || result.status !== 'success') {
            await showMessage(result.message || '保存设置失败', '设置');
            return;
        }

        if (webdavPasswordInput) {
            webdavPasswordInput.value = '';
            webdavPasswordInput.placeholder = result.config?.has_password
                ? '不填写则保留已保存密码'
                : 'WebDAV 密码或 App Password';
        }

        await showMessage('WebDAV 配置已保存。', '设置');
    } catch (err) {
        console.error('保存设置失败:', err);
        await showMessage(`保存设置失败：${err.message}`, '设置');
    }
};
