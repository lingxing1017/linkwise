function isMobileViewport() {
    return window.matchMedia('(max-width: 860px)').matches;
}

function closeSwipeActions(exceptRow = null) {
    document.querySelectorAll('.bookmark-row.actions-open').forEach((row) => {
        if (row !== exceptRow) {
            row.classList.remove('actions-open');
        }
    });
}

function setupSwipeActions(row) {
    if (!row) return;

    let startX = 0;
    let startY = 0;
    let didSwipe = false;

    row.addEventListener('touchstart', (event) => {
        if (!isMobileViewport() || event.touches.length !== 1) return;

        startX = event.touches[0].clientX;
        startY = event.touches[0].clientY;
        didSwipe = false;
    }, { passive: true });

    row.addEventListener('touchmove', (event) => {
        if (!isMobileViewport() || event.touches.length !== 1) return;

        const deltaX = event.touches[0].clientX - startX;
        const deltaY = event.touches[0].clientY - startY;

        if (Math.abs(deltaX) > 12 && Math.abs(deltaX) > Math.abs(deltaY)) {
            didSwipe = true;
        }
    }, { passive: true });

    row.addEventListener('touchend', (event) => {
        if (!isMobileViewport() || !didSwipe) return;

        const touch = event.changedTouches[0];
        const deltaX = touch.clientX - startX;
        const deltaY = touch.clientY - startY;

        if (Math.abs(deltaX) <= Math.abs(deltaY)) return;

        if (deltaX < -42) {
            closeSwipeActions(row);
            row.classList.add('actions-open');
        } else if (deltaX > 30) {
            row.classList.remove('actions-open');
        }
    }, { passive: true });

    row.addEventListener('click', (event) => {
        if (
            !isMobileViewport() ||
            !row.classList.contains('actions-open') ||
            event.target.closest('.bookmark-actions') ||
            event.target.closest('.bookmark-select')
        ) {
            return;
        }

        event.stopPropagation();
        event.preventDefault();
        row.classList.remove('actions-open');
    }, true);
}

document.addEventListener('click', (event) => {
    if (!isMobileViewport()) return;
    if (event.target.closest('.bookmark-row.actions-open')) return;
    closeSwipeActions();
});

document.addEventListener('touchstart', (event) => {
    if (!isMobileViewport()) return;
    if (event.target.closest('.bookmark-row.actions-open')) return;
    closeSwipeActions();
}, { passive: true });

window.addEventListener('resize', () => {
    if (!isMobileViewport()) {
        closeSwipeActions();
    }
});

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
