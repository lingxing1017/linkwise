use crate::models::Bookmark;

pub const DEFAULT_EXPORT_FILENAME_PREFIX: &str = "linkwise-bookmarks";

pub fn export_filename(date: &str) -> String {
    format!("{DEFAULT_EXPORT_FILENAME_PREFIX}-{date}.html")
}

pub fn build_bookmarks_html(_bookmarks: &[Bookmark]) -> String {
    "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<TITLE>Bookmarks</TITLE>\n<H1>Bookmarks</H1>\n"
        .to_string()
}
