//! Trusted editor controls. Applied on the simulation thread at a tick boundary.
use crate::{
    ecs::{EntityId, World, resolve_references},
    physics::PhysicsRegistry,
    scene::{SceneDocument, SceneEntity},
};
use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValueEdit {
    pub path: Vec<String>,
    pub value: Option<Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityEdit {
    pub id: String,
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub components: BTreeMap<String, Vec<ValueEdit>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ParentEdit {
    pub id: String,
    pub parent: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SiblingOrder {
    pub parent: Option<String>,
    pub children: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EditorBatch {
    pub edits: Vec<EntityEdit>,
    pub parents: Vec<ParentEdit>,
    pub destroy: Vec<String>,
    #[serde(default)]
    pub orders: Vec<SiblingOrder>,
    pub create: Vec<(Option<String>, SceneEntity)>,
}
impl EditorBatch {
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
            && self.orders.is_empty()
            && self.parents.is_empty()
            && self.destroy.is_empty()
            && self.create.is_empty()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditorReply {
    pub sequence: u64,
    pub created: HashMap<String, String>,
    pub error: Option<String>,
}
pub struct EditorControl {
    pub sequence: u64,
    pub batch: EditorBatch,
    pub reply: std::sync::mpsc::SyncSender<EditorReply>,
}
fn entity(world: &World, text: &str) -> anyhow::Result<EntityId> {
    EntityId::parse(text)
        .filter(|id| world.is_alive(id))
        .context("entity no longer exists")
}
fn set_value(data: &mut Value, edit: &ValueEdit) -> anyhow::Result<()> {
    if edit.path.is_empty() {
        *data = edit.value.clone().unwrap_or(Value::Null);
        return Ok(());
    }
    let (last, parents) = edit.path.split_last().context("property path")?;
    let mut cursor = data;
    for key in parents {
        cursor = match cursor {
            Value::Object(map) => map.get_mut(key),
            Value::Array(items) => key.parse::<usize>().ok().and_then(|i| items.get_mut(i)),
            _ => None,
        }
        .context("property no longer exists")?;
    }
    match cursor {
        Value::Object(map) => {
            if let Some(value) = &edit.value {
                map.insert(last.clone(), value.clone());
            } else {
                map.remove(last);
            }
        }
        Value::Array(items) => {
            let index = last.parse::<usize>()?;
            let slot = items
                .get_mut(index)
                .context("array property no longer exists")?;
            *slot = edit
                .value
                .clone()
                .context("array elements cannot be removed")?;
        }
        _ => anyhow::bail!("property parent is not editable"),
    };
    Ok(())
}
/// Changes to individual leaves preserve fields that physics/scripts updated since the last snapshot.
pub fn apply_editor_batch(
    world: &mut World,
    batch: &EditorBatch,
) -> anyhow::Result<HashMap<String, String>> {
    let mut candidate = world.clone();
    let mut physics = PhysicsRegistry::default();
    let mut created = HashMap::new();
    let mut entries = Vec::new();
    fn allocate(
        world: &mut World,
        node: &SceneEntity,
        parent: Option<EntityId>,
        entries: &mut Vec<(EntityId, SceneEntity, Option<EntityId>)>,
        created: &mut HashMap<String, String>,
        depth: usize,
    ) -> anyhow::Result<()> {
        ensure!(
            depth <= 64 && entries.len() < 100_000,
            "editor hierarchy exceeds limits"
        );
        let id = world.create_entity();
        if let Some(key) = &node.script_id {
            ensure!(
                created.insert(key.clone(), id.to_string()).is_none(),
                "duplicate new entity ID"
            );
        }
        entries.push((id, node.clone(), parent));
        for child in &node.children {
            allocate(world, child, Some(id), entries, created, depth + 1)?;
        }
        Ok(())
    }
    for (parent, node) in &batch.create {
        let parent = parent
            .as_deref()
            .map(|id| entity(&candidate, id))
            .transpose()?;
        allocate(&mut candidate, node, parent, &mut entries, &mut created, 0)?;
    }
    let resolve = |text: &str| {
        created
            .get(text)
            .and_then(|s| EntityId::parse(s))
            .or_else(|| EntityId::parse(text))
    };
    for (id, node, parent) in entries {
        candidate.names.insert(id.index, node.name);
        candidate.set_enabled(&id, node.enabled);
        ensure!(candidate.set_parent(id, parent), "invalid parent");
        for (kind, mut data) in node.components {
            if read_only(&kind) {
                continue;
            }
            resolve_references(&mut data, &mut |s| {
                resolve(s).filter(|id| candidate.is_alive(id))
            })?;
            let data = physics.normalize_component(&mut candidate, id, &kind, data)?;
            ensure!(
                candidate.set_component(&id, &kind, data),
                "invalid component {kind}"
            );
        }
    }
    for change in &batch.parents {
        let id = entity(&candidate, &change.id)?;
        let parent = change
            .parent
            .as_deref()
            .map(|p| entity(&candidate, p))
            .transpose()?;
        ensure!(
            candidate.set_parent(id, parent),
            "invalid parent or hierarchy cycle"
        );
    }
    for change in &batch.edits {
        let id = entity(&candidate, &change.id)?;
        if let Some(name) = &change.name {
            candidate.names.insert(id.index, name.clone());
        }
        if let Some(enabled) = change.enabled {
            candidate.set_enabled(&id, enabled);
        }
        for (kind, edits) in &change.components {
            ensure!(!read_only(kind), "{kind} is read-only");
            if edits.len() == 1 && edits[0].path.is_empty() && edits[0].value.is_none() {
                candidate.remove_component(&id, kind);
                continue;
            }
            let mut data = candidate.get_component(&id, kind).unwrap_or(Value::Null);
            for edit in edits {
                set_value(&mut data, edit)?;
            }
            resolve_references(&mut data, &mut |s| {
                resolve(s).filter(|id| candidate.is_alive(id))
            })?;
            let data = physics.normalize_component(&mut candidate, id, kind, data)?;
            ensure!(
                candidate.set_component(&id, kind, data),
                "invalid component {kind}"
            );
        }
    }
    for id in &batch.destroy {
        if let Some(id) = EntityId::parse(id).filter(|id| candidate.is_alive(id)) {
            candidate.destroy_entity(id);
        }
    }
    for order in &batch.orders {
        let parent = order
            .parent
            .as_deref()
            .and_then(resolve)
            .filter(|id| candidate.is_alive(id));
        if order.parent.is_some() && parent.is_none() {
            continue;
        }
        let mut ids = Vec::new();
        for text in &order.children {
            if let Some(id) = resolve(text)
                .filter(|id| candidate.is_alive(id) && candidate.get_parent(id) == parent)
                && !ids.contains(&id)
            {
                ids.push(id);
            }
        }
        let existing = parent
            .map(|p| candidate.get_children(&p))
            .unwrap_or_else(|| {
                candidate
                    .allocator
                    .alive_entity_ids()
                    .filter(|id| candidate.get_parent(id).is_none())
                    .collect()
            });
        for id in existing {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        if let Some(parent) = parent {
            candidate
                .children
                .insert(parent.index, ids.into_iter().map(|id| id.index).collect());
        } else {
            candidate.root_order = ids;
        }
    }
    *world = candidate;
    Ok(created)
}
pub fn read_only(kind: &str) -> bool {
    matches!(
        kind,
        "core:entity" | "core:world_transform" | "core:rigid_body_state" | "core:physics_status"
    )
}
pub fn editor_scene(world: &World) -> SceneDocument {
    let mut kinds = vec![
        "core:transform",
        "core:sprite_renderer",
        "core:text_renderer",
        "core:mesh_renderer",
        "core:camera",
        "core:ambient_light",
        "core:directional_light",
        "core:point_light",
    ];
    kinds.extend(
        world
            .custom
            .keys()
            .map(String::as_str)
            .filter(|k| !read_only(k)),
    );
    kinds.sort_unstable();
    kinds.dedup();
    fn node(
        world: &World,
        id: EntityId,
        kinds: &[&str],
        seen: &mut HashSet<EntityId>,
        depth: usize,
    ) -> Option<SceneEntity> {
        if !seen.insert(id) || seen.len() > 100_000 {
            return None;
        }
        Some(SceneEntity {
            script_id: Some(id.to_string()),
            name: world
                .names
                .get(&id.index)
                .cloned()
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| format!("Entity {id}")),
            enabled: world.enabled(&id),
            components: kinds
                .iter()
                .filter_map(|kind| {
                    world
                        .get_component(&id, kind)
                        .map(|data| ((*kind).to_owned(), data))
                })
                .collect(),
            children: if depth >= 64 {
                Vec::new()
            } else {
                world
                    .get_children(&id)
                    .into_iter()
                    .filter_map(|id| node(world, id, kinds, seen, depth + 1))
                    .collect()
            },
        })
    }
    let mut seen = HashSet::new();
    let mut entities: Vec<_> = world
        .root_order
        .iter()
        .copied()
        .filter(|id| world.is_alive(id))
        .filter(|id| world.get_parent(id).is_none())
        .filter_map(|id| node(world, id, &kinds, &mut seen, 0))
        .collect();
    for id in world.allocator.alive_entity_ids() {
        if !seen.contains(&id)
            && let Some(entity) = node(world, id, &kinds, &mut seen, 0)
        {
            entities.push(entity);
        }
    }
    SceneDocument {
        version: 1,
        entities,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn edits_are_atomic_and_do_not_overwrite_newer_simulation_fields() {
        let mut w = World::new();
        let id = w.create_entity();
        w.set_component(
            &id,
            "core:transform",
            json!({"position":[10,20,30],"rotation":[0,0,0,1],"scale":[1,1,1]}),
        );
        let mut batch = EditorBatch::default();
        batch.edits.push(EntityEdit {
            id: id.to_string(),
            name: Some("Live cube".into()),
            enabled: Some(false),
            components: BTreeMap::from([(
                "core:transform".into(),
                vec![ValueEdit {
                    path: vec!["position".into(), "1".into()],
                    value: Some(json!(7)),
                }],
            )]),
        });
        apply_editor_batch(&mut w, &batch).expect("edit");
        assert_eq!(w.transforms[&id.index].position, [10.0, 7.0, 30.0]);
        assert!(!w.enabled(&id));
        batch.edits[0]
            .components
            .insert("core:rigid_body_state".into(), vec![]);
        batch.edits[0].name = Some("Rejected".into());
        assert!(apply_editor_batch(&mut w, &batch).is_err());
        assert_eq!(w.names[&id.index], "Live cube");
        assert_eq!(editor_scene(&w).entities[0].name, "Live cube");
    }
    #[test]
    fn live_ordering_preserves_additional_roots_and_reorders_siblings() {
        let mut world = World::new();
        let a = world.create_entity();
        let b = world.create_entity();
        let c = world.create_entity();
        let extra = world.create_entity();
        apply_editor_batch(
            &mut world,
            &EditorBatch {
                orders: vec![SiblingOrder {
                    parent: None,
                    children: vec![c.to_string(), a.to_string(), b.to_string()],
                }],
                ..Default::default()
            },
        )
        .expect("root order");
        assert_eq!(world.root_order, vec![c, a, b, extra]);
        assert_eq!(
            editor_scene(&world).entities[0].script_id,
            Some(c.to_string())
        );
        apply_editor_batch(
            &mut world,
            &EditorBatch {
                parents: vec![
                    ParentEdit {
                        id: b.to_string(),
                        parent: Some(a.to_string()),
                    },
                    ParentEdit {
                        id: c.to_string(),
                        parent: Some(a.to_string()),
                    },
                ],
                orders: vec![SiblingOrder {
                    parent: Some(a.to_string()),
                    children: vec![c.to_string(), b.to_string()],
                }],
                ..Default::default()
            },
        )
        .expect("child order");
        assert_eq!(world.get_children(&a), vec![c, b]);
        assert!(!world.root_order.contains(&b));
        assert!(!world.root_order.contains(&c));
    }
    #[test]
    fn creation_resolves_existing_and_new_references_and_rejects_cycles_atomically() {
        use serde_json::json;
        let mut w = World::new();
        let physics = w.create_entity();
        w.set_component(&physics, "core:physics_world", json!({}));
        let root = SceneEntity {
            script_id: Some("new_body".into()),
            name: "Spawned cube".into(),
            components: BTreeMap::from([
                (
                    "core:transform".into(),
                    json!({"position":[0,4,0],"rotation":[0,0,0,1],"scale":[1,1,1]}),
                ),
                (
                    "core:rigid_body".into(),
                    json!({"type":"dynamic","world":{"$entity":physics.to_string()}}),
                ),
            ]),
            children: vec![SceneEntity {
                script_id: Some("new_collider".into()),
                components: BTreeMap::from([(
                    "core:collider".into(),
                    json!({"body":{"$entity":"new_body"},"shape":{"type":"box","size":[1,1,1]}}),
                )]),
                ..Default::default()
            }],
            ..Default::default()
        };
        let created = apply_editor_batch(
            &mut w,
            &EditorBatch {
                create: vec![(None, root)],
                ..Default::default()
            },
        )
        .expect("create");
        let body = EntityId::parse(&created["new_body"]).expect("body");
        let collider = EntityId::parse(&created["new_collider"]).expect("collider");
        assert_eq!(w.get_parent(&collider), Some(body));
        assert_eq!(
            w.custom["core:collider"][&collider.index]["body"],
            json!({"$entity":body.to_string()})
        );
        assert_eq!(w.names[&body.index], "Spawned cube");
        let before = w.parents.clone();
        assert!(
            apply_editor_batch(
                &mut w,
                &EditorBatch {
                    parents: vec![ParentEdit {
                        id: body.to_string(),
                        parent: Some(collider.to_string())
                    }],
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert_eq!(w.parents, before);
        apply_editor_batch(
            &mut w,
            &EditorBatch {
                destroy: vec![body.to_string()],
                ..Default::default()
            },
        )
        .expect("destroy");
        assert!(!w.is_alive(&body));
        assert!(!w.is_alive(&collider));
        assert!(!w.names.contains_key(&body.index));
        assert!(
            apply_editor_batch(
                &mut w,
                &EditorBatch {
                    edits: vec![EntityEdit {
                        id: body.to_string(),
                        name: Some("stale".into()),
                        enabled: None,
                        components: BTreeMap::new()
                    }],
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}
