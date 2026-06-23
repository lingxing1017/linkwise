const AUTH_ICON_ASSETS = {
    locked: {
        light: 'static/img/linkwise-locked-light.png',
        dark: 'static/img/linkwise-locked-dark.png'
    },
    unlocked: {
        light: 'static/img/linkwise-unlocked-light.png',
        dark: 'static/img/linkwise-unlocked-dark.png'
    }
};
const AUTH_RING_THRESHOLD_SECONDS = 5 * 60;

function isAdminUnlocked() {
    return Boolean(authState && authState.admin_unlocked);
}

function isReadOnlyMode() {
    return !isAdminUnlocked();
}

function getAdminMode() {
    if (authState.admin_unlocked) return 'unlocked';
    return authState.admin_initialized ? 'locked' : 'uninitialized';
}

function getColorScheme() {
    return darkModeQuery && darkModeQuery.matches ? 'dark' : 'light';
}

function getRemainingAdminSeconds() {
    if (!authState.admin_unlocked || !authState.admin_session_expires_at) return null;
    return Math.max(0, authState.admin_session_expires_at - Math.floor(Date.now() / 1000));
}

function formatRemainingTime(seconds) {
    const safeSeconds = Math.max(0, Number(seconds) || 0);
    const minutes = Math.floor(safeSeconds / 60);
    const remainder = safeSeconds % 60;
    return `${minutes}:${String(remainder).padStart(2, '0')}`;
}

function syncAuthIcon() {
    const mode = getAdminMode();
    const scheme = getColorScheme();
    const iconState = mode === 'unlocked' ? 'unlocked' : 'locked';
    const remaining = getRemainingAdminSeconds();

    if (brandAuthIcon) {
        brandAuthIcon.src = AUTH_ICON_ASSETS[iconState][scheme];
    }

    if (brandAuthButton) {
        const label = mode === 'unlocked'
            ? '锁定'
            : mode === 'uninitialized'
                ? '初始化管理权限'
                : '解锁管理模式';
        brandAuthButton.setAttribute('aria-label', label);
        brandAuthButton.classList.toggle('unlocked', mode === 'unlocked');
        brandAuthButton.classList.toggle('expiring', Boolean(remaining !== null && remaining <= AUTH_RING_THRESHOLD_SECONDS));
    }

    if (brandAuthTooltip) {
        brandAuthTooltip.textContent = remaining === null ? '' : formatRemainingTime(remaining);
    }

    if (brandAuthRing) {
        const ratio = remaining === null || remaining > AUTH_RING_THRESHOLD_SECONDS
            ? 0
            : Math.max(0, Math.min(1, remaining / AUTH_RING_THRESHOLD_SECONDS));
        brandAuthRing.style.setProperty('--auth-ring-progress', `${ratio * 360}deg`);
    }
}

function syncAuthUi() {
    document.body.classList.toggle('readonly-mode', isReadOnlyMode());
    syncAuthIcon();

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
        await lockAdminSession();
        return;
    }

    if (!authState.auth_configured) {
        const missing = Array.isArray(authState.missing_config)
            ? authState.missing_config.join('、')
            : '认证配置';
        await showMessage(`请先配置 ${missing}，再初始化管理员 Passkey。`, '认证配置缺失');
        return;
    }

    if (authState.admin_initialized) {
        await unlockWithPasskey();
        return;
    }

    await initializeFirstPasskey();
};

async function initializeFirstPasskey() {
    const setupToken = window.prompt('请输入 LINKWISE_SETUP_TOKEN');
    if (!setupToken) return;

    const name = window.prompt('为这个 Passkey 命名', '我的 Passkey') || '我的 Passkey';
    await registerPasskey({ setupToken, name });
}

async function unlockWithPasskey() {
    try {
        const optionsRes = await fetch(`${API_BASE}/auth/passkey/login/options`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({})
        });
        const options = await parseApiJson(optionsRes, '解锁失败');

        if (!optionsRes.ok) {
            await showMessage(options.message || '解锁失败', '解锁管理模式');
            return;
        }

        const credential = await navigator.credentials.get({
            publicKey: decodeCredentialRequestOptions(options.publicKey)
        });

        if (!credential) return;

        const verifyRes = await fetch(`${API_BASE}/auth/passkey/login/verify`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                credential: publicKeyCredentialToJSON(credential)
            })
        });
        const result = await parseApiJson(verifyRes, '解锁失败');

        if (!verifyRes.ok || !result.ok) {
            await showMessage(result.message || '解锁失败', '解锁管理模式');
            return;
        }

        await refreshAuthStatus();
        await fetchList();
    } catch (err) {
        console.error('解锁失败:', err);
        await showMessage(`解锁失败：${err.message}`, '解锁管理模式');
    }
}

async function registerPasskey({ setupToken = null, name = '我的 Passkey' } = {}) {
    try {
        const optionsRes = await fetch(`${API_BASE}/auth/passkey/register/options`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                setup_token: setupToken,
                name
            })
        });
        const options = await parseApiJson(optionsRes, 'Passkey 注册失败');

        if (!optionsRes.ok) {
            await showMessage(options.message || 'Passkey 注册失败', 'Passkey');
            return;
        }

        const credential = await navigator.credentials.create({
            publicKey: decodeCredentialCreationOptions(options.publicKey)
        });

        if (!credential) return;

        const verifyRes = await fetch(`${API_BASE}/auth/passkey/register/verify`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                name,
                credential: publicKeyCredentialToJSON(credential)
            })
        });
        const result = await parseApiJson(verifyRes, 'Passkey 注册失败');

        if (!verifyRes.ok || !result.ok) {
            await showMessage(result.message || 'Passkey 注册失败', 'Passkey');
            return;
        }

        await refreshAuthStatus();
        await fetchList();
        await loadAuthManagement();
    } catch (err) {
        console.error('Passkey 注册失败:', err);
        await showMessage(`Passkey 注册失败：${err.message}`, 'Passkey');
    }
}

async function lockAdminSession() {
    const confirmed = await showConfirm('锁定', {
        title: '锁定',
        confirmText: '确定'
    });

    if (!confirmed) return;

    try {
        await fetch(`${API_BASE}/auth/logout`, { method: 'POST' });
    } catch (err) {
        console.error('锁定失败:', err);
    }

    await refreshAuthStatus();
    await fetchList();
}

window.addPasskey = async function() {
    if (!isAdminUnlocked()) return;

    const name = window.prompt('为新的 Passkey 命名', '新的 Passkey') || '新的 Passkey';
    await registerPasskey({ name });
};

window.deletePasskey = async function(credentialId) {
    if (!isAdminUnlocked()) return;

    const confirmed = await showConfirm('删除这个 Passkey 吗？关联会话也会被撤销。', {
        title: '删除 Passkey',
        confirmText: '删除',
        danger: true
    });

    if (!confirmed) return;

    const res = await fetch(`${API_BASE}/auth/passkeys/${encodeURIComponent(credentialId)}`, {
        method: 'DELETE'
    });
    const result = await parseApiJson(res, '删除 Passkey 失败');

    if (!res.ok || !result.ok) {
        await showMessage(result.message || '删除 Passkey 失败', 'Passkey');
        return;
    }

    await loadAuthManagement();
};

window.revokeSession = async function(sessionId) {
    if (!isAdminUnlocked()) return;

    const confirmed = await showConfirm('撤销这个会话吗？', {
        title: '撤销会话',
        confirmText: '撤销'
    });

    if (!confirmed) return;

    const res = await fetch(`${API_BASE}/auth/sessions/${encodeURIComponent(sessionId)}`, {
        method: 'DELETE'
    });
    const result = await parseApiJson(res, '撤销会话失败');

    if (!res.ok || !result.ok) {
        await showMessage(result.message || '撤销会话失败', '会话');
        return;
    }

    await refreshAuthStatus();
    await loadAuthManagement();
};

window.revokeAllSessions = async function() {
    if (!isAdminUnlocked()) return;

    const confirmed = await showConfirm('撤销所有会话吗？当前浏览器也会退出管理模式。', {
        title: '撤销全部会话',
        confirmText: '全部撤销',
        danger: true
    });

    if (!confirmed) return;

    const res = await fetch(`${API_BASE}/auth/sessions/revoke-all`, {
        method: 'POST'
    });
    const result = await parseApiJson(res, '撤销全部会话失败');

    if (!res.ok || !result.ok) {
        await showMessage(result.message || '撤销全部会话失败', '会话');
        return;
    }

    await refreshAuthStatus();
    closeSettings();
};

async function loadAuthManagement() {
    if (!isAdminUnlocked()) return;

    await Promise.all([
        loadPasskeys(),
        loadSessions()
    ]);
}

async function loadPasskeys() {
    if (!authPasskeyList) return;

    const res = await fetch(`${API_BASE}/auth/passkeys`);
    const result = await parseApiJson(res, '获取 Passkey 失败');

    if (!res.ok || !result.ok) {
        authPasskeyList.innerHTML = `<div class="auth-empty">${escapeHtml(result.message || '获取失败')}</div>`;
        return;
    }

    const passkeys = Array.isArray(result.passkeys) ? result.passkeys : [];
    authPasskeyList.innerHTML = passkeys.length
        ? passkeys.map((passkey) => `
            <div class="auth-list-item">
                <div>
                    <div class="auth-list-name">${escapeHtml(passkey.name || 'Passkey')}</div>
                    <div class="auth-list-meta">创建于 ${formatAuthTime(passkey.created_at)}</div>
                </div>
                <button type="button" class="row-btn icon-btn danger" onclick="deletePasskey('${escapeHtml(passkey.credential_id)}')">×</button>
            </div>
        `).join('')
        : '<div class="auth-empty">暂无 Passkey</div>';
}

async function loadSessions() {
    if (!authSessionList) return;

    const res = await fetch(`${API_BASE}/auth/sessions`);
    const result = await parseApiJson(res, '获取会话失败');

    if (!res.ok || !result.ok) {
        authSessionList.innerHTML = `<div class="auth-empty">${escapeHtml(result.message || '获取失败')}</div>`;
        return;
    }

    const sessions = Array.isArray(result.sessions) ? result.sessions : [];
    authSessionList.innerHTML = sessions.length
        ? sessions.map((session) => {
            const revoked = Boolean(session.revoked_at);
            return `
                <div class="auth-list-item ${revoked ? 'muted' : ''}">
                    <div>
                        <div class="auth-list-name">
                            ${escapeHtml(session.credential_name || '未知 Passkey')}
                            ${session.current ? '<span class="auth-pill">当前</span>' : ''}
                            ${revoked ? '<span class="auth-pill muted">已撤销</span>' : ''}
                        </div>
                        <div class="auth-list-meta">最近使用 ${formatAuthTime(session.last_seen_at)} · 过期 ${formatAuthTime(session.expires_at)}</div>
                    </div>
                    ${revoked ? '' : `<button type="button" class="row-btn icon-btn danger" onclick="revokeSession('${escapeHtml(session.id)}')">×</button>`}
                </div>
            `;
        }).join('')
        : '<div class="auth-empty">暂无会话</div>';
}

function decodeCredentialCreationOptions(publicKey) {
    return {
        ...publicKey,
        challenge: base64urlToArrayBuffer(publicKey.challenge),
        user: {
            ...publicKey.user,
            id: base64urlToArrayBuffer(publicKey.user.id)
        },
        excludeCredentials: (publicKey.excludeCredentials || []).map((item) => ({
            ...item,
            id: base64urlToArrayBuffer(item.id)
        }))
    };
}

function decodeCredentialRequestOptions(publicKey) {
    return {
        ...publicKey,
        challenge: base64urlToArrayBuffer(publicKey.challenge),
        allowCredentials: (publicKey.allowCredentials || []).map((item) => ({
            ...item,
            id: base64urlToArrayBuffer(item.id)
        }))
    };
}

function publicKeyCredentialToJSON(credential) {
    const response = credential.response;
    const json = {
        id: credential.id,
        rawId: arrayBufferToBase64url(credential.rawId),
        type: credential.type,
        response: {
            clientDataJSON: arrayBufferToBase64url(response.clientDataJSON)
        }
    };

    if (response.attestationObject) {
        json.response.attestationObject = arrayBufferToBase64url(response.attestationObject);
    }

    if (response.authenticatorData) {
        json.response.authenticatorData = arrayBufferToBase64url(response.authenticatorData);
    }

    if (response.signature) {
        json.response.signature = arrayBufferToBase64url(response.signature);
    }

    if (response.userHandle) {
        json.response.userHandle = arrayBufferToBase64url(response.userHandle);
    }

    return json;
}

function arrayBufferToBase64url(buffer) {
    const bytes = new Uint8Array(buffer);
    let binary = '';

    for (const byte of bytes) {
        binary += String.fromCharCode(byte);
    }

    return btoa(binary)
        .replace(/\+/g, '-')
        .replace(/\//g, '_')
        .replace(/=+$/g, '');
}

function base64urlToArrayBuffer(value) {
    const normalized = String(value || '')
        .replace(/-/g, '+')
        .replace(/_/g, '/');
    const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, '=');
    const binary = atob(padded);
    const bytes = new Uint8Array(binary.length);

    for (let index = 0; index < binary.length; index++) {
        bytes[index] = binary.charCodeAt(index);
    }

    return bytes.buffer;
}

function formatAuthTime(timestamp) {
    if (!timestamp) return '未知';
    return new Date(timestamp * 1000).toLocaleString();
}
