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
        const normalized = String(url || '').trim().toLowerCase();

        return !normalized || (
            normalized.includes('://') &&
            !normalized.startsWith('http://') &&
            !normalized.startsWith('https://')
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
    if (!requireAdminUiAction()) {
        event.target.value = '';
        return;
    }

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
