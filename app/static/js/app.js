let bookmarks = [];
let selectedFolder = '__ALL__';
let expandedFolders = new Set();
let selectedBookmarkIds = new Set();
let lastImportedBookmarkIds = new Set();
let pendingDeleteFolder = '';
let pendingRenameFolder = '';

const wrapper = document.getElementById('cards-wrapper');
const search = document.getElementById('search');
const overlay = document.getElementById('box-overlay');
const form = document.getElementById('bookmark-form');
const boxTitle = document.getElementById('box-title');
const folderInput = document.getElementById('folder');
const smartViewList = document.getElementById('smart-view-list');
const folderList = document.getElementById('folder-list');
const currentFolderTitle = document.getElementById('current-folder-title');
const currentFolderSubtitle = document.getElementById('current-folder-subtitle');
const folderDeleteOverlay = document.getElementById('folder-delete-overlay');
const folderRenameOverlay = document.getElementById('folder-rename-overlay');
const bulkMoveOverlay = document.getElementById('bulk-move-overlay');
const moveBookmarkCount = document.getElementById('move-bookmark-count');
const topbar = document.querySelector('.topbar');
const sidebar = document.querySelector('.sidebar');
const drawerScrim = document.getElementById('drawer-scrim');
const floatingMenu = document.getElementById('floating-menu');
const topbarMoreMenu = document.getElementById('topbar-more-menu');
const bulkMoveBar = document.getElementById('bulk-move-bar');
const bulkSelectedCount = document.getElementById('bulk-selected-count');
const bulkMiniBar = document.getElementById('bulk-mini-bar');
const bulkMiniSelectedCount = document.getElementById('bulk-mini-selected-count');
const bulkMiniSelectAll = document.getElementById('bulk-mini-select-all');
const bulkMoveFolder = document.getElementById('bulk-move-folder');
const bulkSelectAll = document.getElementById('bulk-select-all');
const bulkMoveButton = document.querySelector('.bulk-move-btn');
const bulkExportButton = document.querySelector('.bulk-export-btn');
const bulkDeleteButton = document.querySelector('.bulk-delete-btn');
const bulkFinishButton = document.getElementById('bulk-finish-btn');
const bulkMiniExportButton = document.getElementById('bulk-mini-export-btn');
const bulkMiniFinishButton = document.getElementById('bulk-mini-finish-btn');
const renameFolderInput = document.getElementById('rename-folder-input');
const folderSuggestionPopup = document.getElementById('folder-suggestion-popup');
const settingsOverlay = document.getElementById('settings-overlay');
const webdavUrlInput = document.getElementById('webdav-url');
const webdavUsernameInput = document.getElementById('webdav-username');
const webdavPasswordInput = document.getElementById('webdav-password');
const webdavRemoteDirInput = document.getElementById('webdav-remote-dir');
const webdavFilenameInput = document.getElementById('webdav-filename');

const API_BASE = 'api';
const ALL_BOOKMARKS_VIEW = '__ALL__';
const LAST_IMPORT_VIEW = '__LAST_IMPORT__';
const UNCATEGORIZED_VIEW = '__UNCATEGORIZED__';
let activeFolderSuggestionInput = null;

function syncTopbarHeight() {
    if (topbar) {
        document.documentElement.style.setProperty('--topbar-height', `${topbar.offsetHeight}px`);
    }
}

function getFolderSuggestions() {
    return Array.from(
        new Set(
            bookmarks
                .map(bookmark => normalizeFolderPath(bookmark.folder || ''))
                .filter(Boolean)
        )
    ).sort((a, b) => a.localeCompare(b, 'zh-CN'));
}

function getFolderSuggestionMatches(input) {
    const keyword = String(input.value || '').trim().toLowerCase();
    const suggestions = getFolderSuggestions();
    const matches = keyword
        ? suggestions.filter(folder => folder.toLowerCase().includes(keyword))
        : suggestions;

    return matches;
}

function closeFolderSuggestions() {
    activeFolderSuggestionInput = null;

    if (folderSuggestionPopup) {
        setClassVisible(folderSuggestionPopup, 'show', false);
        folderSuggestionPopup.innerHTML = '';
    }
}

function positionFolderSuggestions(input) {
    if (!folderSuggestionPopup) return;

    const rect = input.getBoundingClientRect();
    folderSuggestionPopup.style.left = `${rect.left}px`;
    folderSuggestionPopup.style.top = `${rect.bottom + 6}px`;
    folderSuggestionPopup.style.width = `${rect.width}px`;
}

function renderFolderSuggestions(input) {
    if (!folderSuggestionPopup || !input || input.disabled) {
        closeFolderSuggestions();
        return;
    }

    const matches = getFolderSuggestionMatches(input);
    if (matches.length === 0) {
        closeFolderSuggestions();
        return;
    }

    activeFolderSuggestionInput = input;
    positionFolderSuggestions(input);
    folderSuggestionPopup.innerHTML = matches
        .map(folder => `
            <button type="button" class="folder-suggestion-item" data-folder="${escapeHtml(folder)}">
                ${escapeHtml(folder)}
            </button>
        `)
        .join('');
    setClassVisible(folderSuggestionPopup, 'show', true);
}

function setupFolderSuggestionInput(input) {
    if (!input) return;

    input.addEventListener('focus', () => renderFolderSuggestions(input));
    input.addEventListener('input', () => renderFolderSuggestions(input));
    input.addEventListener('blur', () => {
        setTimeout(() => {
            if (document.activeElement !== input) {
                closeFolderSuggestions();
            }
        }, 120);
    });
}

function refreshActiveFolderSuggestions() {
    if (activeFolderSuggestionInput && document.activeElement === activeFolderSuggestionInput) {
        renderFolderSuggestions(activeFolderSuggestionInput);
    }
}

function getFolderSuggestionInputs() {
    return [folderInput, renameFolderInput, bulkMoveFolder].filter(Boolean);
}

function getSelectedBookmarkIds() {
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
    const ids = new Set(getVisibleBookmarks().map(bookmark => String(bookmark.id || '')));

    for (const folder of getChildFolders()) {
        for (const id of getFolderRowBookmarkIds(folder.path)) {
            ids.add(id);
        }
    }

    return Array.from(ids);
}

function updateBulkMoveBar() {
    const count = selectedBookmarkIds.size;
    const currentPageIds = getCurrentPageSelectableBookmarkIds();
    const currentPageSelectedCount = currentPageIds.filter(id => selectedBookmarkIds.has(id)).length;

    if (bulkSelectedCount) {
        bulkSelectedCount.textContent = count;
    }

    if (bulkMiniSelectedCount) {
        bulkMiniSelectedCount.textContent = count;
    }

    if (bulkMoveBar) {
        setClassVisible(bulkMoveBar, 'show', true);
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
    setClassVisible(bulkMiniBar, 'show', count > 0);
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
    closeTopbarMoreMenu();
};

function showLastImportedBookmarks() {
    if (lastImportedBookmarkIds.size === 0) {
        return;
    }

    selectedFolder = LAST_IMPORT_VIEW;
    selectedBookmarkIds.clear();

    if (search) {
        search.value = '';
    }

    renderFolders();
    renderCards();
    closeFolderDrawer();
}

function closeLastImportView() {
    lastImportedBookmarkIds.clear();

    if (selectedFolder === LAST_IMPORT_VIEW) {
        selectedFolder = ALL_BOOKMARKS_VIEW;
    }

    selectedBookmarkIds.clear();
    renderFolders();
    renderCards();
}

window.closeLastImportView = closeLastImportView;

function isSmartView(value) {
    return value === ALL_BOOKMARKS_VIEW ||
        value === LAST_IMPORT_VIEW ||
        value === UNCATEGORIZED_VIEW;
}

function isInCurrentFolder(bookmarkFolder) {
    const folder = String(bookmarkFolder || '').trim();

    if (selectedFolder === ALL_BOOKMARKS_VIEW) {
        return true;
    }

    if (selectedFolder === UNCATEGORIZED_VIEW) {
        return folder.length === 0;
    }

    return folder === selectedFolder;
}

function getVisibleBookmarks() {
    const keyword = String(search.value || '').toLowerCase().trim();

    return bookmarks.filter((bookmark) => {
        const id = String(bookmark.id || '');
        const folder = String(bookmark.folder || '').trim();
        const displayFolder = normalizeFolder(folder);

        const matchFolder = selectedFolder === LAST_IMPORT_VIEW
            ? lastImportedBookmarkIds.has(id)
            : hasSearchKeyword() || isInCurrentFolder(folder);

        const matchKeyword =
            !keyword ||
            String(bookmark.title || '').toLowerCase().includes(keyword) ||
            String(bookmark.url || '').toLowerCase().includes(keyword) ||
            displayFolder.toLowerCase().includes(keyword);

        return matchFolder && matchKeyword;
    });
}

function getChildFolders() {
    if (hasSearchKeyword()) {
        return [];
    }

    if (isSmartView(selectedFolder)) {
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
    if (folderPath === ALL_BOOKMARKS_VIEW) {
        return bookmarks.length;
    }

    if (folderPath === LAST_IMPORT_VIEW) {
        return bookmarks.filter(bookmark => lastImportedBookmarkIds.has(String(bookmark.id || ''))).length;
    }

    if (folderPath === UNCATEGORIZED_VIEW) {
        return bookmarks.filter(bookmark => !String(bookmark.folder || '').trim()).length;
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
        const data = await parseApiJson(res, '获取书签失败');

        if (!Array.isArray(data)) {
            throw new Error('INVALID_BOOKMARKS_RESPONSE');
        }

        bookmarks = data;
        pruneBookmarkSelection();
        pruneLastImportedBookmarks();
        refreshActiveFolderSuggestions();

        if (!isSmartView(selectedFolder)) {
            const selectedExists = bookmarks.some((bookmark) => {
                const folder = String(bookmark.folder || '').trim();
                return folder === selectedFolder || folder.startsWith(selectedFolder + ' / ');
            });

            if (!selectedExists) {
                selectedFolder = ALL_BOOKMARKS_VIEW;
            }
        }

        ensureParentFoldersExpanded(selectedFolder);
        renderFolders();
        renderCards();
        updateBulkMoveBar();
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

    if (smartViewList) {
        smartViewList.innerHTML = '';
        renderSmartViews();
    }

    folderList.innerHTML = '';

    const topFolders = Array.from(tree.children.values())
        .sort((a, b) => a.name.localeCompare(b.name, 'zh-CN'));

    for (const folder of topFolders) {
        renderFolderNode(folder, 0);
    }
}

function renderSmartViews() {
    const smartViews = [
        {
            label: '全部书签',
            count: getFolderAllBookmarkCount(ALL_BOOKMARKS_VIEW),
            value: ALL_BOOKMARKS_VIEW,
            icon: 'all'
        },
        {
            label: '未分类',
            count: getFolderAllBookmarkCount(UNCATEGORIZED_VIEW),
            value: UNCATEGORIZED_VIEW,
            icon: 'uncategorized'
        }
    ];

    for (const view of smartViews) {
        smartViewList.appendChild(
            createFolderTreeButton({
                ...view,
                level: 0,
                hasChildren: false,
                canManage: false
            })
        );
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
            hasChildren,
            icon: 'folder'
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

function getFolderIcon(icon) {
    const icons = {
        all: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 6.5A2.5 2.5 0 0 1 6.5 4h11A2.5 2.5 0 0 1 20 6.5v11A2.5 2.5 0 0 1 17.5 20h-11A2.5 2.5 0 0 1 4 17.5v-11Zm2.5-.75A.75.75 0 0 0 5.75 6.5v11c0 .41.34.75.75.75h11c.41 0 .75-.34.75-.75v-11a.75.75 0 0 0-.75-.75h-11Zm2 3.25h7v1.5h-7V9Zm0 3.25h7v1.5h-7v-1.5Zm0 3.25h5v1.5h-5v-1.5Z"/></svg>',
        uncategorized: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 5a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V9.4L14.6 5H7Zm0 1.75h7l4.25 4.25V17a.25.25 0 0 1-.25.25H7A.25.25 0 0 1 6.75 17V7A.25.25 0 0 1 7 6.75Zm2 5.75h6v1.5H9v-1.5Z"/></svg>',
        folder: '<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 7.5A2.5 2.5 0 0 1 5.5 5h4.2l1.8 2H18.5A2.5 2.5 0 0 1 21 9.5v7A2.5 2.5 0 0 1 18.5 19h-13A2.5 2.5 0 0 1 3 16.5v-9Zm2.5-.75a.75.75 0 0 0-.75.75v9c0 .41.34.75.75.75h13c.41 0 .75-.34.75-.75v-7a.75.75 0 0 0-.75-.75h-7.78l-1.8-2H5.5Z"/></svg>'
    };

    return icons[icon] || icons.folder;
}

function createFolderTreeButton({ label, count, value, level, hasChildren, canManage = null, icon = 'folder' }) {
    const button = document.createElement('button');
    button.className = `folder-item ${selectedFolder === value ? 'active' : ''}`;
    button.dataset.folder = value;
    button.style.setProperty('--level', level);

    const isExpanded = expandedFolders.has(value);
    const showActions = canManage ?? !isSmartView(value);

    button.classList.toggle('has-children', hasChildren);
    button.innerHTML = `
        <span class="folder-left">
            ${
                hasChildren
                    ? `<span class="folder-caret ${isExpanded ? 'open' : ''}" data-action="toggle">›</span>`
                    : ''
            }
            <span class="folder-icon">${getFolderIcon(icon)}</span>
            <span class="folder-name" title="${escapeHtml(label)}">${escapeHtml(label)}</span>
        </span>

        <span class="folder-right">
            <span class="folder-count">${count}</span>
            ${
                showActions
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

function createImportGroupDivider(folder) {
    const folderLabel = folder || '未分类';
    const groupIds = getVisibleBookmarks()
        .filter(bookmark => normalizeFolder(bookmark.folder) === folder)
        .map(bookmark => String(bookmark.id || ''));
    const selectedCount = groupIds.filter(id => selectedBookmarkIds.has(id)).length;
    const allSelected = groupIds.length > 0 && selectedCount === groupIds.length;
    const partSelected = selectedCount > 0 && selectedCount < groupIds.length;

    const divider = document.createElement('div');
    divider.className = 'import-group-divider';
    divider.innerHTML = `
        <label class="import-group-select" title="选择该目录下的书签">
            <input type="checkbox" ${allSelected ? 'checked' : ''}>
        </label>
        <span class="import-group-name" title="${escapeHtml(folderLabel)}">${escapeHtml(folderLabel)}</span>
        <span class="import-group-count">${groupIds.length} 个</span>
    `;

    const checkbox = divider.querySelector('.import-group-select input');
    if (checkbox) {
        checkbox.indeterminate = partSelected;

        checkbox.addEventListener('change', () => {
            for (const id of groupIds) {
                if (checkbox.checked) {
                    selectedBookmarkIds.add(id);
                } else {
                    selectedBookmarkIds.delete(id);
                }
            }

            updateBulkMoveBar();
            renderCards();
        });
    }

    return divider;
}

function renderCards() {
    const visible = getVisibleBookmarks();
    const childFolders = getChildFolders();

    const isSearching = hasSearchKeyword();
    const isLastImportView = selectedFolder === LAST_IMPORT_VIEW;
    const title = isLastImportView
        ? '本次新增'
        : isSearching
        ? '搜索结果'
        : selectedFolder === ALL_BOOKMARKS_VIEW
            ? '全部书签'
            : selectedFolder === UNCATEGORIZED_VIEW
            ? '未分类'
            : selectedFolder;

    currentFolderTitle.textContent = title;
    currentFolderSubtitle.textContent = isLastImportView
        ? `本次导入新增 ${getFolderAllBookmarkCount(LAST_IMPORT_VIEW)} 个书签，当前显示 ${visible.length} 个`
        : isSearching
        ? `在全部书签中找到 ${visible.length} 个结果，共 ${bookmarks.length} 个书签`
        : selectedFolder === UNCATEGORIZED_VIEW
        ? `未归入目录的书签，当前显示 ${visible.length} 个，共 ${bookmarks.length} 个书签`
        : `${childFolders.length} 个子目录，${visible.length} 个书签，共 ${bookmarks.length} 个书签`;

    if (bulkExportButton) {
        bulkExportButton.hidden = isLastImportView;
    }

    if (bulkFinishButton) {
        bulkFinishButton.hidden = !isLastImportView;
    }

    if (bulkMiniExportButton) {
        bulkMiniExportButton.hidden = isLastImportView;
    }

    if (bulkMiniFinishButton) {
        bulkMiniFinishButton.hidden = !isLastImportView;
    }

    wrapper.innerHTML = '';

    if (visible.length === 0 && childFolders.length === 0) {
        wrapper.innerHTML = `
            <div class="state-text">
                没有匹配到任何书签
            </div>
        `;
        updateBulkMoveBar();
        return;
    }

    for (const folder of childFolders) {
        const folderBookmarkIds = getFolderRowBookmarkIds(folder.path);
        const selectedInFolderCount = folderBookmarkIds.filter(id => selectedBookmarkIds.has(id)).length;
        const isFolderSelected = folderBookmarkIds.length > 0 && selectedInFolderCount === folderBookmarkIds.length;
        const isFolderPartSelected = selectedInFolderCount > 0 && selectedInFolderCount < folderBookmarkIds.length;

        const row = document.createElement('div');
        row.className = `bookmark-row folder-row ${selectedInFolderCount > 0 ? 'selected' : ''}`;

        row.addEventListener('click', (event) => {
            if (event.target.closest('.bookmark-actions') || event.target.closest('.bookmark-select')) return;

            selectedFolder = folder.path;
            expandedFolders.add(folder.path);
            ensureParentFoldersExpanded(folder.path);
            renderFolders();
            renderCards();
        });

        row.innerHTML = `
            <label class="bookmark-select" title="选择目录下的书签">
                <input type="checkbox" ${isFolderSelected ? 'checked' : ''}>
            </label>

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
                <button class="row-btn" onclick="event.stopPropagation(); handleRenameFolder('${escapeHtml(folder.path)}')">编辑</button>
                <button class="row-btn danger" onclick="event.stopPropagation(); handleDeleteFolder('${escapeHtml(folder.path)}')">删除</button>
            </div>
        `;

        wrapper.appendChild(row);

        const checkbox = row.querySelector('.bookmark-select input');
        if (checkbox) {
            checkbox.indeterminate = isFolderPartSelected;

            checkbox.addEventListener('click', (event) => {
                event.stopPropagation();
            });

            checkbox.addEventListener('change', () => {
                for (const id of folderBookmarkIds) {
                    if (checkbox.checked) {
                        selectedBookmarkIds.add(id);
                    } else {
                        selectedBookmarkIds.delete(id);
                    }
                }

                updateBulkMoveBar();
                renderCards();
            });
        }
    }

    const orderedVisible = isLastImportView
        ? [...visible].sort((a, b) =>
            normalizeFolder(a.folder).localeCompare(normalizeFolder(b.folder), 'zh-CN'))
        : visible;

    let lastGroupFolder = null;

    for (const bookmark of orderedVisible) {
        const id = String(bookmark.id || '');
        const titleText = String(bookmark.title || '未命名书签');
        const url = String(bookmark.url || '');
        const domain = getDomain(url);
        const folder = normalizeFolder(bookmark.folder);
        const firstChar = titleText.charAt(0).toUpperCase() || 'B';
        const isSelected = selectedBookmarkIds.has(id);

        if (isLastImportView && folder !== lastGroupFolder) {
            lastGroupFolder = folder;
            wrapper.appendChild(createImportGroupDivider(folder));
        }

        const row = document.createElement('div');
        row.className = `bookmark-row ${isSelected ? 'selected' : ''}`;

        row.addEventListener('click', (event) => {
            if (event.target.closest('.bookmark-actions') || event.target.closest('.bookmark-select')) return;
            window.open(url, '_blank');
        });

        row.innerHTML = `
            <label class="bookmark-select" title="选择书签">
                <input type="checkbox" ${isSelected ? 'checked' : ''}>
            </label>

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

        const checkbox = row.querySelector('.bookmark-select input');
        if (checkbox) {
            checkbox.addEventListener('click', (event) => {
                event.stopPropagation();
            });

            checkbox.addEventListener('change', () => {
                if (checkbox.checked) {
                    selectedBookmarkIds.add(id);
                    row.classList.add('selected');
                } else {
                    selectedBookmarkIds.delete(id);
                    row.classList.remove('selected');
                }

                updateBulkMoveBar();
            });
        }
    }

    updateBulkMoveBar();
}

function handleDeleteFolder(folder) {
    pendingDeleteFolder = folder;

    const folderNameEl = document.getElementById('delete-folder-name');
    const parentNameEl = document.getElementById('delete-folder-parent-name');

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

    openOverlay(folderDeleteOverlay);
}

function handleRenameFolder(folder) {
    pendingRenameFolder = folder;

    const folderNameEl = document.getElementById('rename-folder-name');
    const inputEl = document.getElementById('rename-folder-input');

    if (folderNameEl) {
        folderNameEl.textContent = `「${folder}」`;
    }

    if (inputEl) {
        inputEl.value = folder;
    }

    if (folderRenameOverlay) {
        openOverlay(folderRenameOverlay);
        setTimeout(() => inputEl && inputEl.focus(), 0);
    }
}

window.closeFolderRenameDialog = function() {
    pendingRenameFolder = '';
    closeOverlay(folderRenameOverlay);
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
    closeOverlay(folderDeleteOverlay);
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

        const result = await parseApiJson(res, '删除目录失败');

        if (!res.ok || result.status !== 'success') {
            await showMessage(result.message || '删除目录失败', '操作失败');
            return;
        }

        await showMessage(`已删除目录，移动 ${result.moved_count} 个书签到上一层。`, '操作完成');

        selectedFolder = result.parent_folder || ALL_BOOKMARKS_VIEW;
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

        const result = await parseApiJson(res, '目录更新失败');

        if (!res.ok || result.status !== 'success') {
            await showMessage(result.message || '目录更新失败', '操作失败');
            return;
        }

        await showMessage(`已更新 ${result.renamed_count} 个书签的目录。`, '操作完成');

        selectedFolder = result.new_folder || ALL_BOOKMARKS_VIEW;
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

        const result = await parseApiJson(res, '彻底删除失败');

        if (!res.ok || result.status !== 'success') {
            await showMessage(result.message || '彻底删除失败', '操作失败');
            return;
        }

        await showMessage(`已彻底删除 ${result.deleted_count} 个书签。`, '操作完成');

        selectedFolder = ALL_BOOKMARKS_VIEW;
        await fetchList();
    } catch (err) {
        console.error('彻底删除目录失败:', err);
        await showMessage(`彻底删除目录失败：${err.message}`, '操作失败');
    }
}

window.toggleVisibleBookmarkSelection = function(checked) {
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

function createExportTree() {
    return {
        name: '',
        bookmarks: [],
        children: new Map()
    };
}

function buildExportTree(bookmarksToExport) {
    const root = createExportTree();

    for (const bookmark of bookmarksToExport) {
        const parts = splitFolder(bookmark.folder || '');
        let current = root;

        for (const part of parts) {
            if (!current.children.has(part)) {
                current.children.set(part, createExportTree());
                current.children.get(part).name = part;
            }

            current = current.children.get(part);
        }

        current.bookmarks.push(bookmark);
    }

    return root;
}

function renderExportBookmark(bookmark, timestamp, indent) {
    const title = escapeHtml(bookmark.title || '未命名书签');
    const url = escapeHtml(bookmark.url || '');

    return `${indent}<DT><A HREF="${url}" ADD_DATE="${timestamp}">${title}</A>`;
}

function renderExportNode(node, timestamp, depth = 1) {
    const indent = '    '.repeat(depth);
    const lines = [];
    const children = Array.from(node.children.values())
        .sort((a, b) => a.name.localeCompare(b.name, 'zh-CN'));

    for (const child of children) {
        lines.push(`${indent}<DT><H3 ADD_DATE="${timestamp}" LAST_MODIFIED="${timestamp}">${escapeHtml(child.name)}</H3>`);
        lines.push(`${indent}<DL><p>`);
        lines.push(...renderExportNode(child, timestamp, depth + 1));
        lines.push(`${indent}</DL><p>`);
    }

    for (const bookmark of node.bookmarks) {
        lines.push(renderExportBookmark(bookmark, timestamp, indent));
    }

    return lines;
}

function buildBookmarksExportHtml(bookmarksToExport) {
    const timestamp = Math.floor(Date.now() / 1000);
    const tree = buildExportTree(bookmarksToExport);
    const lines = [
        '<!DOCTYPE NETSCAPE-Bookmark-file-1>',
        '<META HTTP-EQUIV="Content-Type" CONTENT="text/html; charset=UTF-8">',
        '<TITLE>Bookmarks</TITLE>',
        '<H1>Bookmarks</H1>',
        '<DL><p>',
        ...renderExportNode(tree, timestamp, 1),
        '</DL><p>'
    ];

    return lines.join('\n');
}

function getExportFilename(scope) {
    const date = new Date().toISOString().slice(0, 10);
    return scope === 'selected'
        ? `linkwise-selected-${date}.html`
        : `linkwise-bookmarks-${date}.html`;
}

function exportBookmarks(bookmarksToExport, scope) {
    const html = buildBookmarksExportHtml(bookmarksToExport);
    downloadTextFile(
        getExportFilename(scope),
        html,
        'text/html;charset=utf-8'
    );
}

window.exportAllBookmarks = async function() {
    closeFloatingMenu();
    closeTopbarMoreMenu();

    if (bookmarks.length === 0) {
        await showMessage('没有可导出的书签。');
        return;
    }

    window.location.href = `${API_BASE}/bookmarks/export`;
};

window.exportSelectedBookmarks = async function() {
    const selectedIds = new Set(getSelectedBookmarkIds());
    const selectedBookmarks = bookmarks.filter(bookmark => selectedIds.has(String(bookmark.id || '')));

    if (selectedBookmarks.length === 0) {
        await showMessage('请选择要导出的书签。');
        return;
    }

    exportBookmarks(selectedBookmarks, 'selected');
};

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

            const result = await parseApiJson(res, '导入失败');

            if (!res.ok || result.status !== 'success') {
                await showMessage(result.message || '导入失败', '导入失败');
                return;
            }

            const skippedTotal = (result.duplicate_count || 0) + (result.skipped_count || 0);
            const importedIds = Array.isArray(result.imported_ids)
                ? result.imported_ids.map(id => String(id || '')).filter(Boolean)
                : [];

            lastImportedBookmarkIds = new Set(importedIds);

            event.target.value = '';
            await fetchList();

            if (importedIds.length > 0) {
                const shouldViewImported = await showConfirm(
                    `导入完成：新增 ${result.imported_count} 个，跳过 ${skippedTotal} 个。`,
                    {
                        title: '导入完成',
                        confirmText: '查看本次新增',
                        cancelText: '知道了'
                    }
                );

                if (shouldViewImported) {
                    showLastImportedBookmarks();
                }

                return;
            }

            await showMessage(
                `导入完成：新增 ${result.imported_count} 个，跳过 ${skippedTotal} 个。`,
                '导入完成'
            );
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
    openOverlay(overlay);
    closeFloatingMenu();
};

window.openImportFile = function() {
    closeFloatingMenu();
    document.getElementById('file-import').click();
};

window.closeBox = function() {
    closeOverlay(overlay);
};

window.editItem = function(event, id) {
    event.stopPropagation();

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
}

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
    openOverlay(settingsOverlay);
    closeFloatingMenu();

    try {
        await loadWebdavConfig();
    } catch (err) {
        console.error('获取设置失败:', err);
        await showMessage(`获取设置失败：${err.message}`, '设置');
    }
};

window.closeSettings = function() {
    closeOverlay(settingsOverlay);
};

window.saveWebdavConfig = async function() {
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

        if (res.status === 409 && result.status === 'duplicate') {
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

syncTopbarHeight();
fetchList();
