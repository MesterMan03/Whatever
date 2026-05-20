use super::{Vfs, VfsError, VfsPath};
use std::collections::HashMap;

pub struct LayeredVfs {
    layers: Vec<Box<dyn Vfs>>,
    overrides: HashMap<String, VfsPath>,
}

impl LayeredVfs {
    pub fn new() -> Self {
        LayeredVfs {
            layers: Vec::new(),
            overrides: HashMap::new(),
        }
    }

    pub fn push_layer(&mut self, layer: Box<dyn Vfs>) {
        self.layers.push(layer);
    }

    pub fn add_override(&mut self, from: VfsPath, to: VfsPath) {
        self.overrides.insert(from.as_string(), to);
    }
}

impl Vfs for LayeredVfs {
    fn read(&self, path: &VfsPath) -> Result<Vec<u8>, VfsError> {
        let resolved = self.overrides.get(&path.as_string()).unwrap_or(path);
        for layer in self.layers.iter().rev() {
            if layer.exists(resolved) {
                return layer.read(resolved);
            }
        }
        Err(VfsError::NotFound(path.as_string()))
    }

    fn exists(&self, path: &VfsPath) -> bool {
        let resolved = self.overrides.get(&path.as_string()).unwrap_or(path);
        self.layers.iter().any(|l| l.exists(resolved))
    }

    fn list(&self, mod_id: &str, prefix: &str) -> Result<Vec<String>, VfsError> {
        // Paths that serve as override destinations should not appear as independent assets
        let override_targets: std::collections::HashSet<&str> = self
            .overrides
            .values()
            .filter(|p| p.mod_id == mod_id)
            .map(|p| p.path.as_str())
            .collect();

        let mut seen = std::collections::HashSet::new();
        let mut results = Vec::new();
        for layer in self.layers.iter().rev() {
            for entry in layer.list(mod_id, prefix)? {
                if override_targets.contains(entry.as_str()) {
                    continue;
                }
                if seen.insert(entry.clone()) {
                    results.push(entry);
                }
            }
        }
        Ok(results)
    }
}
