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

    if (webdavFilenameInput) {
        webdavFilenameInput.value = config.filename || 'linkwise-bookmarks.html';
    }

    syncWebdavStatus(config);
}

function syncWebdavStatus(config = getWebdavConfigPayload()) {
    const hasConfig = Boolean(config.webdav_url && config.username);
    const statusSwitch = document.getElementById('webdav-status-switch');
    const statusTitle = document.getElementById('webdav-status-title');
    const statusSubtitle = document.getElementById('webdav-status-subtitle');
    const statusMeta = document.getElementById('webdav-status-meta');

    if (statusSwitch) {
        statusSwitch.classList.toggle('active', hasConfig);
    }

    if (statusTitle) {
        statusTitle.textContent = hasConfig ? '已配置 WebDAV 同步' : 'WebDAV 同步未配置';
    }

    if (statusSubtitle) {
        statusSubtitle.textContent = hasConfig
            ? '保存后将使用当前 WebDAV 配置'
            : '填写地址和用户名后即可保存配置';
    }

    if (statusMeta) {
        statusMeta.textContent = hasConfig ? '配置已读取' : '等待配置';
    }
}

function getWebdavConfigPayload() {
    return {
        webdav_url: webdavUrlInput ? webdavUrlInput.value.trim() : '',
        username: webdavUsernameInput ? webdavUsernameInput.value.trim() : '',
        password: webdavPasswordInput ? webdavPasswordInput.value : '',
        filename: webdavFilenameInput ? webdavFilenameInput.value.trim() : ''
    };
}

window.switchSettingsTab = function(tabName) {
    const activeName = tabName === 'auth' ? 'auth' : 'backup';

    document.querySelectorAll('.settings-tab').forEach((tab) => {
        const active = tab.id === `settings-tab-${activeName}`;
        tab.classList.toggle('active', active);
        tab.setAttribute('aria-selected', active ? 'true' : 'false');
    });

    document.querySelectorAll('.settings-panel').forEach((panel) => {
        const active = panel.id === `settings-panel-${activeName}`;
        panel.classList.toggle('active', active);
        panel.hidden = !active;
    });
};

window.openSettings = async function() {
    if (!requireAdminUiAction()) return;

    openOverlay(settingsOverlay);
    switchSettingsTab('backup');
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

        syncWebdavStatus(result.config);
        await showMessage('WebDAV 配置已保存。', '设置');
    } catch (err) {
        console.error('保存设置失败:', err);
        await showMessage(`保存设置失败：${err.message}`, '设置');
    }
};

window.testWebdavConnection = async function() {
    await showMessage('当前版本仅支持保存 WebDAV 配置，连接测试接口尚未接入。', '测试连接');
};

window.backupWebdavNow = async function() {
    await showMessage('当前版本仅支持保存 WebDAV 配置，立即备份接口尚未接入。', '立即备份');
};
