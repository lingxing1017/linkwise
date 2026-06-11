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
