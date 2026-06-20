pub const D1_BINDING: &str = "DB";

pub fn normalize_folder_path(folder: &str) -> String {
    split_folder_path(folder).join(" / ")
}

pub fn split_folder_path(folder: &str) -> Vec<String> {
    folder
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_folder_path() {
        assert_eq!(normalize_folder_path(" Dev / Rust "), "Dev / Rust");
        assert_eq!(normalize_folder_path("///"), "");
    }
}
