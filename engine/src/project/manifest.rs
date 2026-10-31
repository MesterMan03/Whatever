use crate::{
    debug::{DebugConfig, DebugLogger},
    mods::{ModManifest, ModRegistry, discover_and_load},
    runtime::RuntimeMode,
    vfs::LayeredVfs,
};
use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub id: String,
    /// Paths to individual ordinary mod directories, relative to the project file.
    pub mods: Vec<String>,
    #[serde(default)]
    pub build: BuildSettings,
    #[serde(skip)]
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BuildSettings {
    pub exclude_mods: Vec<String>,
    pub icon: Option<String>,
    pub company: String,
    pub version: String,
}
impl Default for BuildSettings {
    fn default() -> Self {
        Self {
            exclude_mods: Vec::new(),
            icon: None,
            company: String::new(),
            version: "0.1.0".into(),
        }
    }
}

impl Project {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let path = path
            .canonicalize()
            .with_context(|| format!("open {}", path.display()))?;
        let path = if path.is_dir() {
            path.join("whatever.wproject")
        } else {
            path
        };
        if path.is_file() {
            let mut project: Self =
                toml::from_str(&std::fs::read_to_string(&path)?).context("parse .wproject")?;
            project.path = path;
            if project.id.is_empty() {
                project.id = project.game_id();
            }
            ensure!(
                project.version == 1,
                "unsupported project version {}",
                project.version
            );
            return Ok(project);
        }
        let root = path.parent().context("project directory")?;
        let mut mods = Vec::new();
        for directory in [root.join("mods"), root.join("mods_user")] {
            if !directory.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(directory)? {
                let entry = entry?;
                if entry.path().join("mod.toml").is_file() {
                    mods.push(
                        entry
                            .path()
                            .strip_prefix(root)?
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        mods.sort();
        ensure!(
            !mods.is_empty(),
            "no whatever.wproject or mod collection found"
        );
        Ok(Self {
            version: 1,
            id: String::new(),
            name: root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            mods,
            build: BuildSettings::default(),
            path,
        })
    }

    pub fn roots(&self) -> anyhow::Result<Vec<PathBuf>> {
        ensure!(self.version == 1, "unsupported project version");
        ensure!(!self.mods.is_empty(), "project has no mods");
        let parent = self.path.parent().context("project path has no parent")?;
        self.mods
            .iter()
            .map(|path| {
                let root = parent
                    .join(path)
                    .canonicalize()
                    .with_context(|| format!("mod directory '{path}'"))?;
                ensure!(
                    root.join("mod.toml").is_file(),
                    "{} is not a mod directory",
                    root.display()
                );
                Ok(root)
            })
            .collect()
    }

    pub fn game_id(&self) -> String {
        if !self.id.is_empty() {
            return self.id.clone();
        }
        let id: String = self
            .name
            .to_ascii_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let id = id.trim_matches('_');
        if id.is_empty() {
            "game".into()
        } else {
            id.into()
        }
    }
    pub fn for_build(&self) -> anyhow::Result<Self> {
        let manifests = self.manifests()?;
        for excluded in &self.build.exclude_mods {
            ensure!(excluded != "core", "the core mod cannot be excluded");
            ensure!(
                manifests.iter().any(|(_, m)| m.meta.id == *excluded),
                "excluded mod '{excluded}' is not in this project"
            );
        }
        let mut result = self.clone();
        result.mods = self
            .mods
            .iter()
            .zip(manifests.iter())
            .filter(|(_, (_, m))| !self.build.exclude_mods.contains(&m.meta.id))
            .map(|(path, _)| path.clone())
            .collect();
        result.build.exclude_mods.clear();
        result.validate()?;
        Ok(result)
    }
    pub fn manifests(&self) -> anyhow::Result<Vec<(PathBuf, ModManifest)>> {
        self.roots()?
            .into_iter()
            .map(|root| {
                let manifest = toml::from_str(&std::fs::read_to_string(root.join("mod.toml"))?)?;
                Ok((root, manifest))
            })
            .collect()
    }

    pub fn registry(&self, mode: RuntimeMode) -> anyhow::Result<(ModRegistry, LayeredVfs)> {
        let roots = self.roots()?;
        let sink = std::fs::OpenOptions::new()
            .write(true)
            .open(if cfg!(windows) { "NUL" } else { "/dev/null" })?;
        let mut debug = DebugLogger::new(
            &DebugConfig::from_args(&[]),
            Arc::new(Mutex::new(std::io::BufWriter::new(sink))),
        );
        let mut registry = ModRegistry::new();
        let mut vfs = LayeredVfs::new();
        discover_and_load(
            &roots.iter().map(PathBuf::as_path).collect::<Vec<_>>(),
            mode,
            &mut vfs,
            &mut registry,
            &mut debug,
        )?;
        ensure!(
            registry.get("core").is_some(),
            "project needs the ordinary core mod for shared engine resources"
        );
        Ok((registry, vfs))
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        ensure!(
            crate::sandbox::valid_storage_id(&self.game_id()),
            "invalid game ID"
        );
        use crate::vfs::Vfs;
        for mode in [RuntimeMode::Client, RuntimeMode::Server] {
            let (registry, vfs) = self.registry(mode)?;
            if let Some(path) = registry.startup_scene()? {
                crate::scene::SceneDocument::parse(&vfs.read(&crate::scene::scene_path(path)?)?)?;
            }
            for item in registry.iter() {
                if let Some(entry) = item
                    .manifest
                    .script
                    .as_ref()
                    .and_then(|script| script.entry_for(mode))
                {
                    ensure!(
                        crate::sandbox::resolve_contained(&item.root, entry)?.is_file(),
                        "missing {mode} script '{entry}'"
                    );
                }
                for path in vfs.list(&item.manifest.meta.id, "scenes")? {
                    if path.ends_with(".wscene") {
                        let path = crate::vfs::VfsPath {
                            mod_id: item.manifest.meta.id.clone(),
                            path,
                        };
                        crate::scene::SceneDocument::parse(&vfs.read(&path)?)
                            .with_context(|| format!("scene {}", path.as_string()))?;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn save(&self) -> anyhow::Result<()> {
        self.roots()?;
        std::fs::write(&self.path, toml::to_string_pretty(self)?).context("save project")
    }
}
