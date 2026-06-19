use crate::models::Bookmark;
use std::collections::BTreeMap;

pub const DEFAULT_EXPORT_FILENAME_PREFIX: &str = "linkwise-bookmarks";

pub fn export_filename(date: &str) -> String {
    format!("{DEFAULT_EXPORT_FILENAME_PREFIX}-{date}.html")
}

pub fn current_export_filename() -> String {
    let date = js_sys::Date::new_0();
    let year = date.get_full_year();
    let month = date.get_month() + 1;
    let day = date.get_date();
    export_filename(&format!("{year:04}-{month:02}-{day:02}"))
}

pub fn build_bookmarks_html(bookmarks: &[Bookmark], timestamp: i64) -> String {
    let tree = ExportNode::from_bookmarks(bookmarks);
    let mut lines = vec![
        "<!DOCTYPE NETSCAPE-Bookmark-file-1>".to_string(),
        r#"<META HTTP-EQUIV="Content-Type" CONTENT="text/html; charset=UTF-8">"#.to_string(),
        "<TITLE>Bookmarks</TITLE>".to_string(),
        "<H1>Bookmarks</H1>".to_string(),
        "<DL><p>".to_string(),
    ];

    lines.extend(render_export_node(&tree, timestamp, 1));
    lines.push("</DL><p>".to_string());
    lines.join("\n")
}

#[derive(Default)]
struct ExportNode {
    name: String,
    bookmarks: Vec<Bookmark>,
    children: BTreeMap<String, ExportNode>,
}

impl ExportNode {
    fn from_bookmarks(bookmarks: &[Bookmark]) -> Self {
        let mut root = Self::default();

        for bookmark in bookmarks {
            let mut current = &mut root;

            for part in crate::db::split_folder_path(&bookmark.folder) {
                current = current
                    .children
                    .entry(part.clone())
                    .or_insert_with(|| ExportNode {
                        name: part,
                        ..Default::default()
                    });
            }

            current.bookmarks.push(bookmark.clone());
        }

        root
    }
}

fn render_export_node(node: &ExportNode, timestamp: i64, depth: usize) -> Vec<String> {
    let indent = "    ".repeat(depth);
    let mut lines = Vec::new();

    for child in node.children.values() {
        lines.push(format!(
            r#"{indent}<DT><H3 ADD_DATE="{timestamp}" LAST_MODIFIED="{timestamp}">{}</H3>"#,
            escape_html(&child.name, false)
        ));
        lines.push(format!("{indent}<DL><p>"));
        lines.extend(render_export_node(child, timestamp, depth + 1));
        lines.push(format!("{indent}</DL><p>"));
    }

    for bookmark in &node.bookmarks {
        lines.push(render_export_bookmark(bookmark, timestamp, &indent));
    }

    lines
}

fn render_export_bookmark(bookmark: &Bookmark, timestamp: i64, indent: &str) -> String {
    let title = if bookmark.title.is_empty() {
        "未命名书签"
    } else {
        &bookmark.title
    };

    format!(
        r#"{indent}<DT><A HREF="{}" ADD_DATE="{timestamp}">{}</A>"#,
        escape_html(&bookmark.url, true),
        escape_html(title, false)
    )
}

fn escape_html(value: &str, quote: bool) -> String {
    let mut escaped = String::with_capacity(value.len());

    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' if quote => escaped.push_str("&quot;"),
            '\'' if quote => escaped.push_str("&#x27;"),
            _ => escaped.push(ch),
        }
    }

    escaped
}
