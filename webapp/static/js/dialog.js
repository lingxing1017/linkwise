const appDialogOverlay = document.getElementById('app-dialog-overlay');
const appDialogTitle = document.getElementById('app-dialog-title');
const appDialogMessage = document.getElementById('app-dialog-message');
const appDialogCancel = document.getElementById('app-dialog-cancel');
const appDialogConfirm = document.getElementById('app-dialog-confirm');

let appDialogResolve = null;

function closeAppDialog(result) {
    closeOverlay(appDialogOverlay);

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
    setElementVisible(appDialogCancel, showCancel);
    appDialogConfirm.classList.toggle('danger', danger);
    openOverlay(appDialogOverlay);

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

function isAppDialogOpen() {
    return isOverlayOpen(appDialogOverlay);
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
