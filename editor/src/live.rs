use super::*;
use engine::{
    ecs::EntityId,
    runtime::{EditorBatch, EditorServer, EntityEdit, ParentEdit, SiblingOrder, ValueEdit},
};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum View {
    Scene,
    Play,
}
pub(super) struct LiveSession {
    pub saved: Option<Document>,
    pub link: EditorServer,
    undo: Vec<(EditorBatch, EditorBatch)>,
    redo: Vec<(EditorBatch, EditorBatch)>,
    selection: Option<String>,
    counter: u64,
    pub camera: Option<EntityId>,
    grouping: bool,
    awaiting_creation: Option<u64>,
}
impl LiveSession {
    pub fn new(link: EditorServer, saved: Option<Document>) -> Self {
        Self {
            saved,
            link,
            undo: vec![],
            redo: vec![],
            selection: None,
            counter: 0,
            camera: None,
            grouping: false,
            awaiting_creation: None,
        }
    }
}
fn paths(scene: &SceneDocument) -> BTreeMap<String, (Vec<usize>, Option<String>)> {
    fn walk(
        items: &[SceneEntity],
        path: &mut Vec<usize>,
        parent: Option<String>,
        out: &mut BTreeMap<String, (Vec<usize>, Option<String>)>,
    ) {
        for (i, e) in items.iter().enumerate() {
            path.push(i);
            if let Some(id) = &e.script_id {
                out.insert(id.clone(), (path.clone(), parent.clone()));
            }
            walk(&e.children, path, e.script_id.clone(), out);
            path.pop();
        }
    }
    let mut out = BTreeMap::new();
    walk(&scene.entities, &mut vec![], None, &mut out);
    out
}
fn value_diff(before: &Value, after: &Value, path: &mut Vec<String>, out: &mut Vec<ValueEdit>) {
    if before == after {
        return;
    }
    match (before, after) {
        (Value::Object(a), Value::Object(b)) => {
            for key in a
                .keys()
                .chain(b.keys())
                .collect::<std::collections::BTreeSet<_>>()
            {
                path.push(key.clone());
                match (a.get(key), b.get(key)) {
                    (Some(a), Some(b)) => value_diff(a, b, path, out),
                    (_, value) => out.push(ValueEdit {
                        path: path.clone(),
                        value: value.cloned(),
                    }),
                };
                path.pop();
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                path.push(i.to_string());
                value_diff(a, b, path, out);
                path.pop();
            }
        }
        _ => out.push(ValueEdit {
            path: path.clone(),
            value: Some(after.clone()),
        }),
    }
}
fn difference(before: &SceneDocument, after: &SceneDocument) -> EditorBatch {
    let a = paths(before);
    let b = paths(after);
    let mut result = EditorBatch::default();
    for (id, (path, parent)) in &b {
        let Some(node) = entity_at(&after.entities, path) else {
            continue;
        };
        let Some((previous, old_parent)) = a.get(id) else {
            if parent.as_ref().is_none_or(|p| a.contains_key(p)) {
                result.create.push((parent.clone(), node.clone()));
            }
            continue;
        };
        let Some(old) = entity_at(&before.entities, previous) else {
            continue;
        };
        if parent != old_parent {
            result.parents.push(ParentEdit {
                id: id.clone(),
                parent: parent.clone(),
            });
        }
        let mut components = BTreeMap::new();
        for kind in old
            .components
            .keys()
            .chain(node.components.keys())
            .collect::<std::collections::BTreeSet<_>>()
        {
            if engine::runtime::read_only(kind) {
                continue;
            }
            let mut edits = vec![];
            match (old.components.get(kind), node.components.get(kind)) {
                (Some(old), Some(new)) => value_diff(old, new, &mut vec![], &mut edits),
                (_, value) => edits.push(ValueEdit {
                    path: vec![],
                    value: value.cloned(),
                }),
            };
            if !edits.is_empty() {
                components.insert(kind.clone(), edits);
            }
        }
        let edit = EntityEdit {
            id: id.clone(),
            name: (old.name != node.name).then(|| node.name.clone()),
            enabled: (old.enabled != node.enabled).then_some(node.enabled),
            components,
        };
        if edit.name.is_some() || edit.enabled.is_some() || !edit.components.is_empty() {
            result.edits.push(edit);
        }
    }
    for (id, (_, parent)) in &a {
        if !b.contains_key(id) && parent.as_ref().is_none_or(|p| b.contains_key(p)) {
            result.destroy.push(id.clone());
        }
    }
    fn orders(
        items: &[SceneEntity],
        parent: Option<String>,
        out: &mut BTreeMap<Option<String>, Vec<String>>,
    ) {
        out.insert(
            parent,
            items.iter().filter_map(|e| e.script_id.clone()).collect(),
        );
        for e in items {
            orders(&e.children, e.script_id.clone(), out);
        }
    }
    let mut old = BTreeMap::new();
    let mut new = BTreeMap::new();
    orders(&before.entities, None, &mut old);
    orders(&after.entities, None, &mut new);
    for (parent, children) in new {
        if old.get(&parent) != Some(&children) {
            result.orders.push(SiblingOrder { parent, children });
        }
    }
    result
}
fn rebase(batch: &mut EditorBatch, mapping: &HashMap<String, String>) {
    fn id(value: &mut String, map: &HashMap<String, String>) {
        if let Some(next) = map.get(value) {
            *value = next.clone();
        }
    }
    fn refs(value: &mut Value, map: &HashMap<String, String>) {
        match value {
            Value::Object(o) => {
                if let Some(Value::String(value)) = o.get_mut("$entity") {
                    id(value, map);
                } else {
                    for value in o.values_mut() {
                        refs(value, map);
                    }
                }
            }
            Value::Array(a) => {
                for value in a {
                    refs(value, map)
                }
            }
            _ => {}
        }
    }
    fn node(e: &mut SceneEntity, map: &HashMap<String, String>) {
        if let Some(value) = &mut e.script_id {
            id(value, map);
        }
        for v in e.components.values_mut() {
            refs(v, map);
        }
        for e in &mut e.children {
            node(e, map);
        }
    }
    for edit in &mut batch.edits {
        id(&mut edit.id, mapping);
        for edits in edit.components.values_mut() {
            for edit in edits {
                if let Some(value) = &mut edit.value {
                    refs(value, mapping);
                }
            }
        }
    }
    for p in &mut batch.parents {
        id(&mut p.id, mapping);
        if let Some(p) = &mut p.parent {
            id(p, mapping);
        }
    }
    for order in &mut batch.orders {
        if let Some(parent) = &mut order.parent {
            id(parent, mapping);
        }
        for child in &mut order.children {
            id(child, mapping);
        }
    }
    for i in &mut batch.destroy {
        id(i, mapping);
    }
    for (parent, e) in &mut batch.create {
        if let Some(parent) = parent {
            id(parent, mapping);
        }
        node(e, mapping);
    }
}
fn merge_edits(target: &mut EditorBatch, source: &EditorBatch, replace: bool) {
    for edit in &source.edits {
        if let Some(old) = target.edits.iter_mut().find(|e| e.id == edit.id) {
            if (replace || old.name.is_none())
                && let Some(name) = &edit.name
            {
                old.name = Some(name.clone());
            }
            if (replace || old.enabled.is_none())
                && let Some(enabled) = edit.enabled
            {
                old.enabled = Some(enabled);
            }
            for (kind, changes) in &edit.components {
                let current = old.components.entry(kind.clone()).or_default();
                for change in changes {
                    if let Some(existing) = current.iter_mut().find(|e| e.path == change.path) {
                        if replace {
                            *existing = change.clone();
                        }
                    } else {
                        current.push(change.clone());
                    }
                }
            }
        } else {
            target.edits.push(edit.clone());
        }
    }
}
impl Editor {
    pub(super) fn poll_live(&mut self) {
        let Some(session) = &mut self.live else {
            return;
        };
        let (snapshot, replies, error) = session.link.take();
        if let Some(error) = error {
            self.status = error;
        }
        for reply in replies {
            if reply.error.is_some() && session.awaiting_creation == Some(reply.sequence) {
                session.awaiting_creation = None;
            }
            if let Some(error) = reply.error {
                self.status = format!("Live edit rejected: {error}");
                self.inspector_error = Some(error);
            }
            if !reply.created.is_empty() {
                if let Some(doc) = &mut self.document {
                    let mut wrapper = EditorBatch::default();
                    wrapper.create.push((
                        None,
                        SceneEntity {
                            children: std::mem::take(&mut doc.scene.entities),
                            ..Default::default()
                        },
                    ));
                    rebase(&mut wrapper, &reply.created);
                    doc.scene.entities = wrapper.create.remove(0).1.children;
                }
                if let Some(entity) = &mut self.inspector_entity {
                    let mut wrapper = EditorBatch::default();
                    wrapper.create.push((None, entity.clone()));
                    rebase(&mut wrapper, &reply.created);
                    *entity = wrapper.create.remove(0).1;
                }

                for pair in session.undo.iter_mut().chain(session.redo.iter_mut()) {
                    rebase(&mut pair.0, &reply.created);
                    rebase(&mut pair.1, &reply.created);
                }
                if let Some(id) = session
                    .selection
                    .as_ref()
                    .and_then(|id| reply.created.get(id))
                {
                    session.selection = Some(id.clone());
                }
            }
        }
        let inspecting = self.view == View::Scene && !self.player_detached || self.has_dialog();
        if let Some(session) = &self.live {
            session.link.inspect(inspecting);
        }
        let Some(snapshot) = snapshot else {
            return;
        };
        if let Some(session) = &mut self.live {
            session.camera = snapshot.camera;
        }
        let world = snapshot.world;
        if self
            .live
            .as_ref()
            .and_then(|s| s.awaiting_creation)
            .is_some_and(|sequence| snapshot.sequence < sequence)
        {
            self.live_world(world);
            self.preview_dirty = false;
            return;
        }
        if let Some(session) = &mut self.live {
            session.awaiting_creation = None;
        }
        let selected = self
            .document
            .as_ref()
            .and_then(|d| entity_at(&d.scene.entities, &d.selected))
            .and_then(|e| e.script_id.clone());
        if let Some(id) = selected
            && let Some(session) = &mut self.live
        {
            session.selection = Some(id);
        }
        let mut scene = engine::runtime::editor_scene(&world);
        let index = paths(&scene);
        let selection = self
            .live
            .as_ref()
            .and_then(|s| s.selection.as_ref())
            .and_then(|id| index.get(id))
            .map(|p| p.0.clone())
            .unwrap_or_default();
        let holding =
            self.ctx.wants_keyboard_input() || self.gizmo_drag.is_some() || self.inspector_dirty;
        if holding
            && let Some(held) = &mut self.inspector_entity
            && let Some(current) = entity_mut(&mut scene.entities, &selection)
        {
            held.children = current.children.clone();
            let children = current.children.clone();
            *current = held.clone();
            current.children = children;
        }
        if let Some(doc) = &mut self.document {
            doc.scene = scene;
            doc.selected = selection;
            doc.dirty = false;
        }
        if !holding {
            self.sync_selection();
        }
        self.live_world(world);
        self.preview_dirty = false;
    }
    fn live_world(&mut self, world: World) {
        let mut commands = HashMap::new();
        let kinds = [
            "core:transform",
            "core:sprite_renderer",
            "core:text_renderer",
            "core:mesh_renderer",
        ];
        for id in self.world.allocator.alive_entity_ids() {
            if !world.is_alive(&id)
                || kinds.iter().any(|kind| {
                    self.world.get_component(&id, kind) != world.get_component(&id, kind)
                })
            {
                for cmd in engine::script::RenderCommand::remove_entity(&self.world, id) {
                    commands.insert(cmd.key(), cmd);
                }
            }
        }
        for id in world.allocator.alive_entity_ids() {
            let before = self
                .world
                .world_transform(&id)
                .and_then(|p| serde_json::to_value(p).ok());
            let after = world
                .world_transform(&id)
                .and_then(|p| serde_json::to_value(p).ok());
            if !self.world.is_alive(&id)
                || before != after
                || self.world.is_active(&id) != world.is_active(&id)
                || kinds.iter().any(|kind| {
                    self.world.get_component(&id, kind) != world.get_component(&id, kind)
                })
            {
                for cmd in engine::script::RenderCommand::upsert_hierarchy(&world, [id]) {
                    commands.insert(cmd.key(), cmd);
                }
            }
        }
        if let Some(renderer) = &mut self.renderer {
            for cmd in commands.into_values() {
                engine::renderer::apply_render_command(&cmd, renderer, &world, &self.vfs);
            }
        }
        self.world = world;
    }
    pub(super) fn live_changes(&mut self, before: &SceneDocument) -> anyhow::Result<()> {
        let Some(session) = &mut self.live else {
            return Ok(());
        };
        if session.awaiting_creation.is_some() {
            return Ok(());
        }
        let Some(doc) = &mut self.document else {
            return Ok(());
        };
        fn ids(items: &mut [SceneEntity], counter: &mut u64) {
            for e in items {
                if e.script_id.is_none() {
                    *counter += 1;
                    e.script_id = Some(format!("editor_new_{counter}"));
                }
                ids(&mut e.children, counter);
            }
        }
        ids(&mut doc.scene.entities, &mut session.counter);
        let batch = difference(before, &doc.scene);
        let dragging = self.gizmo_drag.is_some() || self.ctx.input(|i| i.pointer.primary_down());
        if batch.is_empty() {
            session.grouping &= dragging;
            return Ok(());
        }
        session.selection =
            entity_at(&doc.scene.entities, &doc.selected).and_then(|e| e.script_id.clone());
        let inverse = difference(&doc.scene, before);
        let sequence = session.link.edit(batch.clone())?;
        if !batch.create.is_empty() {
            session.awaiting_creation = Some(sequence);
        }
        if session.grouping
            && batch.parents.is_empty()
            && batch.orders.is_empty()
            && batch.create.is_empty()
            && batch.destroy.is_empty()
            && let Some((forward, backward)) = session.undo.last_mut()
        {
            merge_edits(forward, &batch, true);
            merge_edits(backward, &inverse, false);
        } else {
            session.undo.push((batch, inverse));
        }
        session.grouping = dragging;
        if session.undo.len() > 100 {
            session.undo.remove(0);
        }
        session.redo.clear();
        doc.dirty = false;
        self.preview_dirty = false;
        Ok(())
    }
    pub(super) fn live_waiting(&self) -> bool {
        self.live
            .as_ref()
            .is_some_and(|s| s.awaiting_creation.is_some())
    }
    pub(super) fn live_undo(&mut self, redo: bool) -> anyhow::Result<()> {
        let session = self.live.as_mut().context("Play session")?;
        let source = if redo {
            &mut session.redo
        } else {
            &mut session.undo
        };
        if let Some(pair) = source.pop() {
            session
                .link
                .edit(if redo { pair.0.clone() } else { pair.1.clone() })?;
            if redo {
                session.undo.push(pair);
            } else {
                session.redo.push(pair);
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn runtime_diffs_only_change_edited_leaves_and_keep_stable_targets() {
        let before = SceneDocument {
            version: 1,
            entities: vec![SceneEntity {
                script_id: Some("5:2".into()),
                components: BTreeMap::from([(
                    "core:transform".into(),
                    json!({"position":[1,2,3]}),
                )]),
                ..Default::default()
            }],
        };
        let mut after = before.clone();
        after.entities[0]
            .components
            .get_mut("core:transform")
            .expect("transform")["position"][1] = json!(9);
        let edits = difference(&before, &after);
        assert_eq!(edits.edits[0].id, "5:2");
        let change = &edits.edits[0].components["core:transform"];
        assert_eq!(change.len(), 1);
        assert_eq!(change[0].path, ["position", "1"]);
        assert!(difference(&before, &before).is_empty());
    }
    #[test]
    fn stop_restores_the_authored_document_and_live_edits_cannot_be_saved() {
        let path = std::env::temp_dir().join(format!(
            "whatever-live-save-{}-{}.wscene",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let source = r#"{"version":1,"entities":[{"script_id":"authored","name":"Original"}]}"#;
        std::fs::write(&path, source).expect("fixture");
        let authored = Document::open(&path).expect("open");
        let mut editor = Editor::new(None, None);
        editor.live = Some(LiveSession::new(
            EditorServer::listen().expect("listen"),
            Some(authored),
        ));
        editor.document = Some(Document::live(SceneDocument {
            version: 1,
            entities: vec![SceneEntity {
                name: "Runtime edit".into(),
                ..Default::default()
            }],
        }));
        editor.document.as_mut().expect("doc").dirty = true;
        assert!(!editor.unsaved());
        assert!(editor.save_scene().is_err());
        assert!(editor.document.as_mut().expect("doc").save().is_err());
        assert_eq!(std::fs::read_to_string(&path).expect("source"), source);
        editor.stop();
        assert_eq!(
            editor.document.as_ref().expect("restored").scene.entities[0].name,
            "Original"
        );
        assert!(!editor.document.as_ref().expect("doc").transient);
        std::fs::remove_file(path).expect("cleanup");
    }
}
