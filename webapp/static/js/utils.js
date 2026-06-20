function escapeHtml(value) {
    return String(value || '')
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#039;');
}

function getDomain(url) {
    try {
        return new URL(url).hostname;
    } catch (err) {
        return '未知域名';
    }
}

function setOverlayVisible(overlayEl, visible) {
    if (overlayEl) {
        overlayEl.style.display = visible ? 'flex' : 'none';
    }
}

function openOverlay(overlayEl) {
    setOverlayVisible(overlayEl, true);
}

function closeOverlay(overlayEl) {
    setOverlayVisible(overlayEl, false);
}

function isOverlayOpen(overlayEl) {
    return Boolean(overlayEl && overlayEl.style.display === 'flex');
}

function setElementVisible(element, visible, displayValue = '') {
    if (element) {
        element.style.display = visible ? displayValue : 'none';
    }
}

function setClassVisible(element, className, visible) {
    if (element) {
        element.classList.toggle(className, visible);
    }
}

async function parseApiJson(res, fallbackMessage = '请求失败') {
    const contentType = res.headers.get('content-type') || '';

    if (contentType.includes('application/json')) {
        return res.json();
    }

    const text = await res.text();
    const snippet = text.replace(/\s+/g, ' ').trim().slice(0, 80);
    throw new Error(`${fallbackMessage}：接口未返回 JSON（HTTP ${res.status}）。${snippet || '请检查后端服务是否已启动。'}`);
}

function normalizeFolder(folder) {
    const value = String(folder || '').trim();
    return value || '全部书签';
}

function splitFolder(folder) {
    const normalized = String(folder || '').trim();

    if (!normalized) {
        return [];
    }

    return normalized
        .split('/')
        .map(part => part.trim())
        .filter(Boolean);
}

function getParentFolderPath(folder) {
    const parts = splitFolder(folder);

    if (parts.length <= 1) {
        return '';
    }

    return parts.slice(0, -1).join(' / ');
}

function normalizeFolderPath(folder) {
    return splitFolder(folder).join(' / ');
}

function downloadTextFile(filename, content, mimeType) {
    const blob = new Blob([content], { type: mimeType });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');

    link.href = url;
    link.download = filename;
    document.body.appendChild(link);
    link.click();
    link.remove();

    setTimeout(() => URL.revokeObjectURL(url), 0);
}
