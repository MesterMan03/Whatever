use super::{Vfs, VfsError, VfsPath};
use std::path::PathBuf;

pub struct DiskLayer {
    mod_id: String,
    root: PathBuf,
}

impl DiskLayer {
    pub fn new(mod_id: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        DiskLayer {
            mod_id: mod_id.into(),
            root: root.into(),
        }
    }
}

impl Vfs for DiskLayer {
    fn read(&self, path: &VfsPath) -> Result<Vec<u8>, VfsError> {
        if path.mod_id != self.mod_id {
            return Err(VfsError::NotFound(path.as_string()));
        }
        let full = crate::sandbox::resolve_contained(&self.root, &path.path)
            .map_err(|_| VfsError::InvalidPath(path.as_string()))?;
        std::fs::read(&full).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                VfsError::NotFound(path.as_string())
            } else {
                VfsError::Io(e)
            }
        })
    }

    fn exists(&self, path: &VfsPath) -> bool {
        if path.mod_id != self.mod_id {
            return false;
        }
        crate::sandbox::resolve_contained(&self.root, &path.path).is_ok_and(|path| path.is_file())
    }

    fn list(&self, mod_id: &str, prefix: &str) -> Result<Vec<String>, VfsError> {
        if mod_id != self.mod_id {
            return Ok(vec![]);
        }
        let root = match self.root.canonicalize() {
            Ok(root) => root,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(error) => return Err(VfsError::Io(error)),
        };
        let search_root = if prefix.is_empty() {
            root.clone()
        } else {
            crate::sandbox::resolve_contained(&root, prefix)
                .map_err(|_| VfsError::InvalidPath(prefix.to_owned()))?
        };
        if !search_root.exists() {
            return Ok(vec![]);
        }
        let mut results = Vec::new();
        collect_files(&search_root, &root, &mut results)?;
        Ok(results)
    }
}

fn collect_files(
    dir: &std::path::Path,
    root: &std::path::Path,
    out: &mut Vec<String>,
) -> Result<(), VfsError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_symlink() {
            continue;
        }
        if path.is_dir() {
            collect_files(&path, root, out)?;
        } else {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            out.push(rel.to_string_lossy().into_owned());
        }
    }
    Ok(())
}
