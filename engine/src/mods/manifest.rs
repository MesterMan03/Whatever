use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct ModManifest {
    #[serde(default)]
    pub initial_scene: Option<String>,
    #[serde(rename = "mod")]
    pub meta: ModMeta,
    #[serde(default)]
    pub dependencies: HashMap<String, String>,
    #[serde(default)]
    pub load_order: LoadOrder,
    pub script: Option<ScriptConfig>,
    #[serde(default)]
    pub assets: AssetsConfig,
    #[serde(default)]
    pub overrides: OverridesConfig,
    /// Additional VFS folders. Scenes are mounted automatically.
    #[serde(default)]
    pub mounts: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ModMeta {
    pub id: String,
    #[serde(default = "default_sides")]
    pub sides: Vec<crate::runtime::RuntimeMode>,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct LoadOrder {
    #[serde(default)]
    pub after: Vec<String>,
    #[serde(default)]
    pub before: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ScriptConfig {
    pub entry: Option<String>,
    pub client: Option<String>,
    pub server: Option<String>,
    #[serde(default = "default_runtime")]
    pub runtime: String,
}

fn default_runtime() -> String {
    "bun".to_owned()
}

#[derive(Debug, Deserialize)]
pub struct AssetsConfig {
    #[serde(default = "default_assets_root")]
    pub root: String,
}

impl Default for AssetsConfig {
    fn default() -> Self {
        AssetsConfig {
            root: default_assets_root(),
        }
    }
}

fn default_assets_root() -> String {
    "assets".to_owned()
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
pub struct OverridesConfig {
    #[serde(default)]
    pub namespaces: Vec<String>,
    #[serde(flatten)]
    pub paths: HashMap<String, String>,
}

fn default_sides() -> Vec<crate::runtime::RuntimeMode> {
    vec![
        crate::runtime::RuntimeMode::Client,
        crate::runtime::RuntimeMode::Server,
    ]
}
impl ScriptConfig {
    pub fn entry_for(&self, mode: crate::runtime::RuntimeMode) -> Option<&str> {
        match mode {
            crate::runtime::RuntimeMode::Client => self.client.as_deref(),
            crate::runtime::RuntimeMode::Server => self.server.as_deref(),
        }
        .or(self.entry.as_deref())
    }
}
