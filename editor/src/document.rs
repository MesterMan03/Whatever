use anyhow::{Context, ensure};
use std::path::{Path, PathBuf};
use whatever_engine::scene::{SceneDocument, SceneEntity};

#[derive(Clone)]
struct Snapshot {
    scene: SceneDocument,
    selected: Vec<usize>,
}

pub struct Document {
    pub scene: SceneDocument,
    pub path: PathBuf,
    pub selected: Vec<usize>,
    pub dirty: bool,
    pub transient: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

pub fn entity_mut<'a>(
    entities: &'a mut [SceneEntity],
    path: &[usize],
) -> Option<&'a mut SceneEntity> {
    let (index, tail) = path.split_first()?;
    let entity = entities.get_mut(*index)?;
    if tail.is_empty() {
        Some(entity)
    } else {
        entity_mut(&mut entity.children, tail)
    }
}

pub fn entity_at<'a>(entities: &'a [SceneEntity], path: &[usize]) -> Option<&'a SceneEntity> {
    let (index, tail) = path.split_first()?;
    let entity = entities.get(*index)?;
    if tail.is_empty() {
        Some(entity)
    } else {
        entity_at(&entity.children, tail)
    }
}

fn siblings_mut<'a>(
    scene: &'a mut SceneDocument,
    parent: &[usize],
) -> Option<&'a mut Vec<SceneEntity>> {
    if parent.is_empty() {
        Some(&mut scene.entities)
    } else {
        entity_mut(&mut scene.entities, parent).map(|entity| &mut entity.children)
    }
}

pub fn world_transform(
    scene: &SceneDocument,
    path: &[usize],
) -> anyhow::Result<whatever_engine::ecs::Transform> {
    use glam::{Quat, Vec3};
    let mut position = Vec3::ZERO;
    let mut rotation = Quat::IDENTITY;
    let mut scale = Vec3::ONE;
    for length in 1..=path.len() {
        if let Some(value) = entity_at(&scene.entities, &path[..length])
            .and_then(|entity| entity.components.get("core:transform"))
        {
            let local: whatever_engine::ecs::Transform = serde_json::from_value(value.clone())?;
            position += rotation * (scale * Vec3::from(local.position));
            rotation = (rotation * Quat::from_array(local.rotation)).normalize();
            scale *= Vec3::from(local.scale);
        }
    }
    Ok(whatever_engine::ecs::Transform {
        position: position.to_array(),
        rotation: rotation.to_array(),
        scale: scale.to_array(),
    })
}

impl Document {
    pub fn live(scene: SceneDocument) -> Self {
        Self {
            scene,
            path: PathBuf::from("Running game"),
            selected: Vec::new(),
            dirty: false,
            transient: true,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn move_entity(
        &mut self,
        source: &[usize],
        parent: &[usize],
        index: usize,
    ) -> anyhow::Result<()> {
        let (&source_index, source_parent) =
            source.split_last().context("select an entity to move")?;
        ensure!(
            !parent.starts_with(source),
            "an entity cannot be parented to itself or a descendant"
        );
        let original =
            entity_at(&self.scene.entities, source).context("dragged entity no longer exists")?;
        let preserve =
            original.components.contains_key("core:transform") && parent != source_parent;
        let original_world = world_transform(&self.scene, source)?;
        let parent_world = world_transform(&self.scene, parent)?;
        let mut target_parent = parent.to_vec();
        if target_parent.starts_with(source_parent)
            && target_parent.len() > source_parent.len()
            && target_parent[source_parent.len()] > source_index
        {
            target_parent[source_parent.len()] -= 1;
        }
        let mut target_index = index;
        if source_parent == parent && target_index > source_index {
            target_index -= 1;
        }
        if source_parent == parent && target_index == source_index {
            return Ok(());
        }
        let mut candidate = self.scene.clone();
        let entity = siblings_mut(&mut candidate, source_parent)
            .context("source parent")?
            .remove(source_index);
        let siblings =
            siblings_mut(&mut candidate, &target_parent).context("drop target no longer exists")?;
        ensure!(target_index <= siblings.len(), "invalid insertion position");
        siblings.insert(target_index, entity);
        let mut selection = target_parent;
        selection.push(target_index);
        if preserve {
            use glam::{Quat, Vec3};
            let scale = Vec3::from(parent_world.scale);
            ensure!(
                scale.abs().min_element() > 1.0e-6,
                "cannot preserve position under a parent with zero scale"
            );
            let inverse = Quat::from_array(parent_world.rotation).inverse();
            let local = whatever_engine::ecs::Transform {
                position: ((inverse
                    * (Vec3::from(original_world.position) - Vec3::from(parent_world.position)))
                    / scale)
                    .to_array(),
                rotation: (inverse * Quat::from_array(original_world.rotation))
                    .normalize()
                    .to_array(),
                scale: (Vec3::from(original_world.scale) / scale).to_array(),
            };
            entity_mut(&mut candidate.entities, &selection)
                .context("moved entity")?
                .components
                .insert("core:transform".into(), serde_json::to_value(local)?);
        }
        if !self.transient {
            candidate.validate()?;
        }
        self.checkpoint();
        self.scene = candidate;
        self.selected = selection;
        Ok(())
    }
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        Ok(Self {
            scene: SceneDocument::parse(&std::fs::read(path)?)?,
            path: path.to_path_buf(),
            selected: Vec::new(),
            dirty: false,
            transient: false,
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }
    pub fn checkpoint(&mut self) {
        if self.transient {
            self.dirty = true;
            return;
        }
        self.undo.push(Snapshot {
            scene: self.scene.clone(),
            selected: self.selected.clone(),
        });
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }
    pub fn undo(&mut self) {
        if let Some(previous) = self.undo.pop() {
            self.restore(previous, true);
            self.dirty = true;
        }
    }
    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.restore(next, false);
            self.dirty = true;
        }
    }
    fn restore(&mut self, snapshot: Snapshot, undo: bool) {
        let identity =
            entity_at(&self.scene.entities, &self.selected).and_then(|e| e.script_id.clone());
        let current = Snapshot {
            scene: std::mem::replace(&mut self.scene, snapshot.scene),
            selected: self.selected.clone(),
        };
        self.selected = identity
            .and_then(|id| {
                fn find(es: &[SceneEntity], path: &mut Vec<usize>, id: &str) -> Option<Vec<usize>> {
                    for (i, e) in es.iter().enumerate() {
                        path.push(i);
                        if e.script_id.as_deref() == Some(id) {
                            return Some(path.clone());
                        }
                        if let Some(found) = find(&e.children, path, id) {
                            return Some(found);
                        }
                        path.pop();
                    }
                    None
                }
                find(&self.scene.entities, &mut Vec::new(), &id)
            })
            .unwrap_or_else(|| snapshot.selected.clone());
        if entity_at(&self.scene.entities, &self.selected).is_none() {
            self.selected.clear();
        }
        if undo {
            self.redo.push(current);
        } else {
            self.undo.push(current);
        }
    }
    pub fn copy_selected(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string(
            &serde_json::json!({"whatever_entity": entity_at(&self.scene.entities,&self.selected).context("select an entity")?}),
        )?)
    }
    pub fn paste(&mut self, text: &str) -> anyhow::Result<()> {
        let value: serde_json::Value =
            serde_json::from_str(text).context("clipboard is not an entity")?;
        let mut entity: SceneEntity = serde_json::from_value(
            value
                .get("whatever_entity")
                .context("clipboard is not a Whatever entity")?
                .clone(),
        )?;
        let mut ids = std::collections::HashSet::new();
        fn names(es: &[SceneEntity], ids: &mut std::collections::HashSet<String>) {
            for e in es {
                if let Some(id) = &e.script_id {
                    ids.insert(id.clone());
                }
                names(&e.children, ids);
            }
        }
        names(&self.scene.entities, &mut ids);
        let mut mapping = std::collections::HashMap::new();
        fn rename(
            e: &mut SceneEntity,
            ids: &mut std::collections::HashSet<String>,
            mapping: &mut std::collections::HashMap<String, String>,
        ) {
            if let Some(old) = &e.script_id {
                let mut n = 1;
                let id = loop {
                    let id = format!("{old}_copy_{n}");
                    if ids.insert(id.clone()) {
                        break id;
                    }
                    n += 1;
                };
                mapping.insert(old.clone(), id.clone());
                e.script_id = Some(id);
            }
            for c in &mut e.children {
                rename(c, ids, mapping);
            }
        }
        rename(&mut entity, &mut ids, &mut mapping);
        fn refs(
            value: &mut serde_json::Value,
            mapping: &std::collections::HashMap<String, String>,
        ) {
            match value {
                serde_json::Value::Object(o) => {
                    if let Some(id) = o
                        .get("$entity")
                        .and_then(|v| v.as_str())
                        .and_then(|id| mapping.get(id))
                        .cloned()
                    {
                        o.insert("$entity".into(), id.into());
                    } else {
                        for v in o.values_mut() {
                            refs(v, mapping);
                        }
                    }
                }
                serde_json::Value::Array(a) => {
                    for v in a {
                        refs(v, mapping);
                    }
                }
                _ => {}
            }
        }
        let mut encoded = serde_json::to_value(&entity)?;
        refs(&mut encoded, &mapping);
        entity = serde_json::from_value(encoded)?;
        entity.name = format!("{} Copy", entity.name);
        let parent = self
            .selected
            .split_last()
            .map(|(_, parent)| parent.to_vec())
            .unwrap_or_default();
        let mut candidate = self.scene.clone();
        let siblings = siblings_mut(&mut candidate, &parent).context("paste parent")?;
        let index = siblings.len();
        siblings.push(entity);
        if !self.transient {
            candidate.validate()?;
        }
        self.checkpoint();
        self.scene = candidate;
        self.selected = parent;
        self.selected.push(index);
        Ok(())
    }
    pub fn save(&mut self) -> anyhow::Result<()> {
        ensure!(
            !self.transient,
            "running-game changes cannot be saved as a scene"
        );
        self.scene.validate()?;
        let parent = self.path.parent().context("scene directory")?;
        std::fs::create_dir_all(parent)?;
        let temporary = self.path.with_extension("wscene.saving");
        ensure!(
            !temporary.exists(),
            "another save is in progress: {}",
            temporary.display()
        );
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        use std::io::Write;
        file.write_all(&serde_json::to_vec_pretty(&self.scene)?)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, &self.path).context("replace saved scene")?;
        self.dirty = false;
        Ok(())
    }
    pub fn add(&mut self, child: bool) {
        self.checkpoint();
        let entity = SceneEntity {
            name: "Entity".into(),
            ..Default::default()
        };
        if child && let Some(parent) = entity_mut(&mut self.scene.entities, &self.selected) {
            let index = parent.children.len();
            parent.children.push(entity);
            self.selected.push(index);
        } else {
            self.selected = vec![self.scene.entities.len()];
            self.scene.entities.push(entity);
        }
    }
    pub fn delete(&mut self) {
        let Some((&index, parent)) = self.selected.split_last() else {
            return;
        };
        let parent = parent.to_vec();
        self.checkpoint();
        let entities = if parent.is_empty() {
            Some(&mut self.scene.entities)
        } else {
            entity_mut(&mut self.scene.entities, &parent).map(|entity| &mut entity.children)
        };
        if let Some(entities) = entities
            && index < entities.len()
        {
            entities.remove(index);
        }
        self.selected.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document(value: serde_json::Value) -> Document {
        Document {
            scene: serde_json::from_value(value).expect("scene"),
            path: PathBuf::from("test.wscene"),
            selected: Vec::new(),
            dirty: false,
            transient: false,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }
    #[test]
    fn undo_preserves_the_selected_identity_through_property_edits_and_reparenting() {
        let mut doc = document(
            serde_json::json!({"version":1,"entities":[{"script_id":"parent"},{"script_id":"body","name":"Body"}]}),
        );
        doc.selected = vec![1];
        doc.checkpoint();
        doc.scene.entities[1].enabled = false;
        doc.undo();
        assert_eq!(doc.selected, vec![1]);
        assert!(doc.scene.entities[1].enabled);
        doc.redo();
        assert_eq!(doc.selected, vec![1]);
        assert!(!doc.scene.entities[1].enabled);
        doc.move_entity(&[1], &[0], 0).expect("reparent");
        doc.undo();
        assert_eq!(doc.selected, vec![1]);
        doc.redo();
        assert_eq!(doc.selected, vec![0, 0]);
        doc.delete();
        doc.undo();
        assert_eq!(doc.selected, vec![0, 0]);
    }
    #[test]
    fn pasted_subtrees_get_fresh_ids_and_internal_references_but_keep_external_links() {
        let mut doc = document(
            serde_json::json!({"version":1,"entities":[{"script_id":"outside"},{"script_id":"base","name":"Template","enabled":false,"components":{"example:links":{"child":{"$entity":"child"},"outside":{"$entity":"outside"}}},"children":[{"script_id":"child"}]}]}),
        );
        doc.selected = vec![1];
        let text = doc.copy_selected().expect("copy");
        doc.paste(&text).expect("paste");
        assert_eq!(doc.selected, vec![2]);
        let clone = &doc.scene.entities[2];
        assert!(!clone.enabled);
        assert_eq!(clone.script_id.as_deref(), Some("base_copy_1"));
        assert_eq!(clone.children[0].script_id.as_deref(), Some("child_copy_1"));
        assert_eq!(
            clone.components["example:links"]["child"]["$entity"],
            "child_copy_1"
        );
        assert_eq!(
            clone.components["example:links"]["outside"]["$entity"],
            "outside"
        );
        doc.undo();
        assert_eq!(doc.selected, vec![1]);
        assert_eq!(doc.scene.entities.len(), 2);
        doc.paste(&text).expect("paste again");
        doc.paste(&text).expect("second copy");
        assert_eq!(
            doc.scene.entities[3].script_id.as_deref(),
            Some("base_copy_2")
        );
    }
    #[test]
    fn moving_between_branches_preserves_world_pose_and_undo_restores_the_tree() {
        let transform = |x, scale| serde_json::json!({"core:transform":{"position":[x,0,0],"rotation":[0,0,0,1],"scale":[scale,scale,scale]}});
        let mut doc = document(serde_json::json!({"version":1,"entities":[
            {"script_id":"a","components":transform(10,1),"children":[{"script_id":"moving","components":transform(2,1)}]},
            {"script_id":"b","components":transform(20,2),"children":[{"script_id":"existing"}]}
        ]}));
        let before = world_transform(&doc.scene, &[0, 0]).expect("pose");
        doc.move_entity(&[0, 0], &[1], 1).expect("move");
        assert_eq!(doc.selected, vec![1, 1]);
        let after = world_transform(&doc.scene, &doc.selected).expect("pose");
        for (a, b) in before.position.into_iter().zip(after.position) {
            assert!((a - b).abs() < 1.0e-5);
        }
        assert_eq!(before.scale, after.scale);
        doc.undo();
        assert_eq!(
            doc.scene.entities[0].children[0].script_id.as_deref(),
            Some("moving")
        );
        doc.redo();
        assert_eq!(
            doc.scene.entities[1].children[1].script_id.as_deref(),
            Some("moving")
        );
    }
    #[test]
    fn reordering_adjusts_indices_and_invalid_cycles_leave_the_document_unchanged() {
        let mut doc = document(
            serde_json::json!({"version":1,"entities":[{"script_id":"a"},{"script_id":"b"},{"script_id":"c"}]}),
        );
        doc.move_entity(&[0], &[], 3).expect("reorder");
        assert_eq!(
            doc.scene
                .entities
                .iter()
                .map(|e| e.script_id.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("b"), Some("c"), Some("a")]
        );
        doc.move_entity(&[0], &[2], 0)
            .expect("target index must follow root removal");
        assert_eq!(doc.selected, vec![1, 0]);
        assert_eq!(
            doc.scene.entities[1].children[0].script_id.as_deref(),
            Some("b")
        );
        let saved = serde_json::to_value(&doc.scene).expect("scene");
        assert!(doc.move_entity(&[1], &[1, 0], 0).is_err());
        assert_eq!(serde_json::to_value(&doc.scene).expect("scene"), saved);
    }
    #[test]
    fn invalid_edits_cannot_replace_saved_scene_and_undo_restores_deleted_hierarchy() {
        let root = std::env::temp_dir().join(format!(
            "whatever-editor-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir(&root).expect("fixture");
        let path = root.join("test.wscene");
        let original =
            r#"{"version":1,"entities":[{"script_id":"root","children":[{"script_id":"child"}]}]}"#;
        std::fs::write(&path, original).expect("scene");
        let mut document = Document::open(&path).expect("open");
        document.selected = vec![0];
        document.delete();
        document.undo();
        document.save().expect("save existing file");
        let saved = std::fs::read(&path).expect("saved");
        assert_eq!(
            SceneDocument::parse(&saved).expect("scene").entities[0].children[0]
                .script_id
                .as_deref(),
            Some("child")
        );
        document.checkpoint();
        document.scene.version = 9;
        assert!(document.save().is_err());
        assert_eq!(std::fs::read(&path).expect("unchanged"), saved);
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
