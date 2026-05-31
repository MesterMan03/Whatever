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
use std::sync::Arc;
use winit::window::Window;

// --- Return types ------------------------------------------------------------

#[derive(Default)]
pub struct DispatchResult {
    pub render_cmds: Vec<RenderCommand>,
    pub new_tick_rate: Option<f64>,
    /// `Some(Some(fps))` = set cap; `Some(None)` = remove cap.
    pub new_fps_cap: Option<Option<f64>>,
    pub new_vsync: Option<bool>,
    /// `Some(Some(id))` = set main camera; `Some(None)` = clear (no camera).
    pub set_main_camera: Option<Option<EntityId>>,
}

pub enum RenderCommand {
    UpsertSprite { entity_idx: u32 },
    RemoveSprite { entity_idx: u32 },
    UpsertText { entity_idx: u32 },
    RemoveText { entity_idx: u32 },
    UpsertMesh { entity_idx: u32 },
    RemoveMesh { entity_idx: u32 },
}

// --- Dispatcher --------------------------------------------------------------

pub struct EngineContext<'a> {
    pub window: &'a Arc<Window>,
    pub script_host: &'a mut ScriptHost,
    pub registry: &'a ModRegistry,
    pub game_id: &'a str,
    pub debug: &'a mut DebugLogger,
    pub world: &'a mut World,
    pub vfs: &'a dyn Vfs,
    pub audio: &'a mut AudioManager,
}

pub fn dispatch(mod_id: &str, msg: ScriptMessage, ctx: EngineContext) -> DispatchResult {
    let EngineContext {
        window,
        script_host,
        registry,
        game_id,
        debug,
        world,
        vfs,
        audio,
    } = ctx;
    match msg {
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
        ScriptMessage::SetWindowTitle { title } => {
            debug.window(&format!("[{mod_id}] SetWindowTitle: {title}"));
            window.set_title(&title);
        }
        ScriptMessage::SetWindowSize { width, height } => {
            debug.window(&format!("[{mod_id}] SetWindowSize: {width}x{height}"));
            let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(width, height));
        }
        ScriptMessage::SetWindowMode { mode } => {
            debug.window(&format!("[{mod_id}] SetWindowMode: {mode}"));
            let fullscreen = match mode.as_str() {
                "windowed" => None,
                "borderless" => Some(winit::window::Fullscreen::Borderless(None)),
                "fullscreen" => window
                    .current_monitor()
                    .and_then(|m| m.video_modes().next())
                    .map(winit::window::Fullscreen::Exclusive)
                    .or(Some(winit::window::Fullscreen::Borderless(None))),
                _ => {
                    tracing::warn!(mod_id, "SetWindowMode: unknown mode '{mode}'");
                    return DispatchResult::default();
                }
            };
            window.set_fullscreen(fullscreen);
        }

        // --- Events ----------------------------------------------------------
        ScriptMessage::Subscribe { events } => {
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
        ScriptMessage::EntityCreate { request_id } => {
            let id = world.create_entity();
            script_host.send(
                mod_id,
                &EngineMessage::EntityCreated {
                    request_id,
                    entity_id: id.to_string(),
                },
                debug,
            );
        }
        ScriptMessage::EntityDestroy { entity_id } => {
            let Some(id) = EntityId::parse(&entity_id) else {
                tracing::warn!(mod_id, "EntityDestroy: invalid entity_id '{entity_id}'");
                return DispatchResult::default();
            };
            let idx = id.index;
            let had_sprite = world.is_alive(&id) && world.sprite_renderers.contains_key(&idx);
            let had_text = world.is_alive(&id) && world.text_renderers.contains_key(&idx);
            let had_mesh = world.is_alive(&id) && world.mesh_renderers.contains_key(&idx);
            world.destroy_entity(id);
            let mut render_cmds = Vec::new();
            if had_sprite {
                render_cmds.push(RenderCommand::RemoveSprite { entity_idx: idx });
            }
            if had_text {
                render_cmds.push(RenderCommand::RemoveText { entity_idx: idx });
            }
            if had_mesh {
                render_cmds.push(RenderCommand::RemoveMesh { entity_idx: idx });
            }
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
            let Some(child) = EntityId::parse(&entity_id) else {
                tracing::warn!(mod_id, "EntitySetParent: invalid entity_id '{entity_id}'");
                return DispatchResult::default();
            };
            let parent = match &parent_id {
                None => None,
                Some(pid) => match EntityId::parse(pid) {
                    Some(p) => Some(p),
                    None => {
                        tracing::warn!(mod_id, "EntitySetParent: invalid parent_id '{pid}'");
                        return DispatchResult::default();
                    }
                },
            };
            world.set_parent(child, parent);
            // Re-render affected entities if they have renderable components.
            let mut render_cmds = Vec::new();
            let affected = std::iter::once(child.index).chain(
                world
                    .get_children(&child)
                    .iter()
                    .map(|e| e.index)
                    .collect::<Vec<_>>(),
            );
            for idx in affected {
                if world.transforms.contains_key(&idx) {
                    if world.sprite_renderers.contains_key(&idx) {
                        render_cmds.push(RenderCommand::UpsertSprite { entity_idx: idx });
                    }
                    if world.text_renderers.contains_key(&idx) {
                        render_cmds.push(RenderCommand::UpsertText { entity_idx: idx });
                    }
                    if world.mesh_renderers.contains_key(&idx) {
                        render_cmds.push(RenderCommand::UpsertMesh { entity_idx: idx });
                    }
                }
            }
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
            let parent_id = EntityId::parse(&entity_id)
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
            let child_ids = EntityId::parse(&entity_id)
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

        // --- Component management --------------------------------------------
        ScriptMessage::ComponentSet {
            entity_id,
            component_type,
            data,
        } => {
            let Some(id) = EntityId::parse(&entity_id) else {
                tracing::warn!(mod_id, "ComponentSet: invalid entity_id '{entity_id}'");
                return DispatchResult::default();
            };
            world.set_component(&id, &component_type, data);
            let idx = id.index;
            let has_transform = world.transforms.contains_key(&idx);
            // Emit UpsertSprite only once both sprite renderer components are present.
            if (component_type == COMPONENT_TRANSFORM
                || component_type == COMPONENT_SPRITE_RENDERER)
                && has_transform
                && world.sprite_renderers.contains_key(&idx)
            {
                return DispatchResult {
                    render_cmds: vec![RenderCommand::UpsertSprite { entity_idx: idx }],
                    ..Default::default()
                };
            }
            // Emit UpsertText only once both text renderer components are present.
            if (component_type == COMPONENT_TRANSFORM || component_type == COMPONENT_TEXT_RENDERER)
                && has_transform
                && world.text_renderers.contains_key(&idx)
            {
                return DispatchResult {
                    render_cmds: vec![RenderCommand::UpsertText { entity_idx: idx }],
                    ..Default::default()
                };
            }
            // Emit UpsertMesh only once both mesh renderer components are present.
            if (component_type == COMPONENT_TRANSFORM || component_type == COMPONENT_MESH_RENDERER)
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
            let Some(id) = EntityId::parse(&entity_id) else {
                tracing::warn!(mod_id, "ComponentRemove: invalid entity_id '{entity_id}'");
                return DispatchResult::default();
            };
            let idx = id.index;
            world.remove_component(&id, &component_type);
            if component_type == COMPONENT_TRANSFORM || component_type == COMPONENT_SPRITE_RENDERER
            {
                return DispatchResult {
                    render_cmds: vec![RenderCommand::RemoveSprite { entity_idx: idx }],
                    ..Default::default()
                };
            }
            if component_type == COMPONENT_TRANSFORM || component_type == COMPONENT_TEXT_RENDERER {
                return DispatchResult {
                    render_cmds: vec![RenderCommand::RemoveText { entity_idx: idx }],
                    ..Default::default()
                };
            }
            if component_type == COMPONENT_TRANSFORM || component_type == COMPONENT_MESH_RENDERER {
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
            let reply = match EntityId::parse(&entity_id) {
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
                match EntityId::parse(&entity_id) {
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

        // --- Tick sync (intercepted in run_tick; this arm is a safety net) ---
        ScriptMessage::TickDone { .. } => {
            tracing::warn!(mod_id, "TickDone reached dispatcher unexpectedly");
        }
    }

    DispatchResult::default()
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
    let base = dirs::data_local_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine local data directory"))?;
    Ok(base.join("Whatever").join(game_id).join(mod_id))
}

fn resolve_mod_data_path(
    game_id: &str,
    mod_id: &str,
    path: &str,
) -> anyhow::Result<std::path::PathBuf> {
    if path.split('/').any(|c| c == "..") {
        anyhow::bail!("path traversal rejected: {path}");
    }
    Ok(mod_data_root(game_id, mod_id)?.join(path))
}
