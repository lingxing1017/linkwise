function getStoredBookmarkDensity() {
    try {
        const stored = localStorage.getItem(BOOKMARK_DENSITY_KEY);
        return BOOKMARK_DENSITIES.has(stored) ? stored : 'comfortable';
    } catch (_) {
        return 'comfortable';
    }
}

function updateDensityMenu(density) {
    if (densityCurrentLabel) {
        densityCurrentLabel.textContent = BOOKMARK_DENSITY_LABELS[density] || BOOKMARK_DENSITY_LABELS.comfortable;
    }

    document.querySelectorAll('[data-density-option]').forEach((option) => {
        const isActive = option.dataset.densityOption === density;
        option.classList.toggle('active', isActive);
        option.setAttribute('aria-checked', String(isActive));
    });
}

function applyBookmarkDensity(density) {
    const normalized = BOOKMARK_DENSITIES.has(density) ? density : 'comfortable';
    document.body.dataset.density = normalized;
    updateDensityMenu(normalized);
}

window.setBookmarkDensity = function(density, event) {
    if (event) {
        event.stopPropagation();
    }

    const normalized = BOOKMARK_DENSITIES.has(density) ? density : 'comfortable';

    try {
        localStorage.setItem(BOOKMARK_DENSITY_KEY, normalized);
    } catch (_) {
        // Ignore storage failures; the current session can still switch density.
    }

    applyBookmarkDensity(normalized);
    closeTopbarMoreMenu();
};

window.toggleDensitySubmenu = function(event) {
    event.stopPropagation();

    if (densityMenu) {
        densityMenu.classList.toggle('submenu-open');
    }
};
