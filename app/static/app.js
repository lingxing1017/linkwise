let bookmarks = [];
let selectedFolder = '__ALL__';
let expandedFolders = new Set();
let pendingDeleteFolder = '';
let pendingRenameFolder = '';

const wrapper = document.getElementById('cards-wrapper');
const search = document.getElementById('search');
const overlay = document.getElementById('box-overlay');
const form = document.getElementById('bookmark-form');
const boxTitle = document.getElementById('box-title');
const folderList = document.getElementById('folder-list');
const currentFolderTitle = document.getElementById('current-folder-title');
const currentFolderSubtitle = document.getElementById('current-folder-subtitle');
const appDialogOverlay = document.getElementById('app-dialog-overlay');
const appDialogTitle = document.getElementById('app-dialog-title');
const appDialogMessage = document.getElementById('app-dialog-message');
const appDialogCancel = document.getElementById('app-dialog-cancel');
const appDialogConfirm = document.getElementById('app-dialog-confirm');
const sidebar = document.querySelector('.sidebar');
const drawerScrim = document.getElementById('drawer-scrim');
const floatingMenu = document.getElementById('floating-menu');

const API_BASE = 'api';
let appDialogResolve = null;

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

function closeAppDialog(result) {
    if (appDialogOverlay) {
        appDialogOverlay.style.display = 'none';
    }

    if (appDialogResolve) {
        appDialogResolve(result);
        appDialogResolve = null;
    }
}

function openAppDialog({
    title = '提示',
    message = '',
    confirmText = '确定',
    cancelText = '取消',
    showCancel = false,
    danger = false
} = {}) {
    if (!appDialogOverlay || !appDialogTitle || !appDialogMessage || !appDialogConfirm || !appDialogCancel) {
        return Promise.resolve(!showCancel);
    }

    if (appDialogResolve) {
        closeAppDialog(false);
    }

    appDialogTitle.textContent = title;
    appDialogMessage.textContent = message;
    appDialogConfirm.textContent = confirmText;
    appDialogCancel.textContent = cancelText;
    appDialogCancel.style.display = showCancel ? '' : 'none';
    appDialogConfirm.classList.toggle('danger', danger);
    appDialogOverlay.style.display = 'flex';

    return new Promise((resolve) => {
        appDialogResolve = resolve;
    });
}

function showMessage(message, title = '提示') {
    return openAppDialog({
        title,
        message,
        confirmText: '知道了'
    });
}

function showConfirm(message, {
    title = '确认操作',
    confirmText = '确定',
    cancelText = '取消',
    danger = false
} = {}) {
    return openAppDialog({
        title,
        message,
        confirmText,
        cancelText,
        showCancel: true,
        danger
    });
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

function hasSearchKeyword() {
    return Boolean(String(search.value || '').trim());
}

function isMobileViewport() {
    return window.matchMedia('(max-width: 860px)').matches;
}

function closeFolderDrawer() {
    if (sidebar) {
        sidebar.classList.remove('open');
    }

    if (drawerScrim) {
        drawerScrim.classList.remove('open');
    }
}

window.closeFolderDrawer = closeFolderDrawer;

window.toggleFolderDrawer = function(event) {
    if (event) {
        event.stopPropagation();
    }

    if (!sidebar || !drawerScrim) return;

    const shouldOpen = !sidebar.classList.contains('open');
    sidebar.classList.toggle('open', shouldOpen);
    drawerScrim.classList.toggle('open', shouldOpen);
    closeFloatingMenu();
};

function isInCurrentFolder(bookmarkFolder) {
    const folder = String(bookmarkFolder || '').trim();

    if (selectedFolder === '__ALL__') {
        return true;
    }

    return folder === selectedFolder;
}

function getVisibleBookmarks() {
    const keyword = String(search.value || '').toLowerCase().trim();

    return bookmarks.filter((bookmark) => {
        const folder = String(bookmark.folder || '').trim();
        const displayFolder = normalizeFolder(folder);

        const matchFolder = hasSearchKeyword() || isInCurrentFolder(folder);

        const matchKeyword =
            !keyword ||
            String(bookmark.title || '').toLowerCase().includes(keyword) ||
            String(bookmark.url || '').toLowerCase().includes(keyword) ||
            displayFolder.toLowerCase().includes(keyword);

        return matchFolder && matchKeyword;
    });
}

function getChildFolders() {
    if (selectedFolder === '__ALL__' || hasSearchKeyword()) {
        return [];
    }

    const keyword = String(search.value || '').toLowerCase().trim();
    const childMap = new Map();

    for (const bookmark of bookmarks) {
        const folder = String(bookmark.folder || '').trim();
        const parts = splitFolder(folder);
        const selectedParts = splitFolder(selectedFolder);

        if (parts.length <= selectedParts.length) {
            continue;
        }

        const parentPath = parts.slice(0, selectedParts.length).join(' / ');

        if (parentPath !== selectedFolder) {
            continue;
        }

        const childName = parts[selectedParts.length];
        const childPath = parts.slice(0, selectedParts.length + 1).join(' / ');

        if (
            keyword &&
            !childName.toLowerCase().includes(keyword) &&
            !childPath.toLowerCase().includes(keyword)
        ) {
            continue;
        }

        childMap.set(childPath, {
            name: childName,
            path: childPath,
            count: (childMap.get(childPath)?.count || 0) + 1
        });
    }

    return Array.from(childMap.values()).sort((a, b) => a.name.localeCompare(b.name, 'zh-CN'));
}

function getFolderAllBookmarkCount(folderPath) {
    if (folderPath === '__ALL__') {
        return bookmarks.length;
    }

    return bookmarks.filter(bookmark => {
        const folder = String(bookmark.folder || '').trim();
        return folder === folderPath || folder.startsWith(folderPath + ' / ');
    }).length;
}

function ensureParentFoldersExpanded(folderPath) {
    const parts = splitFolder(folderPath);

    for (let i = 1; i < parts.length; i++) {
        expandedFolders.add(parts.slice(0, i).join(' / '));
    }
}

async function fetchList() {
    try {
        const res = await fetch(`${API_BASE}/bookmarks`);
        const data = await res.json();

        if (!Array.isArray(data)) {
            throw new Error('INVALID_BOOKMARKS_RESPONSE');
        }

        bookmarks = data;

        if (selectedFolder !== '__ALL__') {
            const selectedExists = bookmarks.some((bookmark) => {
                const folder = String(bookmark.folder || '').trim();
                return folder === selectedFolder || folder.startsWith(selectedFolder + ' / ');
            });

            if (!selectedExists) {
                selectedFolder = '__ALL__';
            }
        }

        ensureParentFoldersExpanded(selectedFolder);
        renderFolders();
        renderCards();
    } catch (err) {
        console.error('获取书签失败:', err);
        const message = err.message === 'INVALID_BOOKMARKS_RESPONSE'
            ? '数据格式异常，请稍后重试。'
            : '书签加载失败，请刷新页面或检查服务是否已启动。';

        wrapper.innerHTML = `
            <div class="state-text">
                ${escapeHtml(message)}
            </div>
        `;
    }
}

function buildFolderTree() {
    const root = {
        name: '',
        path: '',
        count: 0,
        children: new Map()
    };

    for (const bookmark of bookmarks) {
        const folder = String(bookmark.folder || '').trim();

        if (!folder) {
            continue;
        }

        const parts = splitFolder(folder);
        let current = root;

        for (let i = 0; i < parts.length; i++) {
            const name = parts[i];
            const path = parts.slice(0, i + 1).join(' / ');

            if (!current.children.has(name)) {
                current.children.set(name, {
                    name,
                    path,
                    count: 0,
                    children: new Map()
                });
            }

            current = current.children.get(name);
            current.count++;
        }
    }

    return root;
}

function renderFolders() {
    const tree = buildFolderTree();

    folderList.innerHTML = '';

    folderList.appendChild(
        createFolderTreeButton({
            label: '全部书签',
            count: bookmarks.length,
            value: '__ALL__',
            level: 0,
            hasChildren: false
        })
    );

    const topFolders = Array.from(tree.children.values())
        .sort((a, b) => a.name.localeCompare(b.name, 'zh-CN'));

    for (const folder of topFolders) {
        renderFolderNode(folder, 0);
    }
}

function renderFolderNode(node, level) {
    const hasChildren = node.children.size > 0;

    folderList.appendChild(
        createFolderTreeButton({
            label: node.name,
            count: node.count,
            value: node.path,
            level,
            hasChildren
        })
    );

    if (!expandedFolders.has(node.path)) {
        return;
    }

    const children = Array.from(node.children.values())
        .sort((a, b) => a.name.localeCompare(b.name, 'zh-CN'));

    for (const child of children) {
        renderFolderNode(child, level + 1);
    }
}

function createFolderTreeButton({ label, count, value, level, hasChildren }) {
    const button = document.createElement('button');
    button.className = `folder-item ${selectedFolder === value ? 'active' : ''}`;
    button.dataset.folder = value;
    button.style.setProperty('--level', level);

    const isExpanded = expandedFolders.has(value);
    const canDelete = value !== '__ALL__';

    button.innerHTML = `
        <span class="folder-left">
            ${
                hasChildren
                    ? `<span class="folder-caret ${isExpanded ? 'open' : ''}" data-action="toggle">›</span>`
                    : `<span class="folder-caret-placeholder"></span>`
            }
            <span class="folder-name" title="${escapeHtml(label)}">${escapeHtml(label)}</span>
        </span>

        <span class="folder-right">
            <span class="folder-count">${count}</span>
            ${
                canDelete
                    ? `
                    <span class="folder-actions">
                        <span class="folder-edit" data-action="rename-folder" title="重命名或移动目录">✎</span>
                        <span class="folder-delete" data-action="delete-folder" title="删除目录">×</span>
                    </span>
                `
                    : ''
            }
        </span>
    `;

    button.addEventListener('click', async (event) => {
        const toggleTarget = event.target.closest('[data-action="toggle"]');
        const renameTarget = event.target.closest('[data-action="rename-folder"]');
        const deleteTarget = event.target.closest('[data-action="delete-folder"]');

        if (toggleTarget && hasChildren) {
            event.stopPropagation();

            if (expandedFolders.has(value)) {
                expandedFolders.delete(value);
            } else {
                expandedFolders.add(value);
            }

            renderFolders();
            return;
        }

        if (renameTarget) {
            event.stopPropagation();
            handleRenameFolder(value);
            return;
        }

        if (deleteTarget) {
            event.stopPropagation();
            handleDeleteFolder(value);
            return;
        }

        selectedFolder = value;

        if (hasChildren && !expandedFolders.has(value)) {
            expandedFolders.add(value);
        }

        ensureParentFoldersExpanded(value);
        renderFolders();
        renderCards();

        if (isMobileViewport()) {
            closeFolderDrawer();
        }
    });

    return button;
}

function renderCards() {
    const visible = getVisibleBookmarks();
    const childFolders = getChildFolders();

    const isSearching = hasSearchKeyword();
    const title = isSearching
        ? '搜索结果'
        : selectedFolder === '__ALL__'
            ? '全部书签'
            : selectedFolder;

    currentFolderTitle.textContent = title;
    currentFolderSubtitle.textContent = isSearching
        ? `在全部书签中找到 ${visible.length} 个结果，共 ${bookmarks.length} 个书签`
        : `${childFolders.length} 个子目录，${visible.length} 个书签，共 ${bookmarks.length} 个书签`;

    wrapper.innerHTML = '';

    if (visible.length === 0 && childFolders.length === 0) {
        wrapper.innerHTML = `
            <div class="state-text">
                没有匹配到任何书签
            </div>
        `;
        return;
    }

    for (const folder of childFolders) {
        const row = document.createElement('div');
        row.className = 'bookmark-row folder-row';

        row.addEventListener('click', () => {
            selectedFolder = folder.path;
            expandedFolders.add(folder.path);
            ensureParentFoldersExpanded(folder.path);
            renderFolders();
            renderCards();
        });

        row.innerHTML = `
            <div class="bookmark-letter folder-letter">📁</div>

            <div class="bookmark-main">
                <div class="bookmark-title" title="${escapeHtml(folder.name)}">
                    ${escapeHtml(folder.name)}
                </div>

                <div class="bookmark-meta">
                    <span class="bookmark-domain">${folder.count} 个书签</span>
                    <span class="dot">•</span>
                    <span class="bookmark-folder" title="${escapeHtml(folder.path)}">${escapeHtml(folder.path)}</span>
                </div>
            </div>

            <div class="bookmark-actions">
                <button class="row-btn" onclick="event.stopPropagation(); selectedFolder='${escapeHtml(folder.path)}'; expandedFolders.add('${escapeHtml(folder.path)}'); ensureParentFoldersExpanded('${escapeHtml(folder.path)}'); renderFolders(); renderCards();">打开</button>
            </div>
        `;

        wrapper.appendChild(row);
    }

    for (const bookmark of visible) {
        const id = String(bookmark.id || '');
        const titleText = String(bookmark.title || '未命名书签');
        const url = String(bookmark.url || '');
        const domain = getDomain(url);
        const folder = normalizeFolder(bookmark.folder);
        const firstChar = titleText.charAt(0).toUpperCase() || 'B';

        const row = document.createElement('div');
        row.className = 'bookmark-row';

        row.addEventListener('click', (event) => {
            if (event.target.closest('.bookmark-actions')) return;
            window.open(url, '_blank');
        });

        row.innerHTML = `
            <div class="bookmark-letter">${escapeHtml(firstChar)}</div>

            <div class="bookmark-main">
                <div class="bookmark-title" title="${escapeHtml(titleText)}">
                    ${escapeHtml(titleText)}
                </div>

                <div class="bookmark-meta">
                    <span class="bookmark-domain" title="${escapeHtml(url)}">${escapeHtml(domain)}</span>
                    <span class="dot">•</span>
                    <span class="bookmark-folder" title="${escapeHtml(folder)}">${escapeHtml(folder)}</span>
                </div>
            </div>

            <div class="bookmark-actions">
                <button class="row-btn" onclick="editItem(event, '${escapeHtml(id)}')">编辑</button>
                <button class="row-btn danger" onclick="removeItem(event, '${escapeHtml(id)}')">删除</button>
            </div>
        `;

        wrapper.appendChild(row);
    }
}

function handleDeleteFolder(folder) {
    pendingDeleteFolder = folder;

    const folderNameEl = document.getElementById('delete-folder-name');
    const parentNameEl = document.getElementById('delete-folder-parent-name');
    const overlayEl = document.getElementById('folder-delete-overlay');

    const parentFolder = getParentFolderPath(folder);

    if (folderNameEl) {
        folderNameEl.textContent = `「${folder}」`;
    }

    if (parentNameEl) {
        parentNameEl.textContent = parentFolder ? `「${parentFolder}」` : '「全部书签」';
    }

    const moveRadio = document.querySelector('input[name="folder-delete-mode"][value="move-up"]');
    if (moveRadio) {
        moveRadio.checked = true;
    }

    if (overlayEl) {
        overlayEl.style.display = 'flex';
    }
}

function handleRenameFolder(folder) {
    pendingRenameFolder = folder;

    const folderNameEl = document.getElementById('rename-folder-name');
    const inputEl = document.getElementById('rename-folder-input');
    const overlayEl = document.getElementById('folder-rename-overlay');

    if (folderNameEl) {
        folderNameEl.textContent = `「${folder}」`;
    }

    if (inputEl) {
        inputEl.value = folder;
    }

    if (overlayEl) {
        overlayEl.style.display = 'flex';
        setTimeout(() => inputEl && inputEl.focus(), 0);
    }
}

window.closeFolderRenameDialog = function() {
    pendingRenameFolder = '';

    const overlayEl = document.getElementById('folder-rename-overlay');
    if (overlayEl) {
        overlayEl.style.display = 'none';
    }
};

window.confirmFolderRename = async function() {
    if (!pendingRenameFolder) return;

    const inputEl = document.getElementById('rename-folder-input');
    const folder = pendingRenameFolder;
    const newFolder = normalizeFolderPath(inputEl ? inputEl.value : '');

    closeFolderRenameDialog();

    await renameFolder(folder, newFolder);
};

window.closeFolderDeleteDialog = function() {
    pendingDeleteFolder = '';

    const overlayEl = document.getElementById('folder-delete-overlay');
    if (overlayEl) {
        overlayEl.style.display = 'none';
    }
};

window.confirmFolderDelete = async function() {
    if (!pendingDeleteFolder) return;

    const folder = pendingDeleteFolder;
    const checked = document.querySelector('input[name="folder-delete-mode"]:checked');
    const mode = checked ? checked.value : 'move-up';

    closeFolderDeleteDialog();

    if (mode === 'delete-all') {
        const really = await showConfirm(`确认删除目录「${folder}」以及它下面的所有书签吗？这个操作不可恢复。`, {
            title: '彻底删除目录',
            confirmText: '删除',
            danger: true
        });
        if (!really) return;

        await deleteFolderWithBookmarks(folder);
        return;
    }

    await moveFolderBookmarksUp(folder);
};

async function moveFolderBookmarksUp(folder) {
    try {
        const res = await fetch(`${API_BASE}/folders/move-up`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({ folder })
        });

        const result = await res.json();

        if (!res.ok || result.status !== 'success') {
            await showMessage(result.message || '删除目录失败', '操作失败');
            return;
        }

        await showMessage(`已删除目录，移动 ${result.moved_count} 个书签到上一层。`, '操作完成');

        selectedFolder = result.parent_folder || '__ALL__';
        ensureParentFoldersExpanded(selectedFolder);
        await fetchList();
    } catch (err) {
        console.error('删除目录失败:', err);
        await showMessage(`删除目录失败：${err.message}`, '操作失败');
    }
}

async function renameFolder(folder, newFolder) {
    try {
        const res = await fetch(`${API_BASE}/folders/rename`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                folder,
                new_folder: newFolder
            })
        });

        const result = await res.json();

        if (!res.ok || result.status !== 'success') {
            await showMessage(result.message || '目录更新失败', '操作失败');
            return;
        }

        await showMessage(`已更新 ${result.renamed_count} 个书签的目录。`, '操作完成');

        selectedFolder = result.new_folder || '__ALL__';
        ensureParentFoldersExpanded(selectedFolder);
        await fetchList();
    } catch (err) {
        console.error('目录更新失败:', err);
        await showMessage(`目录更新失败：${err.message}`, '操作失败');
    }
}

async function deleteFolderWithBookmarks(folder) {
    try {
        const res = await fetch(`${API_BASE}/folders/delete`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({ folder })
        });

        const result = await res.json();

        if (!res.ok || result.status !== 'success') {
            await showMessage(result.message || '彻底删除失败', '操作失败');
            return;
        }

        await showMessage(`已彻底删除 ${result.deleted_count} 个书签。`, '操作完成');

        selectedFolder = '__ALL__';
        await fetchList();
    } catch (err) {
        console.error('彻底删除目录失败:', err);
        await showMessage(`彻底删除目录失败：${err.message}`, '操作失败');
    }
}

function parseBookmarksHtml(htmlContent) {
    const parser = new DOMParser();
    const doc = parser.parseFromString(htmlContent, 'text/html');
    const rootDL = doc.querySelector('dl');
    const parsedBookmarks = [];

    if (!rootDL) {
        return parsedBookmarks;
    }

    function isIgnoredUrl(url) {
        return (
            !url ||
            url.startsWith('about:') ||
            url.startsWith('place:') ||
            url.startsWith('javascript:')
        );
    }

    function getDirectChildByTag(element, tagName) {
        const target = tagName.toUpperCase();

        return Array.from(element.children).find(child => {
            return child.tagName && child.tagName.toUpperCase() === target;
        });
    }

    function findNestedDLForDT(dtElement) {
        let next = dtElement.nextElementSibling;

        while (next) {
            const tag = next.tagName ? next.tagName.toUpperCase() : '';

            if (tag === 'DL') {
                return next;
            }

            if (tag === 'P') {
                const dlInP = next.querySelector('dl');
                if (dlInP) return dlInP;
            }

            if (tag === 'DT') {
                break;
            }

            next = next.nextElementSibling;
        }

        const innerDL = getDirectChildByTag(dtElement, 'DL');
        if (innerDL) {
            return innerDL;
        }

        return null;
    }

    function parseContainer(container, folderPath = []) {
        const children = Array.from(container.children);

        for (const child of children) {
            const tag = child.tagName ? child.tagName.toUpperCase() : '';

            if (tag === 'P') {
                parseContainer(child, folderPath);
                continue;
            }

            if (tag === 'DL') {
                parseContainer(child, folderPath);
                continue;
            }

            if (tag !== 'DT') {
                continue;
            }

            const directA = getDirectChildByTag(child, 'A');
            const directH3 = getDirectChildByTag(child, 'H3');

            if (directA) {
                const title = directA.textContent.trim() || '未命名书签';
                const url = directA.getAttribute('href') || '';

                if (!isIgnoredUrl(url)) {
                    parsedBookmarks.push({
                        id: `${Date.now()}-${parsedBookmarks.length}`,
                        title,
                        url,
                        folder: folderPath.join(' / ')
                    });
                }

                continue;
            }

            if (directH3) {
                const folderName = directH3.textContent.trim();
                const nestedDL = findNestedDLForDT(child);

                if (nestedDL) {
                    parseContainer(
                        nestedDL,
                        folderName ? [...folderPath, folderName] : folderPath
                    );
                }
            }
        }
    }

    parseContainer(rootDL, []);
    console.log('parseBookmarksHtml 解析结果:', parsedBookmarks);

    return parsedBookmarks;
}

async function handleImport(event) {
    const file = event.target.files[0];
    if (!file) return;

    const reader = new FileReader();

    reader.onload = async function(e) {
        const bookmarksToImport = parseBookmarksHtml(e.target.result);

        if (bookmarksToImport.length === 0) {
            await showMessage('没有找到可导入的书签。');
            event.target.value = '';
            return;
        }

        const shouldImport = await showConfirm(`找到 ${bookmarksToImport.length} 个书签，是否导入？`, {
            title: '导入书签',
            confirmText: '导入'
        });

        if (!shouldImport) {
            event.target.value = '';
            return;
        }

        try {
            const res = await fetch(`${API_BASE}/bookmarks/bulk`, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json'
                },
                body: JSON.stringify({
                    bookmarks: bookmarksToImport
                })
            });

            const result = await res.json();

            if (!res.ok || result.status !== 'success') {
                await showMessage(result.message || '导入失败', '导入失败');
                return;
            }

            const skippedTotal = (result.duplicate_count || 0) + (result.skipped_count || 0);

            await showMessage(
                `导入完成：新增 ${result.imported_count} 个，跳过 ${skippedTotal} 个。`,
                '导入完成'
            );

            event.target.value = '';
            await fetchList();
        } catch (err) {
            console.error('导入失败:', err);
            await showMessage(`导入失败：${err.message}`, '导入失败');
        }
    };

    reader.readAsText(file);
}

window.openBox = function() {
    boxTitle.innerText = '创建新书签';
    document.getElementById('bookmark-id').value = '';
    form.reset();
    overlay.style.display = 'flex';
    closeFloatingMenu();
};

window.openImportFile = function() {
    closeFloatingMenu();
    document.getElementById('file-import').click();
};

window.closeBox = function() {
    overlay.style.display = 'none';
};

window.editItem = function(event, id) {
    event.stopPropagation();

    const target = bookmarks.find((bookmark) => String(bookmark.id) === String(id));
    if (!target) return;

    boxTitle.innerText = '编辑书签';
    document.getElementById('bookmark-id').value = target.id;
    document.getElementById('title').value = target.title || '';
    document.getElementById('url').value = target.url || '';
    document.getElementById('folder').value = target.folder || '';
    overlay.style.display = 'flex';
};

window.removeItem = async function(event, id) {
    event.stopPropagation();

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

        const result = await res.json();

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

    if (!floatingMenu) return;

    const shouldOpen = !floatingMenu.classList.contains('open');
    floatingMenu.classList.toggle('open', shouldOpen);
    floatingMenu.classList.remove('hover');

    if (event.currentTarget) {
        event.currentTarget.blur();
    }
};

function closeFloatingMenu() {
    if (floatingMenu) {
        floatingMenu.classList.remove('open', 'hover');
    }
}

document.addEventListener('click', function(event) {
    if (floatingMenu && !floatingMenu.contains(event.target)) {
        closeFloatingMenu();
    }
});

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

if (appDialogCancel) {
    appDialogCancel.addEventListener('click', () => closeAppDialog(false));
}

if (appDialogConfirm) {
    appDialogConfirm.addEventListener('click', () => closeAppDialog(true));
}

if (appDialogOverlay) {
    appDialogOverlay.addEventListener('click', (event) => {
        if (event.target === appDialogOverlay) {
            closeAppDialog(false);
        }
    });
}

const renameFolderInput = document.getElementById('rename-folder-input');
if (renameFolderInput) {
    renameFolderInput.addEventListener('keydown', (event) => {
        if (event.key === 'Enter') {
            event.preventDefault();
            confirmFolderRename();
        }
    });
}

document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape' && sidebar && sidebar.classList.contains('open')) {
        closeFolderDrawer();
        return;
    }

    if (event.key === 'Escape' && appDialogOverlay && appDialogOverlay.style.display === 'flex') {
        closeAppDialog(false);
    }
});

form.addEventListener('submit', async function(event) {
    event.preventDefault();

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

        const result = await res.json();

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

fetchList();
