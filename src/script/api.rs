use super::host::{PendingReply, ScriptHost};
use super::ipc::{EngineMessage, ModManifestDto, QueryResultDto, ScriptMessage};
use crate::debug::DebugLogger;
use crate::ecs::{COMPONENT_SPRITE_RENDERER, COMPONENT_TRANSFORM, EntityId, World};
use crate::mods::ModRegistry;
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
}

pub enum RenderCommand {
    UpsertSprite { entity_idx: u32 },
    RemoveSprite { entity_idx: u32 },
}

// --- Dispatcher --------------------------------------------------------------

pub struct EngineContext<'a> {
    pub window: &'a Arc<Window>,
    pub script_host: &'a mut ScriptHost,
    pub registry: &'a ModRegistry,
    pub game_id: &'a str,
    pub debug: &'a mut DebugLogger,
    pub world: &'a mut World,
}

pub fn dispatch(mod_id: &str, msg: ScriptMessage, ctx: EngineContext) -> DispatchResult {
    let EngineContext {
        window,
        script_host,
        registry,
        game_id,
        debug,
        world,
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
            let had_sprite = world.is_alive(&id) && world.sprite_renderers.contains_key(&id.index);
            world.destroy_entity(id);
            if had_sprite {
                return DispatchResult {
                    render_cmds: vec![RenderCommand::RemoveSprite {
                        entity_idx: id.index,
                    }],
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
            // Emit UpsertSprite only once both renderer components are present.
            let is_renderer_comp = component_type == COMPONENT_TRANSFORM
                || component_type == COMPONENT_SPRITE_RENDERER;
            if is_renderer_comp
                && world.transforms.contains_key(&id.index)
                && world.sprite_renderers.contains_key(&id.index)
            {
                return DispatchResult {
                    render_cmds: vec![RenderCommand::UpsertSprite {
                        entity_idx: id.index,
                    }],
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
            let is_renderer_comp = component_type == COMPONENT_TRANSFORM
                || component_type == COMPONENT_SPRITE_RENDERER;
            world.remove_component(&id, &component_type);
            if is_renderer_comp {
                return DispatchResult {
                    render_cmds: vec![RenderCommand::RemoveSprite {
                        entity_idx: id.index,
                    }],
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

        // --- Tick sync (intercepted in run_tick; this arm is a safety net) ---
        ScriptMessage::TickDone { .. } => {
            tracing::warn!(mod_id, "TickDone reached dispatcher unexpectedly");
        }
    }

    DispatchResult::default()
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
