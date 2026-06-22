let bookmarks = [];
let selectedFolder = '__ALL__';
let expandedFolders = new Set();
let selectedBookmarkIds = new Set();
let lastImportedBookmarkIds = new Set();
let folderOrders = new Map();
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
const authActionButton = document.getElementById('auth-action-btn');
const densityMenu = document.getElementById('density-menu');
const densityCurrentLabel = document.getElementById('density-current-label');
const bookmarkListHeader = document.getElementById('bookmark-list-header');
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
const authPasskeyList = document.getElementById('auth-passkey-list');
const authSessionList = document.getElementById('auth-session-list');

const API_BASE = 'api';
const ALL_BOOKMARKS_VIEW = '__ALL__';
const LAST_IMPORT_VIEW = '__LAST_IMPORT__';
const UNCATEGORIZED_VIEW = '__UNCATEGORIZED__';
const BOOKMARK_DENSITY_KEY = 'linkwise-bookmark-density';
const BOOKMARK_DENSITIES = new Set(['comfortable', 'compact']);
const BOOKMARK_DENSITY_LABELS = {
    comfortable: '舒适',
    compact: '紧凑'
};
let authState = {
    public_read: true,
    admin_initialized: false,
    admin_unlocked: false,
    admin_session_expires_at: null,
    auth_configured: true,
    missing_config: []
};
let activeFolderSuggestionInput = null;

function syncTopbarHeight() {
    if (topbar) {
        document.documentElement.style.setProperty('--topbar-height', `${topbar.offsetHeight}px`);
    }
}
