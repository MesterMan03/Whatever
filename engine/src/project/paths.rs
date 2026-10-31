use std::path::{Path, PathBuf};

/// Friendly Windows paths for labels and portable project metadata. Filesystem operations
/// may keep canonical extended paths internally, especially for paths exceeding MAX_PATH.
pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if let Some(path) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{path}")
    } else {
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
    }
}

pub fn portable_path(path: &Path) -> PathBuf {
    PathBuf::from(display_path(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extended_paths_are_presented_as_normal_drive_or_network_paths() {
        assert_eq!(
            display_path(Path::new(r"\\?\C:\Games\My Game")),
            r"C:\Games\My Game"
        );
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\server\games")),
            r"\\server\games"
        );
    }
}
