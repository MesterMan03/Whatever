use super::manifest::ModManifest;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct LoadedMod {
    pub manifest: ModManifest,
    pub root: PathBuf,
}

pub struct ModRegistry {
    mods: Vec<LoadedMod>,
    id_to_index: HashMap<String, usize>,
}

impl ModRegistry {
    pub fn new() -> Self {
        ModRegistry {
            mods: Vec::new(),
            id_to_index: HashMap::new(),
        }
    }

    pub fn register(&mut self, loaded: LoadedMod) {
        let id = loaded.manifest.meta.id.clone();
        let idx = self.mods.len();
        self.mods.push(loaded);
        self.id_to_index.insert(id, idx);
    }

    pub fn iter(&self) -> impl Iterator<Item = &LoadedMod> {
        self.mods.iter()
    }

    pub fn mod_ids(&self) -> impl Iterator<Item = &str> {
        self.mods.iter().map(|m| m.manifest.meta.id.as_str())
    }
}
