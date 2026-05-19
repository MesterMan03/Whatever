use std::path::PathBuf;
use super::{Vfs, VfsError, VfsPath};

pub struct DiskLayer {
    mod_id: String,
    root: PathBuf,
}

impl DiskLayer {
    pub fn new(mod_id: impl Into<String>, root: impl Into<PathBuf>) -> Self {
        DiskLayer { mod_id: mod_id.into(), root: root.into() }
    }
}

impl Vfs for DiskLayer {
    fn read(&self, path: &VfsPath) -> Result<Vec<u8>, VfsError> {
        if path.mod_id != self.mod_id {
            return Err(VfsError::NotFound(path.to_string()));
        }
        let full = self.root.join(&path.path);
        std::fs::read(&full).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                VfsError::NotFound(path.to_string())
            } else {
                VfsError::Io(e)
            }
        })
    }

    fn exists(&self, path: &VfsPath) -> bool {
        if path.mod_id != self.mod_id {
            return false;
        }
        self.root.join(&path.path).exists()
    }

    fn list(&self, mod_id: &str, prefix: &str) -> Result<Vec<String>, VfsError> {
        if mod_id != self.mod_id {
            return Ok(vec![]);
        }
        let search_root = self.root.join(prefix);
        if !search_root.exists() {
            return Ok(vec![]);
        }
        let mut results = Vec::new();
        collect_files(&search_root, &self.root, &mut results)?;
        Ok(results)
    }
}

fn collect_files(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<String>) -> Result<(), VfsError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, root, out)?;
        } else {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            out.push(rel.to_string_lossy().into_owned());
        }
    }
    Ok(())
}