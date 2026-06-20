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

    return Array.from(childMap.values()).sort((a, b) => compareFolders(selectedFolder, a, b));
}

function setFolderOrders(orderRows) {
    folderOrders = new Map();

    for (const row of orderRows) {
        const parentFolder = normalizeFolderPath(row.parent_folder || '');
        const folderName = String(row.folder_name || '').trim();

        if (!folderName) {
            continue;
        }

        if (!folderOrders.has(parentFolder)) {
            folderOrders.set(parentFolder, new Map());
        }

        folderOrders.get(parentFolder).set(folderName, Number(row.sort_order) || 0);
    }
}

function getFolderOrder(parentFolder, folderName) {
    const parentOrders = folderOrders.get(normalizeFolderPath(parentFolder));

    if (!parentOrders || !parentOrders.has(folderName)) {
        return Number.POSITIVE_INFINITY;
    }

    return parentOrders.get(folderName);
}

function compareFolders(parentFolder, a, b) {
    const orderA = getFolderOrder(parentFolder, a.name);
    const orderB = getFolderOrder(parentFolder, b.name);

    if (orderA !== orderB) {
        return orderA - orderB;
    }

    return a.name.localeCompare(b.name, 'zh-CN');
}

function compareBookmarks(a, b) {
    const orderA = Number.isFinite(Number(a.sort_order)) ? Number(a.sort_order) : Number.POSITIVE_INFINITY;
    const orderB = Number.isFinite(Number(b.sort_order)) ? Number(b.sort_order) : Number.POSITIVE_INFINITY;

    if (orderA !== orderB) {
        return orderA - orderB;
    }

    return String(a.title || '').localeCompare(String(b.title || ''), 'zh-CN');
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
        const res = await fetch(`${API_BASE}/bootstrap`);
        const result = await parseApiJson(res, '获取书签失败');
        const data = result.bookmarks;
        const orderData = result.folder_orders;

        if (!Array.isArray(data) || !Array.isArray(orderData)) {
            throw new Error('INVALID_BOOKMARKS_RESPONSE');
        }

        bookmarks = data;
        setFolderOrders(orderData);
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
        .sort((a, b) => compareFolders('', a, b));

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
    const parentFolder = getParentFolderPath(node.path);

    folderList.appendChild(
        createFolderTreeButton({
            label: node.name,
            count: node.count,
            value: node.path,
            parentFolder,
            level,
            hasChildren,
            icon: 'folder'
        })
    );

    if (!expandedFolders.has(node.path)) {
        return;
    }

    const children = Array.from(node.children.values())
        .sort((a, b) => compareFolders(node.path, a, b));

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

function createFolderTreeButton({ label, count, value, parentFolder = '', level, hasChildren, canManage = null, icon = 'folder' }) {
    const button = document.createElement('button');
    button.className = `folder-item ${selectedFolder === value ? 'active' : ''}`;
    button.dataset.folder = value;
    button.dataset.parentFolder = parentFolder;
    button.dataset.folderName = label;
    button.style.setProperty('--level', level);

    const isExpanded = expandedFolders.has(value);
    const showActions = canManage ?? !isSmartView(value);
    const canSort = !isSmartView(value);

    if (canSort) {
        button.draggable = true;
        button.classList.add('sortable');
        setupFolderTreeDrag(button);
    }

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

function canSortContentRows() {
    return !hasSearchKeyword() &&
        selectedFolder !== ALL_BOOKMARKS_VIEW &&
        selectedFolder !== LAST_IMPORT_VIEW;
}

function setupFolderTreeDrag(button) {
    button.addEventListener('dragstart', (event) => {
        if (event.target.closest('.folder-actions') || event.target.closest('[data-action="toggle"]')) {
            event.preventDefault();
            return;
        }

        event.dataTransfer.effectAllowed = 'move';
        event.dataTransfer.setData('text/plain', button.dataset.folder || '');
        button.classList.add('dragging');
    });

    button.addEventListener('dragend', () => {
        button.classList.remove('dragging');
        clearSortDropTargets();
    });

    button.addEventListener('dragover', (event) => {
        const dragging = folderList.querySelector('.folder-item.dragging');

        if (!dragging || dragging === button || dragging.dataset.parentFolder !== button.dataset.parentFolder) {
            return;
        }

        event.preventDefault();
        event.dataTransfer.dropEffect = 'move';
        markSortDropTarget(button, event);
    });

    button.addEventListener('dragleave', () => {
        button.classList.remove('drop-before', 'drop-after');
    });

    button.addEventListener('drop', async (event) => {
        const dragging = folderList.querySelector('.folder-item.dragging');

        if (!dragging || dragging === button || dragging.dataset.parentFolder !== button.dataset.parentFolder) {
            return;
        }

        event.preventDefault();
        await reorderFolderTreeAfterDrop(dragging, button, event);
    });
}

function setupContentRowDrag(row) {
    row.draggable = true;
    row.classList.add('sortable');

    row.addEventListener('dragstart', (event) => {
        if (
            event.target.closest('.bookmark-actions') ||
            event.target.closest('.bookmark-select') ||
            event.target.closest('button') ||
            event.target.closest('input')
        ) {
            event.preventDefault();
            return;
        }

        event.dataTransfer.effectAllowed = 'move';
        event.dataTransfer.setData('text/plain', row.dataset.sortId || '');
        row.classList.add('dragging');
    });

    row.addEventListener('dragend', () => {
        row.classList.remove('dragging');
        clearSortDropTargets();
    });

    row.addEventListener('dragover', (event) => {
        const dragging = wrapper.querySelector('.bookmark-row.dragging');

        if (!dragging || dragging === row || dragging.dataset.sortType !== row.dataset.sortType) {
            return;
        }

        event.preventDefault();
        event.dataTransfer.dropEffect = 'move';
        markSortDropTarget(row, event);
    });

    row.addEventListener('dragleave', () => {
        row.classList.remove('drop-before', 'drop-after');
    });

    row.addEventListener('drop', async (event) => {
        const dragging = wrapper.querySelector('.bookmark-row.dragging');

        if (!dragging || dragging === row || dragging.dataset.sortType !== row.dataset.sortType) {
            return;
        }

        event.preventDefault();
        await reorderContentRowsAfterDrop(dragging, row, event);
    });
}

function markSortDropTarget(target, event) {
    const rect = target.getBoundingClientRect();
    const placeAfter = event.clientY > rect.top + rect.height / 2;

    clearSortDropTargets();
    target.classList.add(placeAfter ? 'drop-after' : 'drop-before');
}

function clearSortDropTargets() {
    document.querySelectorAll('.drop-before, .drop-after').forEach((element) => {
        element.classList.remove('drop-before', 'drop-after');
    });
}

function reorderItems(items, draggedItem, targetItem, event) {
    const nextItems = items.filter(item => item !== draggedItem);
    const targetIndex = nextItems.indexOf(targetItem);
    const rect = targetItem.getBoundingClientRect();
    const insertAfter = event.clientY > rect.top + rect.height / 2;

    nextItems.splice(targetIndex + (insertAfter ? 1 : 0), 0, draggedItem);
    return nextItems;
}

async function reorderFolderTreeAfterDrop(dragging, target, event) {
    const parentFolder = dragging.dataset.parentFolder || '';
    const siblings = Array.from(folderList.querySelectorAll('.folder-item.sortable'))
        .filter(item => item.dataset.parentFolder === parentFolder);
    const ordered = reorderItems(siblings, dragging, target, event);
    const folders = ordered.map(item => item.dataset.folderName).filter(Boolean);

    await saveFolderOrder(parentFolder, folders);
}

async function reorderContentRowsAfterDrop(dragging, target, event) {
    const type = dragging.dataset.sortType;
    const siblings = Array.from(wrapper.querySelectorAll(`.bookmark-row[data-sort-type="${type}"]`));
    const ordered = reorderItems(siblings, dragging, target, event);

    if (type === 'folder') {
        await saveFolderOrder(
            selectedFolder,
            ordered.map(item => item.dataset.folderName).filter(Boolean)
        );
        return;
    }

    await saveBookmarkOrder(
        selectedFolder === UNCATEGORIZED_VIEW ? '' : selectedFolder,
        ordered.map(item => item.dataset.bookmarkId).filter(Boolean)
    );
}

async function saveFolderOrder(parentFolder, folders) {
    try {
        const res = await fetch(`${API_BASE}/folders/reorder`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                parent_folder: parentFolder,
                folders
            })
        });
        const result = await parseApiJson(res, '目录排序失败');

        if (!res.ok || result.status !== 'success') {
            await showMessage(result.message || '目录排序失败', '目录排序失败');
            return;
        }

        if (!folderOrders.has(parentFolder)) {
            folderOrders.set(parentFolder, new Map());
        }

        folders.forEach((name, index) => folderOrders.get(parentFolder).set(name, index));
        renderFolders();
        renderCards();
    } catch (err) {
        console.error('目录排序失败:', err);
        await showMessage('目录排序失败，请稍后重试', '目录排序失败');
        await fetchList();
    }
}

async function saveBookmarkOrder(folder, ids) {
    try {
        const res = await fetch(`${API_BASE}/bookmarks/reorder`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                folder,
                ids
            })
        });
        const result = await parseApiJson(res, '书签排序失败');

        if (!res.ok || result.status !== 'success') {
            await showMessage(result.message || '书签排序失败', '书签排序失败');
            return;
        }

        ids.forEach((id, index) => {
            const bookmark = bookmarks.find(item => String(item.id || '') === id);

            if (bookmark) {
                bookmark.sort_order = index;
            }
        });

        renderCards();
    } catch (err) {
        console.error('书签排序失败:', err);
        await showMessage('书签排序失败，请稍后重试', '书签排序失败');
        await fetchList();
    }
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
        row.dataset.sortType = 'folder';
        row.dataset.sortId = folder.path;
        row.dataset.folderName = folder.name;

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

            <div class="bookmark-text">
                <div class="bookmark-title" title="${escapeHtml(folder.name)}">
                    ${escapeHtml(folder.name)}
                </div>

                <div class="bookmark-domain">${folder.count} 个书签</div>
            </div>

            <span class="bookmark-folder" title="${escapeHtml(folder.path)}">${escapeHtml(folder.path)}</span>

            <div class="bookmark-actions">
                <button class="row-btn icon-btn" title="编辑目录" aria-label="编辑目录" onclick="event.stopPropagation(); handleRenameFolder('${escapeHtml(folder.path)}')">✎</button>
                <button class="row-btn icon-btn danger" title="删除目录" aria-label="删除目录" onclick="event.stopPropagation(); handleDeleteFolder('${escapeHtml(folder.path)}')">×</button>
            </div>
        `;

        wrapper.appendChild(row);
        setupSwipeActions(row);

        if (canSortContentRows()) {
            setupContentRowDrag(row);
        }

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
        : [...visible].sort(compareBookmarks);

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
        row.dataset.sortType = 'bookmark';
        row.dataset.sortId = id;
        row.dataset.bookmarkId = id;

        row.addEventListener('click', (event) => {
            if (event.target.closest('.bookmark-actions') || event.target.closest('.bookmark-select')) return;
            window.open(url, '_blank');
        });

        row.innerHTML = `
            <label class="bookmark-select" title="选择书签">
                <input type="checkbox" ${isSelected ? 'checked' : ''}>
            </label>

            <div class="bookmark-letter">${escapeHtml(firstChar)}</div>

            <div class="bookmark-text">
                <div class="bookmark-title" title="${escapeHtml(titleText)}">
                    ${escapeHtml(titleText)}
                </div>

                <div class="bookmark-domain" title="${escapeHtml(url)}">${escapeHtml(domain)}</div>
            </div>

            <span class="bookmark-folder" title="${escapeHtml(folder)}">${escapeHtml(folder)}</span>

            <div class="bookmark-actions">
                <button class="row-btn icon-btn" title="编辑书签" aria-label="编辑书签" onclick="editItem(event, '${escapeHtml(id)}')">✎</button>
                <button class="row-btn icon-btn danger" title="删除书签" aria-label="删除书签" onclick="removeItem(event, '${escapeHtml(id)}')">×</button>
            </div>
        `;

        wrapper.appendChild(row);
        setupSwipeActions(row);

        if (canSortContentRows()) {
            setupContentRowDrag(row);
        }

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
