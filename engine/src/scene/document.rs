use crate::ecs::{EntityId, World, resolve_references};
use crate::vfs::{Vfs, VfsPath};
use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// An authored scene contains no runtime entity IDs. Children encode parentage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneDocument {
    pub version: u32,
    pub entities: Vec<SceneEntity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneEntity {
    #[serde(default = "default_enabled", skip_serializing_if = "is_enabled")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_id: Option<String>,
    #[serde(default)]
    pub components: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SceneEntity>,
}

fn default_enabled() -> bool {
    true
}
fn is_enabled(value: &bool) -> bool {
    *value
}
impl Default for SceneEntity {
    fn default() -> Self {
        Self {
            enabled: true,
            name: String::new(),
            script_id: None,
            components: Default::default(),
            tags: Vec::new(),
            children: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadedScene {
    pub path: String,
    pub entity_ids: Vec<String>,
    pub script_ids: BTreeMap<String, String>,
    pub load_time: f64,
}

pub fn scene_path(path: &str) -> anyhow::Result<VfsPath> {
    let mut path = VfsPath::parse(path).context("expected mod_id://scene path")?;
    ensure!(
        crate::sandbox::valid_storage_id(&path.mod_id),
        "invalid scene namespace"
    );
    if !path.path.starts_with("scenes/") {
        path.path = format!("scenes/{}", path.path);
    }
    if !path.path.ends_with(".wscene") {
        path.path.push_str(".wscene");
    }
    Ok(path)
}

impl Default for SceneDocument {
    fn default() -> Self {
        Self {
            version: 1,
            entities: Vec::new(),
        }
    }
}

impl SceneDocument {
    pub fn parse(bytes: &[u8]) -> anyhow::Result<Self> {
        let document: Self = serde_json::from_slice(bytes).context("parse .wscene JSON")?;
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        let mut world = World::new();
        world.allow_missing_components = true;
        self.instantiate(&mut world).map(|_| ())
    }

    pub fn load(vfs: &dyn Vfs, path: &str, world: &mut World) -> anyhow::Result<LoadedScene> {
        let start = std::time::Instant::now();
        let bytes = vfs.read(&scene_path(path)?)?;
        let document: Self = serde_json::from_slice(&bytes).context("parse .wscene JSON")?;
        let mut loaded = document.instantiate(world)?;
        loaded.path = path.to_owned();
        loaded.load_time = start.elapsed().as_secs_f64() * 1000.0;
        Ok(loaded)
    }

    /// Allocate fresh IDs per instance. Roll back the whole instance on failure.
    pub fn instantiate(&self, world: &mut World) -> anyhow::Result<LoadedScene> {
        ensure!(
            self.version == 1,
            "unsupported scene version {}",
            self.version
        );
        fn flatten<'a>(
            items: &'a [SceneEntity],
            parent: Option<usize>,
            depth: usize,
            out: &mut Vec<(&'a SceneEntity, Option<usize>)>,
        ) -> anyhow::Result<()> {
            ensure!(depth <= 64, "scene hierarchy exceeds 64 levels");
            for item in items {
                let index = out.len();
                ensure!(index < 100_000, "scene exceeds 100000 entities");
                out.push((item, parent));
                flatten(&item.children, Some(index), depth + 1, out)?;
            }
            Ok(())
        }
        let mut entries = Vec::new();
        flatten(&self.entities, None, 0, &mut entries)?;
        let mut names = BTreeMap::new();
        for (index, (entity, _)) in entries.iter().enumerate() {
            if let Some(name) = &entity.script_id {
                ensure!(!name.trim().is_empty(), "script_id cannot be empty");
                ensure!(
                    names.insert(name.clone(), index).is_none(),
                    "duplicate script_id '{name}'"
                );
            }
        }
        let ids: Vec<EntityId> = entries.iter().map(|_| world.create_entity()).collect();
        let mut physics = crate::physics::PhysicsRegistry::default();
        let result = (|| -> anyhow::Result<()> {
            for (index, (entity, parent)) in entries.iter().enumerate() {
                world.names.insert(
                    ids[index].index,
                    if entity.name.is_empty() {
                        entity.script_id.clone().unwrap_or_default()
                    } else {
                        entity.name.clone()
                    },
                );
                world.set_enabled(&ids[index], entity.enabled);
                world.set_tags(&ids[index], entity.tags.clone())?;
                if let Some(parent) = parent {
                    ensure!(
                        world.set_parent(ids[index], Some(ids[*parent])),
                        "invalid scene hierarchy"
                    );
                }
                for (kind, data) in &entity.components {
                    ensure!(
                        kind.contains(':') && !kind.ends_with(':'),
                        "invalid component type '{kind}'"
                    );
                    let mut data = data.clone();
                    resolve_references(&mut data, &mut |name| names.get(name).map(|i| ids[*i]))
                        .with_context(|| format!("component '{kind}' on '{}'", entity.name))?;
                    // Scene instantiation must not allocate an implicit world outside the instance.
                    // Explicit authored references also make physics ownership visible in the editor.
                    if kind == "core:rigid_body" {
                        ensure!(
                            !data["world"].is_null(),
                            "scene rigid bodies must reference an authored physics world"
                        );
                    }
                    if kind == "core:collider" && data.get("body").is_some_and(Value::is_null) {
                        ensure!(
                            !data["world"].is_null(),
                            "standalone scene colliders must reference an authored physics world"
                        );
                    }
                    let data = physics
                        .normalize_component(world, ids[index], kind, data)
                        .with_context(|| format!("component '{kind}' on '{}'", entity.name))?;
                    ensure!(
                        world.set_component(&ids[index], kind, data),
                        "invalid component '{kind}' on '{}'",
                        entity.name
                    );
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            for id in &ids {
                world.destroy_entity(*id);
            }
            return Err(error);
        }
        Ok(LoadedScene {
            path: String::new(),
            entity_ids: ids.iter().map(ToString::to_string).collect(),
            script_ids: names
                .into_iter()
                .map(|(name, i)| (name, ids[i].to_string()))
                .collect(),
            load_time: 0.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn instances_remap_references_and_parentage_without_touching_strings() {
        let document: SceneDocument = serde_json::from_value(json!({"version":1,"entities":[{
            "script_id":"root","children":[{"script_id":"child","components":{
                "test:link":{"target":{"$entity":"root"},"text":"root"}
            }}]
        }]}))
        .expect("scene");
        let mut world = World::new();
        crate::ecs::register_test_component(
            &mut world,
            "test:link",
            &[
                (
                    "target",
                    crate::schema::Schema::Nullable {
                        inner: Box::new(crate::schema::Schema::Entity),
                    },
                    Value::Null,
                ),
                (
                    "text",
                    crate::schema::Schema::String { max_length: None },
                    json!(""),
                ),
            ],
        );
        let first = document.instantiate(&mut world).expect("first");
        let second = document.instantiate(&mut world).expect("second");
        assert_ne!(first.entity_ids, second.entity_ids);
        let child = EntityId::parse(&second.script_ids["child"]).expect("id");
        assert_eq!(
            world.get_parent(&child).map(|id| id.to_string()),
            Some(second.script_ids["root"].clone())
        );
        assert_eq!(
            world.get_component(&child, "test:link"),
            Some(json!({"target":{"$entity":second.script_ids["root"]},"text":"root"}))
        );
    }

    #[test]
    fn invalid_scenes_are_atomic_and_do_not_destroy_existing_entities() {
        let mut world = World::new();
        let existing = world.create_entity();
        for scene in [
            json!({"version":9,"entities":[]}),
            json!({"version":1,"entities":[{"script_id":"a"},{"script_id":"a"}]}),
            json!({"version":1,"entities":[{}, {"components":{"test:link":{"$entity":"missing"}}}]}),
            json!({"version":1,"entities":[{"children":[{"components":{"core:transform":{}}}]}]}),
            json!({"version":1,"entities":[{"components":{"core:rigid_body":{"type":"dynamic"}}}]}),
            json!({"version":1,"entities":[{"components":{"core:collider":{"body":null,"shape":{"type":"box","size":[1,1,1]}}}}]}),
            json!({"version":1,"entities":[{"components":{"core:physics_world":{"gravity":[0,"invalid",0]}}}]}),
        ] {
            let document: SceneDocument = serde_json::from_value(scene).expect("shape");
            assert!(document.instantiate(&mut world).is_err());
            assert_eq!(
                world.allocator.alive_entity_ids().collect::<Vec<_>>(),
                vec![existing]
            );
        }
    }
}
