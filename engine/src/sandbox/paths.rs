use anyhow::{Context, bail};
use std::path::{Component, Path, PathBuf};

pub fn resolve_contained(root: &Path, path: &str) -> anyhow::Result<PathBuf> {
    if path.is_empty()
        || path.contains(['\\', ':', '\0'])
        || Path::new(path)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        bail!("invalid relative path: {path}");
    }
    let root = root.canonicalize().context("resolve storage root")?;
    let full = root.join(path);
    let mut ancestor = full.as_path();
    loop {
        match std::fs::symlink_metadata(ancestor) {
            Ok(_) => {
                let resolved = ancestor.canonicalize().context("resolve storage path")?;
                if !resolved.starts_with(&root) {
                    bail!("path escapes its storage root: {path}");
                }
                return Ok(full);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ancestor = ancestor
                    .parent()
                    .context("storage path has no existing ancestor")?;
            }
            Err(error) => return Err(error).context("inspect storage path"),
        }
    }
}

pub fn valid_storage_id(value: &str) -> bool {
    whatever_script_vm::valid_mod_id(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_nonportable_and_parent_paths() {
        for path in [
            "",
            "../secret",
            "folder/../../secret",
            "/secret",
            "C:/secret",
            "C:\\secret",
            "folder\\..\\secret",
            "//server/share",
            "save:stream",
            "bad\0name",
        ] {
            assert!(resolve_contained(Path::new("."), path).is_err(), "{path:?}");
        }
    }

    #[test]
    fn storage_ids_are_single_components() {
        assert!(valid_storage_id("first_game-2"));
        for id in ["", "Core", "..", "/tmp", "C:\\temp", "a/b", "a:b"] {
            assert!(!valid_storage_id(id), "{id}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn existing_and_dangling_symlinks_cannot_escape_storage() {
        let directory =
            std::env::temp_dir().join(format!("whatever-storage-{}", std::process::id()));
        std::fs::create_dir(&directory).expect("unique fixture directory");
        let root = directory.join("storage");
        let outside = directory.join("outside");
        std::fs::create_dir(&root).expect("storage fixture");
        std::fs::create_dir(&outside).expect("outside fixture");
        std::fs::write(outside.join("secret"), "secret").expect("outside file");
        std::os::unix::fs::symlink(&outside, root.join("escape")).expect("escape link");
        std::os::unix::fs::symlink(directory.join("missing"), root.join("dangling"))
            .expect("dangling link");
        assert!(resolve_contained(&root, "escape/secret").is_err());
        assert!(resolve_contained(&root, "escape/new/file").is_err());
        assert!(resolve_contained(&root, "dangling/new/file").is_err());
        assert_eq!(
            resolve_contained(&root, "new/nested/save.json").expect("local path"),
            root.canonicalize()
                .expect("canonical storage root")
                .join("new/nested/save.json")
        );
        std::fs::remove_dir_all(&directory).expect("remove fixture");
    }
}
