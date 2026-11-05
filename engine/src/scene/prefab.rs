use super::{LoadedScene, SceneDocument, SceneEntity};
use crate::{
    ecs::World,
    mods::ModRegistry,
    vfs::{Vfs, VfsPath},
};
use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};

/// A reusable template containing exactly one root and optional descendants.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrefabDocument {
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category: String,
    pub entity: SceneEntity,
}
impl PrefabDocument {
    pub fn parse(bytes: &[u8]) -> anyhow::Result<Self> {
        let prefab: Self = serde_json::from_slice(bytes).context("parse .wprefab JSON")?;
        ensure!(prefab.version == 1, "unsupported prefab version");
        ensure!(
            !prefab.name.trim().is_empty(),
            "prefab needs a display name"
        );
        prefab.scene().validate()?;
        Ok(prefab)
    }

    pub fn scene(&self) -> SceneDocument {
        let mut entity = self.entity.clone();
        if entity.name.is_empty() {
            entity.name = self.name.clone();
        }
        SceneDocument {
            version: 1,
            entities: vec![entity],
        }
    }

    pub fn load_checked(
        vfs: &dyn Vfs,
        path: &str,
        world: &mut World,
        registry: &ModRegistry,
    ) -> anyhow::Result<LoadedScene> {
        let start = std::time::Instant::now();
        let target = prefab_path(path)?;
        let owner = registry
            .get(&target.mod_id)
            .context("prefab-owning mod is unavailable")?;
        let prefab = Self::parse(&vfs.read(&target)?)?;
        let scene = prefab.scene();
        super::validate_references(&scene, owner, vfs, &world.component_types)
            .with_context(|| format!("prefab '{path}'"))?;
        let mut instance = scene.instantiate(world)?;
        instance.path = target.as_string();
        instance.load_time = start.elapsed().as_secs_f64() * 1000.0;
        Ok(instance)
    }
}

pub fn prefab_path(path: &str) -> anyhow::Result<VfsPath> {
    let mut path = VfsPath::parse(path).context("expected mod_id://prefab path")?;
    ensure!(
        crate::sandbox::valid_storage_id(&path.mod_id),
        "invalid prefab namespace"
    );
    // Full VFS paths allow imported libraries and custom mounts; shorthand uses prefabs/.
    if !path.path.contains('/') {
        path.path = format!("prefabs/{}", path.path);
    }
    if !path.path.ends_with(".wprefab") {
        path.path.push_str(".wprefab");
    }
    Ok(path)
}
