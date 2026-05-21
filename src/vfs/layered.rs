use super::{Vfs, VfsError, VfsPath};
use crate::debug::DebugLogger;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufWriter;
use std::sync::{Arc, Mutex};

pub struct LayeredVfs {
    layers: Vec<Box<dyn Vfs>>,
    overrides: HashMap<String, VfsPath>,
    override_dests: HashSet<String>,
    log_file: Option<Arc<Mutex<BufWriter<File>>>>,
    log_console: Option<Arc<Mutex<Vec<String>>>>,
}

impl LayeredVfs {
    pub fn new() -> Self {
        LayeredVfs {
            layers: Vec::new(),
            overrides: HashMap::new(),
            override_dests: HashSet::new(),
            log_file: None,
            log_console: None,
        }
    }

    pub fn set_log(
        &mut self,
        file: Arc<Mutex<BufWriter<File>>>,
        console: Option<Arc<Mutex<Vec<String>>>>,
    ) {
        self.log_file = Some(file);
        self.log_console = console;
    }

    pub fn push_layer(&mut self, layer: Box<dyn Vfs>) {
        self.layers.push(layer);
    }

    pub fn add_override(&mut self, from: VfsPath, to: VfsPath) {
        self.vfs_log(&format!("override: {} -> {}", from.as_string(), to.as_string()));
        self.override_dests.insert(to.as_string());
        self.overrides.insert(from.as_string(), to);
    }

    fn vfs_log(&self, msg: &str) {
        if let Some(ref w) = self.log_file {
            DebugLogger::write_vfs(w, self.log_console.as_ref(), msg);
        }
    }
}

impl Vfs for LayeredVfs {
    fn read(&self, path: &VfsPath) -> Result<Vec<u8>, VfsError> {
        if self.override_dests.contains(&path.as_string()) {
            self.vfs_log(&format!("read {} -> blocked (override destination)", path.as_string()));
            return Err(VfsError::NotFound(path.as_string()));
        }
        let resolved = self.overrides.get(&path.as_string()).unwrap_or(path);
        for layer in self.layers.iter().rev() {
            if layer.exists(resolved) {
                let result = layer.read(resolved);
                match &result {
                    Ok(bytes) => {
                        if resolved.as_string() != path.as_string() {
                            self.vfs_log(&format!(
                                "read {} -> ok ({} bytes, redirected: {})",
                                path.as_string(),
                                bytes.len(),
                                resolved.as_string()
                            ));
                        } else {
                            self.vfs_log(&format!("read {} -> ok ({} bytes)", path.as_string(), bytes.len()));
                        }
                    }
                    Err(e) => self.vfs_log(&format!("read {} -> err: {e}", path.as_string())),
                }
                return result;
            }
        }
        self.vfs_log(&format!("read {} -> not found", path.as_string()));
        Err(VfsError::NotFound(path.as_string()))
    }

    fn exists(&self, path: &VfsPath) -> bool {
        if self.override_dests.contains(&path.as_string()) {
            self.vfs_log(&format!("exists {} -> false (override destination)", path.as_string()));
            return false;
        }
        let resolved = self.overrides.get(&path.as_string()).unwrap_or(path);
        let found = self.layers.iter().any(|l| l.exists(resolved));
        if resolved.as_string() != path.as_string() {
            self.vfs_log(&format!(
                "exists {} -> {found} (redirected: {})",
                path.as_string(),
                resolved.as_string()
            ));
        } else {
            self.vfs_log(&format!("exists {} -> {found}", path.as_string()));
        }
        found
    }

    fn list(&self, mod_id: &str, prefix: &str) -> Result<Vec<String>, VfsError> {
        let mut seen = HashSet::new();
        let mut results = Vec::new();
        let mut suppressed = 0usize;
        for layer in self.layers.iter().rev() {
            for entry in layer.list(mod_id, prefix)? {
                let full = format!("{mod_id}://{entry}");
                if self.override_dests.contains(&full) {
                    suppressed += 1;
                    continue;
                }
                if seen.insert(entry.clone()) {
                    results.push(entry);
                }
            }
        }
        let prefix_display = if prefix.is_empty() { String::new() } else { format!("{prefix}/") };
        if suppressed > 0 {
            self.vfs_log(&format!(
                "list {mod_id}://{prefix_display} -> {} entries ({suppressed} suppressed as override destinations)",
                results.len()
            ));
        } else {
            self.vfs_log(&format!("list {mod_id}://{prefix_display} -> {} entries", results.len()));
        }
        Ok(results)
    }
}
