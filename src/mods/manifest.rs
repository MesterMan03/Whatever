use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct ModManifest {
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
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ModMeta {
    pub id: String,
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
    pub entry: String,
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
