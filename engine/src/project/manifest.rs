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
        self.validate_build_selection()?;
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

    /// Fast dependency checks for the build-settings UI, without source inspection.
    pub fn validate_build_selection(&self) -> anyhow::Result<()> {
        let manifests = self.manifests()?;
        let excluded = &self.build.exclude_mods;
        ensure!(
            !excluded.iter().any(|id| id == "core"),
            "the core mod cannot be excluded"
        );
        for id in excluded {
            ensure!(
                manifests.iter().any(|(_, m)| &m.meta.id == id),
                "unknown excluded mod '{id}'"
            );
        }
        for (_, manifest) in &manifests {
            if excluded.contains(&manifest.meta.id) {
                continue;
            }
            for dependency in manifest.dependencies.keys() {
                ensure!(
                    !excluded.contains(dependency),
                    "cannot exclude '{dependency}': included mod '{}' depends on it",
                    manifest.meta.id
                );
            }
        }
        Ok(())
    }

    pub fn validate_scene(
        &self,
        path: &Path,
        scene: &crate::scene::SceneDocument,
        vfs: &dyn crate::vfs::Vfs,
        definitions: &std::collections::BTreeMap<String, crate::ecs::ComponentDefinition>,
    ) -> anyhow::Result<()> {
        let path = super::portable_path(&std::path::absolute(path)?);
        let (root, owner) = self
            .manifests()?
            .into_iter()
            .filter(|(root, _)| path.starts_with(super::portable_path(root)))
            .max_by_key(|(root, _)| root.components().count())
            .context("scene is outside project mods")?;
        crate::scene::validate_references(scene, &owner, vfs, definitions).with_context(|| {
            format!(
                "scene {} in mod '{}'",
                path.strip_prefix(root).unwrap_or(&path).display(),
                owner.meta.id
            )
        })
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
            "project needs a core mod to define the game"
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
            let mut schemas = crate::ecs::World::new();
            crate::script::load_component_types(&mut schemas, &registry)?;
            if let Some(path) = registry.startup_scene()? {
                let target = crate::scene::scene_path(path)?;
                let core = registry.get("core").context("core mod")?;
                ensure!(
                    crate::scene::allows_namespace(core, &target.mod_id),
                    "core must declare a dependency on '{}' for its initial scene",
                    target.mod_id
                );
            }
            for item in registry.iter() {
                if let Some(path) = &item.manifest.initial_scene {
                    let target = crate::scene::scene_path(path)?;
                    ensure!(
                        crate::scene::allows_namespace(&item.manifest, &target.mod_id),
                        "mod '{}' must depend on '{}' for its initial scene",
                        item.manifest.meta.id,
                        target.mod_id
                    );
                    ensure!(vfs.exists(&target), "missing initial scene '{path}'");
                }
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
                        let scene = crate::scene::SceneDocument::parse(&vfs.read(&path)?)?;
                        crate::scene::validate_references(
                            &scene,
                            &item.manifest,
                            &vfs,
                            &schemas.component_types,
                        )
                        .with_context(|| format!("scene {}", path.as_string()))?;
                        scene.instantiate(&mut schemas.clone())?;
                    }
                }
                for path in vfs.list(&item.manifest.meta.id, "")? {
                    if path.ends_with(".wprefab")
                        && !path
                            .split('/')
                            .any(|part| [".git", ".whatever", "node_modules"].contains(&part))
                    {
                        let path = crate::vfs::VfsPath {
                            mod_id: item.manifest.meta.id.clone(),
                            path,
                        };
                        let prefab = crate::scene::PrefabDocument::parse(&vfs.read(&path)?)?;
                        crate::scene::validate_references(
                            &prefab.scene(),
                            &item.manifest,
                            &vfs,
                            &schemas.component_types,
                        )
                        .with_context(|| format!("prefab {}", path.as_string()))?;
                        prefab.scene().instantiate(&mut schemas.clone())?;
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
