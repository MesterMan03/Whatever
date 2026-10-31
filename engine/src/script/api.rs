use super::RenderCommand;
use super::host::{PendingReply, ScriptHost};
use super::ipc::{EngineMessage, ModManifestDto, QueryResultDto, ScriptMessage};
use crate::audio::manager::LoadOpts;
use crate::audio::{AudioManager, CloseStrategy};
use crate::debug::DebugLogger;
use crate::ecs::{
    COMPONENT_MESH_RENDERER, COMPONENT_SPRITE_RENDERER, COMPONENT_TEXT_RENDERER,
    COMPONENT_TRANSFORM, EntityId, World,
};
use crate::mods::ModRegistry;
use crate::vfs::{Vfs, VfsPath};
use base64::Engine as _;

// --- Return types ------------------------------------------------------------

#[derive(Default)]
pub struct DispatchResult {
    pub render_cmds: Vec<RenderCommand>,
    pub client_commands: Vec<ScriptMessage>,
    pub new_tick_rate: Option<f64>,
    /// `Some(Some(fps))` = set cap; `Some(None)` = remove cap.
    pub new_fps_cap: Option<Option<f64>>,
    pub new_vsync: Option<bool>,
    /// `Some(Some(id))` = set main camera; `Some(None)` = clear (no camera).
    pub set_main_camera: Option<Option<EntityId>>,
}

// --- Dispatcher --------------------------------------------------------------

pub struct EngineContext<'a> {
    pub mode: crate::runtime::RuntimeMode,
    pub network: &'a mut crate::network::NetworkRegistry,
    pub script_host: &'a mut ScriptHost,
    pub registry: &'a ModRegistry,
    pub game_id: &'a str,
    pub debug: &'a mut DebugLogger,
    pub world: &'a mut World,
    pub physics: &'a mut crate::physics::PhysicsRegistry,
    pub vfs: &'a dyn Vfs,
    pub audio: &'a mut AudioManager,
    /// References created earlier in this mod's current batch, never shared across mods.
    pub batch_entities: &'a mut std::collections::HashMap<String, EntityId>,
}

pub fn dispatch(mod_id: &str, msg: ScriptMessage, ctx: EngineContext) -> DispatchResult {
    let EngineContext {
        mode,
        network,
        script_host,
        registry,
        game_id,
        debug,
        world,
        physics,
        vfs,
        audio,
        batch_entities,
    } = ctx;
    // The default world is an ordinary, lazily allocated ECS entity. Its symbolic
    // reference is resolved before any component data is persisted.
    fn uses_default(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(fields) => {
                fields.get("$entity").and_then(|v| v.as_str()) == Some("default:physics_world")
                    || fields.values().any(uses_default)
            }
            serde_json::Value::Array(items) => items.iter().any(uses_default),
            _ => false,
        }
    }
    let default_needed = match &msg {
        ScriptMessage::PhysicsRequest { request, .. } => uses_default(request),
        ScriptMessage::ComponentSet {
            entity_id, data, ..
        } => entity_id == "default:physics_world" || uses_default(data),
        ScriptMessage::ComponentGet { entity_id, .. }
        | ScriptMessage::ComponentRemove { entity_id, .. }
        | ScriptMessage::EntityDestroy { entity_id }
        | ScriptMessage::EntityMove { entity_id, .. }
        | ScriptMessage::EntityGetParent { entity_id, .. }
        | ScriptMessage::EntityGetChildren { entity_id, .. }
        | ScriptMessage::SetMainCamera { entity_id } => entity_id == "default:physics_world",
        ScriptMessage::EntitySetParent {
            entity_id,
            parent_id,
        } => {
            entity_id == "default:physics_world"
                || parent_id.as_deref() == Some("default:physics_world")
        }
        _ => false,
    };
    if default_needed {
        batch_entities.insert(
            "default:physics_world".into(),
            physics.ensure_default(world),
        );
    }
    if matches!(&msg, ScriptMessage::EntitySetParent { .. }) {
        physics.invalidate();
    }
    let adding_tag = matches!(&msg, ScriptMessage::EntityTagAdd { .. });
    match msg {
        ScriptMessage::SceneLoad { request_id, path } => {
            let result = crate::scene::SceneDocument::load(vfs, &path, world);
            let render_cmds = result
                .as_ref()
                .map(|scene| {
                    RenderCommand::upsert_hierarchy(
                        world,
                        scene.entity_ids.iter().filter_map(|id| EntityId::parse(id)),
                    )
                })
                .unwrap_or_default();
            let (scene, error) = match result {
                Ok(scene) => {
                    physics.invalidate();
                    (Some(scene), None)
                }
                Err(error) => (None, Some(format!("{error:#}"))),
            };
            script_host.send(
                mod_id,
                &EngineMessage::SceneLoaded {
                    request_id,
                    scene,
                    error,
                },
                debug,
            );
            return DispatchResult {
                render_cmds,
                ..Default::default()
            };
        }
        ScriptMessage::GetMainCamera { .. } => {}
        ScriptMessage::PhysicsRequest {
            request_id,
            mut request,
        } => {
            let result = crate::ecs::resolve_references(&mut request, &mut |reference| {
                resolve_entity(reference, batch_entities)
            })
            .and_then(|()| physics.request(world, request));
            let (result, error) = match result {
                Ok(value) => (Some(value), None),
                Err(error) => (None, Some(format!("{error:#}"))),
            };
            script_host.send(
                mod_id,
                &EngineMessage::PhysicsResponse {
                    request_id,
                    result,
                    error,
                },
                debug,
            );
        }
        ScriptMessage::NetworkRequest {
            request_id,
            request,
        } => {
            let result = match serde_json::from_value::<crate::network::NetworkRequest>(request) {
                Ok(request) => network.dispatch(mod_id, mode, request, &request_id),
                Err(error) => Some(Err(error.into())),
            };
            let Some(result) = result else {
                return DispatchResult::default();
            };
            script_host.send(
                mod_id,
                &EngineMessage::NetworkResponse {
                    request_id,
                    result: result.as_ref().ok().cloned(),
                    error: result.err().map(|e| e.to_string()),
                },
                debug,
            );
        }
        // --- Logging ---------------------------------------------------------
        ScriptMessage::Log { level, message } => {
            let name = registry
                .get(mod_id)
                .map(|m| m.meta.name.as_str())
                .unwrap_or(mod_id);
            match level.as_str() {
                "error" => tracing::error!("[{name}] {message}"),
                "warn" => tracing::warn!("[{name}] {message}"),
                "info" => tracing::info!("[{name}] {message}"),
                _ => tracing::error!("[{name}] invalid log level '{level}'"),
            }
        }

        // --- Window ----------------------------------------------------------
        command @ (ScriptMessage::SetWindowTitle { .. }
        | ScriptMessage::SetWindowSize { .. }
        | ScriptMessage::SetWindowMode { .. }) => {
            if mode == crate::runtime::RuntimeMode::Server {
                tracing::warn!(mod_id, "Window is unavailable in server mode");
            } else {
                return DispatchResult {
                    client_commands: vec![command],
                    ..Default::default()
                };
            }
        }

        // --- Events ----------------------------------------------------------
        ScriptMessage::Subscribe { events } => {
            if events.iter().any(|event| event == "PhysicsEvent") {
                physics.subscribers.insert(mod_id.to_owned());
            }
            tracing::debug!(mod_id, "subscribed to: {:?}", events);
        }

        // --- Assets (stub) ---------------------------------------------------
        ScriptMessage::AssetRequest { request_id, path } => {
            tracing::debug!(mod_id, "AssetRequest {request_id} path={path}");
        }

        // --- File I/O --------------------------------------------------------
        ScriptMessage::FileWrite {
            request_id,
            path,
            data_base64,
        } => {
            let result = (|| -> anyhow::Result<()> {
                let full_path = resolve_mod_data_path(game_id, mod_id, &path)?;
                let bytes = base64::engine::general_purpose::STANDARD.decode(&data_base64)?;
                if let Some(parent) = full_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&full_path, &bytes)?;
                Ok(())
            })();
            script_host.send(
                mod_id,
                &EngineMessage::FileResponse {
                    request_id,
                    data_base64: None,
                    error: result.err().map(|e| e.to_string()),
                },
                debug,
            );
        }
        ScriptMessage::FileRead { request_id, path } => {
            let result = (|| -> anyhow::Result<String> {
                let full_path = resolve_mod_data_path(game_id, mod_id, &path)?;
                let bytes = std::fs::read(&full_path)?;
                Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
            })();
            script_host.send(
                mod_id,
                &EngineMessage::FileResponse {
                    request_id,
                    data_base64: result.as_ref().ok().cloned(),
                    error: result.err().map(|e| e.to_string()),
                },
                debug,
            );
        }
        ScriptMessage::FileDelete { request_id, path } => {
            let result = (|| -> anyhow::Result<()> {
                let full_path = resolve_mod_data_path(game_id, mod_id, &path)?;
                std::fs::remove_file(&full_path)?;
                Ok(())
            })();
            script_host.send(
                mod_id,
                &EngineMessage::FileResponse {
                    request_id,
                    data_base64: None,
                    error: result.err().map(|e| e.to_string()),
                },
                debug,
            );
        }

        // --- Mod queries -----------------------------------------------------
        ScriptMessage::ModListRequest { request_id } => {
            let mods = registry
                .iter()
                .map(|lm| ModManifestDto::from(&lm.manifest))
                .collect();
            script_host.send(
                mod_id,
                &EngineMessage::ModListResponse { request_id, mods },
                debug,
            );
        }
        ScriptMessage::ModGetRequest {
            request_id,
            mod_id: target_id,
        } => {
            let reply = match registry.get(&target_id) {
                Some(manifest) => EngineMessage::ModGetResponse {
                    request_id,
                    manifest: Some(Box::new(ModManifestDto::from(manifest))),
                    error: None,
                },
                None => EngineMessage::ModGetResponse {
                    request_id,
                    manifest: None,
                    error: Some(format!("mod '{target_id}' not found")),
                },
            };
            script_host.send(mod_id, &reply, debug);
        }

        // --- Inter-mod messaging ---------------------------------------------
        ScriptMessage::ModMessageSend {
            target_mod_id,
            request_id,
            payload,
        } => {
            if !script_host.has_process(&target_mod_id) {
                tracing::warn!(mod_id, "ModMessageSend to unknown mod '{target_mod_id}'");
                return DispatchResult::default();
            }
            let namespaced_id = request_id.as_deref().map(|rid| format!("{mod_id}-{rid}"));
            if let (Some(_), Some(rid)) = (&namespaced_id, &request_id) {
                script_host.add_pending_reply(
                    mod_id,
                    rid,
                    PendingReply {
                        sender_mod_id: mod_id.to_owned(),
                        original_request_id: rid.to_string(),
                    },
                );
            }
            script_host.send(
                &target_mod_id,
                &EngineMessage::ModMessageReceived {
                    source_mod_id: mod_id.to_owned(),
                    request_id: namespaced_id,
                    payload,
                },
                debug,
            );
        }
        ScriptMessage::ModMessageReply {
            request_id,
            payload,
        } => match script_host.take_pending_reply(&request_id) {
            Some(pending) => {
                script_host.send(
                    &pending.sender_mod_id,
                    &EngineMessage::ModMessageReplyDelivered {
                        request_id: pending.original_request_id,
                        payload,
                    },
                    debug,
                );
            }
            None => tracing::warn!(mod_id, "ModMessageReply for unknown key '{request_id}'"),
        },

        // --- Console (intercepted upstream; these arms are safety nets) ------
        ScriptMessage::RegisterCommand { name, .. } => {
            tracing::warn!(
                mod_id,
                "RegisterCommand for '{name}' reached dispatcher unexpectedly"
            );
        }
        ScriptMessage::CommandResponse { request_id, .. } => {
            tracing::warn!(
                mod_id,
                "CommandResponse '{request_id}' reached dispatcher unexpectedly"
            );
        }
        ScriptMessage::ArgSuggestResponse { request_id, .. } => {
            tracing::warn!(
                mod_id,
                "ArgSuggestResponse '{request_id}' reached dispatcher unexpectedly"
            );
        }

        // --- Entity management -----------------------------------------------
        ScriptMessage::EntitySetEnabled { entity_id, enabled } => {
            if let Some(id) = resolve_entity(&entity_id, batch_entities) {
                world.set_enabled(&id, enabled);
                physics.invalidate();
                return DispatchResult {
                    render_cmds: RenderCommand::upsert_hierarchy(world, [id]),
                    ..Default::default()
                };
            }
        }
        ScriptMessage::EntityClone {
            request_id,
            entity_id,
            recursive,
            enabled,
        } => {
            let result = resolve_entity(&entity_id, batch_entities)
                .ok_or_else(|| anyhow::anyhow!("invalid clone source"))
                .and_then(|id| world.clone_entity(id, recursive, enabled));
            match result {
                Ok(id) => {
                    batch_entities.insert(format!("pending:{request_id}"), id);
                    physics.invalidate();
                    script_host.send(
                        mod_id,
                        &EngineMessage::EntityCreated {
                            request_id,
                            entity_id: id.to_string(),
                        },
                        debug,
                    );
                    return DispatchResult {
                        render_cmds: RenderCommand::upsert_hierarchy(world, [id]),
                        ..Default::default()
                    };
                }
                Err(error) => {
                    script_host.send(
                        mod_id,
                        &EngineMessage::EntityCreateFailed {
                            request_id,
                            error: error.to_string(),
                        },
                        debug,
                    );
                }
            }
        }
        ScriptMessage::EntityCreate { request_id } => {
            let id = world.create_entity();
            batch_entities.insert(format!("pending:{request_id}"), id);
            script_host.send(
                mod_id,
                &EngineMessage::EntityCreated {
                    request_id,
                    entity_id: id.to_string(),
                },
                debug,
            );
        }
        ScriptMessage::EntityMove {
            entity_id,
            position,
        } => {
            let Some(id) = resolve_entity(&entity_id, batch_entities) else {
                tracing::warn!(mod_id, "EntityMove: invalid entity_id '{entity_id}'");
                return DispatchResult::default();
            };
            let mut transform = world.transforms.get(&id.index).cloned().unwrap_or_default();
            transform.position = position;
            if let Ok(data) = serde_json::to_value(transform) {
                return dispatch(
                    mod_id,
                    ScriptMessage::ComponentSet {
                        entity_id: id.to_string(),
                        component_type: COMPONENT_TRANSFORM.to_owned(),
                        data,
                    },
                    EngineContext {
                        mode,
                        network,
                        script_host,
                        registry,
                        game_id,
                        debug,
                        world,
                        physics,
                        vfs,
                        audio,
                        batch_entities,
                    },
                );
            }
        }
        ScriptMessage::EntityDestroy { entity_id } => {
            let Some(id) = resolve_entity(&entity_id, batch_entities) else {
                tracing::warn!(mod_id, "EntityDestroy: invalid entity_id '{entity_id}'");
                return DispatchResult::default();
            };
            let render_cmds = RenderCommand::remove_hierarchy(world, [id]);
            physics.entity_destroyed(world, id);
            world.destroy_entity(id);
            if !render_cmds.is_empty() {
                return DispatchResult {
                    render_cmds,
                    ..Default::default()
                };
            }
        }
        ScriptMessage::EntityListRequest { request_id } => {
            let entity_ids = world
                .allocator
                .alive_entity_ids()
                .map(|id| id.to_string())
                .collect();
            script_host.send(
                mod_id,
                &EngineMessage::EntityListResponse {
                    request_id,
                    entity_ids,
                },
                debug,
            );
        }
        ScriptMessage::EntitySetParent {
            entity_id,
            parent_id,
        } => {
            let Some(child) = resolve_entity(&entity_id, batch_entities) else {
                tracing::warn!(mod_id, "EntitySetParent: invalid entity_id '{entity_id}'");
                return DispatchResult::default();
            };
            let parent = match &parent_id {
                None => None,
                Some(pid) => match resolve_entity(pid, batch_entities) {
                    Some(p) => Some(p),
                    None => {
                        tracing::warn!(mod_id, "EntitySetParent: invalid parent_id '{pid}'");
                        return DispatchResult::default();
                    }
                },
            };
            world.set_parent(child, parent);
            let render_cmds = RenderCommand::upsert_hierarchy(world, [child]);
            if !render_cmds.is_empty() {
                return DispatchResult {
                    render_cmds,
                    ..Default::default()
                };
            }
        }
        ScriptMessage::EntityGetParent {
            request_id,
            entity_id,
        } => {
            let parent_id = resolve_entity(&entity_id, batch_entities)
                .and_then(|id| world.get_parent(&id))
                .map(|p| p.to_string());
            script_host.send(
                mod_id,
                &EngineMessage::EntityParentResponse {
                    request_id,
                    entity_id,
                    parent_id,
                },
                debug,
            );
        }
        ScriptMessage::EntityGetChildren {
            request_id,
            entity_id,
        } => {
            let child_ids = resolve_entity(&entity_id, batch_entities)
                .map(|id| {
                    world
                        .get_children(&id)
                        .into_iter()
                        .map(|c| c.to_string())
                        .collect()
                })
                .unwrap_or_default();
            script_host.send(
                mod_id,
                &EngineMessage::EntityChildrenResponse {
                    request_id,
                    entity_id,
                    child_ids,
                },
                debug,
            );
        }

        ScriptMessage::EntityTagAdd { entity_id, tag }
        | ScriptMessage::EntityTagRemove { entity_id, tag } => {
            let result = resolve_entity(&entity_id, batch_entities)
                .ok_or_else(|| anyhow::anyhow!("invalid entity ID"))
                .and_then(|id| {
                    if adding_tag {
                        world.add_tag(&id, tag)
                    } else {
                        world.remove_tag(&id, &tag)
                    }
                });
            if let Err(error) = result {
                tracing::warn!(mod_id, %error, "tag edit rejected");
            }
        }
        ScriptMessage::EntityTagsGet {
            request_id,
            entity_id,
        } => {
            let result = resolve_entity(&entity_id, batch_entities)
                .ok_or_else(|| anyhow::anyhow!("invalid entity ID"))
                .and_then(|id| world.entity_tags(&id));
            let (tags, error) = match result {
                Ok(tags) => (tags, None),
                Err(error) => (vec![], Some(error.to_string())),
            };
            script_host.send(
                mod_id,
                &EngineMessage::EntityTagsResponse {
                    request_id,
                    tags,
                    error,
                },
                debug,
            );
        }
        ScriptMessage::EntityTagQuery { request_id, query } => {
            let entity_ids = match world.find_by_tags(&query) {
                Ok(ids) => ids.into_iter().map(|id| id.to_string()).collect(),
                Err(error) => {
                    tracing::warn!(mod_id, %error, "tag query rejected");
                    vec![]
                }
            };
            script_host.send(
                mod_id,
                &EngineMessage::EntityListResponse {
                    request_id,
                    entity_ids,
                },
                debug,
            );
        }

        // --- Component management --------------------------------------------
        ScriptMessage::ComponentPatch {
            entity_id,
            component_type,
            data,
        } => {
            let Some(id) = resolve_entity(&entity_id, batch_entities) else {
                return DispatchResult::default();
            };
            if !world.component_types.contains_key(&component_type)
                && !matches!(
                    component_type.as_str(),
                    "core:transform"
                        | "core:sprite_renderer"
                        | "core:text_renderer"
                        | "core:mesh_renderer"
                        | "core:camera"
                        | "core:ambient_light"
                        | "core:directional_light"
                        | "core:point_light"
                        | "core:physics_world"
                        | "core:rigid_body"
                        | "core:collider"
                        | "core:joint"
                )
            {
                return DispatchResult::default();
            }
            let Some(mut current) = world.get_component(&id, &component_type) else {
                return DispatchResult::default();
            };
            let Some(patch) = data.as_object() else {
                return DispatchResult::default();
            };
            let Some(object) = current.as_object_mut() else {
                return DispatchResult::default();
            };
            object.extend(patch.clone());
            return dispatch(
                mod_id,
                ScriptMessage::ComponentSet {
                    entity_id,
                    component_type,
                    data: current,
                },
                EngineContext {
                    mode,
                    network,
                    script_host,
                    registry,
                    game_id,
                    debug,
                    world,
                    physics,
                    vfs,
                    audio,
                    batch_entities,
                },
            );
        }
        ScriptMessage::ComponentSet {
            entity_id,
            component_type,
            mut data,
        } => {
            let Some(id) = resolve_entity(&entity_id, batch_entities) else {
                tracing::warn!(mod_id, "ComponentSet: invalid entity_id '{entity_id}'");
                return DispatchResult::default();
            };
            if !world.is_alive(&id) {
                return DispatchResult::default();
            }
            let data = crate::ecs::resolve_references(&mut data, &mut |reference| {
                resolve_entity(reference, batch_entities)
            })
            .and_then(|()| physics.normalize_component(world, id, &component_type, data));
            let data = match data {
                Ok(data) => data,
                Err(error) => {
                    tracing::warn!(mod_id,%component_type,%error,"ComponentSet rejected");
                    return DispatchResult::default();
                }
            };
            physics.component_changed(world, id, &component_type);
            if !world.set_component(&id, &component_type, data) {
                return DispatchResult::default();
            }
            if component_type == COMPONENT_TRANSFORM {
                return DispatchResult {
                    render_cmds: RenderCommand::upsert_hierarchy(world, [id]),
                    ..Default::default()
                };
            }
            let idx = id.index;
            let has_transform = world.transforms.contains_key(&idx);
            // Emit UpsertSprite only once both sprite renderer components are present.
            if component_type == COMPONENT_SPRITE_RENDERER
                && has_transform
                && world.sprite_renderers.contains_key(&idx)
            {
                return DispatchResult {
                    render_cmds: vec![RenderCommand::UpsertSprite { entity_idx: idx }],
                    ..Default::default()
                };
            }
            // Emit UpsertText only once both text renderer components are present.
            if component_type == COMPONENT_TEXT_RENDERER
                && has_transform
                && world.text_renderers.contains_key(&idx)
            {
                return DispatchResult {
                    render_cmds: vec![RenderCommand::UpsertText { entity_idx: idx }],
                    ..Default::default()
                };
            }
            // Emit UpsertMesh only once both mesh renderer components are present.
            if component_type == COMPONENT_MESH_RENDERER
                && has_transform
                && world.mesh_renderers.contains_key(&idx)
            {
                return DispatchResult {
                    render_cmds: vec![RenderCommand::UpsertMesh { entity_idx: idx }],
                    ..Default::default()
                };
            }
        }
        ScriptMessage::ComponentRemove {
            entity_id,
            component_type,
        } => {
            if crate::physics::PhysicsRegistry::is_read_only(&component_type) {
                tracing::warn!(mod_id,%component_type,"native physics state is read-only");
                return DispatchResult::default();
            }
            let Some(id) = resolve_entity(&entity_id, batch_entities) else {
                tracing::warn!(mod_id, "ComponentRemove: invalid entity_id '{entity_id}'");
                return DispatchResult::default();
            };
            let idx = id.index;
            physics.component_changed(world, id, &component_type);
            let mut removed_transform = if component_type == COMPONENT_TRANSFORM {
                RenderCommand::remove_entity(world, id)
            } else {
                Vec::new()
            };
            world.remove_component(&id, &component_type);
            if component_type == COMPONENT_TRANSFORM {
                removed_transform.extend(RenderCommand::upsert_hierarchy(
                    world,
                    world.get_children(&id),
                ));
                return DispatchResult {
                    render_cmds: removed_transform,
                    ..Default::default()
                };
            }
            if component_type == COMPONENT_SPRITE_RENDERER {
                return DispatchResult {
                    render_cmds: vec![RenderCommand::RemoveSprite { entity_idx: idx }],
                    ..Default::default()
                };
            }
            if component_type == COMPONENT_TEXT_RENDERER {
                return DispatchResult {
                    render_cmds: vec![RenderCommand::RemoveText { entity_idx: idx }],
                    ..Default::default()
                };
            }
            if component_type == COMPONENT_MESH_RENDERER {
                return DispatchResult {
                    render_cmds: vec![RenderCommand::RemoveMesh { entity_idx: idx }],
                    ..Default::default()
                };
            }
        }
        ScriptMessage::ComponentGet {
            request_id,
            entity_id,
            component_type,
        } => {
            if let Err(error) = physics.synchronize(world) {
                tracing::warn!(%error,"physics synchronization failed");
            }
            let reply = match resolve_entity(&entity_id, batch_entities) {
                None => EngineMessage::ComponentGetResponse {
                    request_id,
                    entity_id,
                    component_type,
                    data: None,
                    error: Some("invalid entity_id".to_owned()),
                },
                Some(id) => EngineMessage::ComponentGetResponse {
                    request_id,
                    data: world.get_component(&id, &component_type),
                    entity_id,
                    component_type,
                    error: None,
                },
            };
            script_host.send(mod_id, &reply, debug);
        }
        ScriptMessage::ComponentQuery {
            request_id,
            component_types,
        } => {
            if let Err(error) = physics.synchronize(world) {
                tracing::warn!(%error,"physics synchronization failed");
            }
            let type_refs: Vec<&str> = component_types.iter().map(String::as_str).collect();
            let results = world
                .query(&type_refs)
                .into_iter()
                .map(|(id, components)| QueryResultDto {
                    entity_id: id.to_string(),
                    components,
                })
                .collect();
            script_host.send(
                mod_id,
                &EngineMessage::ComponentQueryResponse {
                    request_id,
                    results,
                },
                debug,
            );
        }

        // --- Audio -----------------------------------------------------------
        ScriptMessage::AudioLoad {
            request_id,
            audio_id,
            path,
            play,
            volume,
            speed,
            loop_,
            close_strategy,
        } => {
            debug.audio(
                mod_id,
                &format!("AudioLoad {audio_id} path={path} play={play}"),
            );
            let reply = match VfsPath::parse(&path) {
                None => EngineMessage::AudioLoaded {
                    request_id,
                    audio_id,
                    duration_ms: None,
                    sample_rate: 0,
                    channels: 0,
                    error: Some(format!("invalid vfs path: {path}")),
                },
                Some(vfs_path) => match vfs.read(&vfs_path) {
                    Err(e) => EngineMessage::AudioLoaded {
                        request_id,
                        audio_id,
                        duration_ms: None,
                        sample_rate: 0,
                        channels: 0,
                        error: Some(format!("vfs read error: {e}")),
                    },
                    Ok(data) => {
                        let strategy = if close_strategy == "Manual" {
                            CloseStrategy::Manual
                        } else {
                            CloseStrategy::Auto
                        };
                        let opts = LoadOpts {
                            play,
                            volume,
                            speed,
                            loop_,
                            close_strategy: strategy,
                        };
                        match audio.load(audio_id.clone(), mod_id.to_owned(), data, opts) {
                            Ok(meta) => EngineMessage::AudioLoaded {
                                request_id,
                                audio_id,
                                duration_ms: meta.duration_ms,
                                sample_rate: meta.sample_rate,
                                channels: meta.channels,
                                error: None,
                            },
                            Err(e) => EngineMessage::AudioLoaded {
                                request_id,
                                audio_id,
                                duration_ms: None,
                                sample_rate: 0,
                                channels: 0,
                                error: Some(e.to_string()),
                            },
                        }
                    }
                },
            };
            script_host.send(mod_id, &reply, debug);
        }

        ScriptMessage::AudioPlay {
            request_id,
            audio_id,
            volume,
            speed,
        } => {
            debug.audio(mod_id, &format!("AudioPlay {audio_id}"));
            let reply = match audio.play(&audio_id, volume, speed) {
                Ok(pos) => build_audio_state(request_id, audio_id.clone(), pos, audio),
                Err(e) => audio_state_error(request_id, audio_id, e.to_string()),
            };
            script_host.send(mod_id, &reply, debug);
        }

        ScriptMessage::AudioPause {
            request_id,
            audio_id,
        } => {
            debug.audio(mod_id, &format!("AudioPause {audio_id}"));
            let reply = match audio.pause(&audio_id) {
                Ok(pos) => build_audio_state(request_id, audio_id.clone(), pos, audio),
                Err(e) => audio_state_error(request_id, audio_id, e.to_string()),
            };
            script_host.send(mod_id, &reply, debug);
        }

        ScriptMessage::AudioStop { audio_id } => {
            debug.audio(mod_id, &format!("AudioStop {audio_id}"));
            audio.stop(&audio_id);
            script_host.send(mod_id, &EngineMessage::AudioClose { audio_id }, debug);
        }

        ScriptMessage::AudioSeekTo {
            request_id,
            audio_id,
            position_ms,
        } => {
            debug.audio(
                mod_id,
                &format!("AudioSeekTo {audio_id} pos={position_ms}ms"),
            );
            let reply = match audio.seek_to(&audio_id, position_ms) {
                Ok(prev) => build_audio_state(request_id, audio_id.clone(), prev, audio),
                Err(e) => audio_state_error(request_id, audio_id, e.to_string()),
            };
            script_host.send(mod_id, &reply, debug);
        }

        ScriptMessage::AudioSeek {
            request_id,
            audio_id,
            offset_ms,
        } => {
            debug.audio(
                mod_id,
                &format!("AudioSeek {audio_id} offset={offset_ms}ms"),
            );
            let reply = match audio.seek(&audio_id, offset_ms) {
                Ok(new_pos) => build_audio_state(request_id, audio_id.clone(), new_pos, audio),
                Err(e) => audio_state_error(request_id, audio_id, e.to_string()),
            };
            script_host.send(mod_id, &reply, debug);
        }

        ScriptMessage::AudioSetLoop { audio_id, loop_ } => {
            debug.audio(mod_id, &format!("AudioSetLoop {audio_id} loop={loop_}"));
            if let Err(e) = audio.set_loop(&audio_id, loop_) {
                tracing::warn!(mod_id, "AudioSetLoop {audio_id}: {e}");
            }
        }

        ScriptMessage::AudioQuery {
            request_id,
            audio_id,
        } => {
            debug.audio(mod_id, &format!("AudioQuery {audio_id}"));
            let reply = match audio.query(&audio_id) {
                Ok(state) => EngineMessage::AudioState {
                    request_id,
                    audio_id,
                    position_ms: state.position_ms,
                    volume: state.volume,
                    speed: state.speed,
                    is_playing: state.is_playing,
                    is_looping: state.is_looping,
                    error: None,
                },
                Err(e) => audio_state_error(request_id, audio_id, e.to_string()),
            };
            script_host.send(mod_id, &reply, debug);
        }

        // --- Tick rate -------------------------------------------------------
        ScriptMessage::SetTickRate { ticks_per_second } => {
            return DispatchResult {
                new_tick_rate: Some(ticks_per_second),
                ..Default::default()
            };
        }

        // --- Frame rate / vsync ----------------------------------------------
        ScriptMessage::SetFpsCap { fps } => {
            return DispatchResult {
                new_fps_cap: Some(fps),
                ..Default::default()
            };
        }
        ScriptMessage::SetVsync { enabled } => {
            return DispatchResult {
                new_vsync: Some(enabled),
                ..Default::default()
            };
        }

        // --- Camera ----------------------------------------------------------
        ScriptMessage::SetMainCamera { entity_id } => {
            let camera_entity = if entity_id.is_empty() {
                None
            } else {
                match resolve_entity(&entity_id, batch_entities) {
                    Some(id) => Some(id),
                    None => {
                        tracing::warn!(mod_id, "SetMainCamera: invalid entity_id '{entity_id}'");
                        return DispatchResult::default();
                    }
                }
            };
            return DispatchResult {
                set_main_camera: Some(camera_entity),
                ..Default::default()
            };
        }
    }

    DispatchResult::default()
}

fn resolve_entity(
    reference: &str,
    batch_entities: &std::collections::HashMap<String, EntityId>,
) -> Option<EntityId> {
    batch_entities
        .get(reference)
        .copied()
        .or_else(|| EntityId::parse(reference))
}

fn build_audio_state(
    request_id: String,
    audio_id: String,
    position_ms: u64,
    audio: &AudioManager,
) -> EngineMessage {
    let state = audio.query(&audio_id).ok();
    EngineMessage::AudioState {
        request_id,
        audio_id,
        position_ms,
        volume: state.as_ref().map_or(1.0, |s| s.volume),
        speed: state.as_ref().map_or(1.0, |s| s.speed),
        is_playing: state.as_ref().is_some_and(|s| s.is_playing),
        is_looping: state.as_ref().is_some_and(|s| s.is_looping),
        error: None,
    }
}

fn audio_state_error(request_id: String, audio_id: String, error: String) -> EngineMessage {
    EngineMessage::AudioState {
        request_id,
        audio_id,
        position_ms: 0,
        volume: 0.0,
        speed: 0.0,
        is_playing: false,
        is_looping: false,
        error: Some(error),
    }
}

/// Returns the root of a mod's persistent data directory.
pub fn mod_data_root(game_id: &str, mod_id: &str) -> anyhow::Result<std::path::PathBuf> {
    if !crate::sandbox::valid_storage_id(game_id) || !crate::sandbox::valid_storage_id(mod_id) {
        anyhow::bail!(
            "game and mod IDs must contain only lowercase letters, digits, underscores or hyphens"
        );
    }
    let base = dirs::data_local_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine local data directory"))?;
    Ok(base.join("Whatever").join(game_id).join(mod_id))
}

fn resolve_mod_data_path(
    game_id: &str,
    mod_id: &str,
    path: &str,
) -> anyhow::Result<std::path::PathBuf> {
    crate::sandbox::resolve_contained(&mod_data_root(game_id, mod_id)?, path)
}
