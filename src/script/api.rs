use super::host::{PendingReply, ScriptHost};
use super::ipc::{EngineMessage, ModManifestDto, ScriptMessage};
use crate::debug::DebugLogger;
use crate::mods::ModRegistry;
use base64::Engine as _;
use std::sync::Arc;
use winit::window::Window;

pub fn dispatch(
    mod_id: &str,
    msg: ScriptMessage,
    window: &Arc<Window>,
    script_host: &mut ScriptHost,
    registry: &ModRegistry,
    game_id: &str,
    debug: &mut DebugLogger,
) {
    match msg {
        ScriptMessage::Log { level, message } => match level.as_str() {
            "error" => tracing::error!(mod_id, "{message}"),
            "warn" => tracing::warn!(mod_id, "{message}"),
            _ => tracing::info!(mod_id, "{message}"),
        },
        ScriptMessage::SetWindowTitle { title } => {
            debug.window(&format!("[{mod_id}] SetWindowTitle: {title}"));
            window.set_title(&title);
        }
        ScriptMessage::Subscribe { events } => {
            tracing::debug!(mod_id, "subscribed to: {:?}", events);
        }
        ScriptMessage::SpawnSprite {
            entity_id,
            texture,
            position,
            scale,
        } => {
            tracing::debug!(
                mod_id,
                "SpawnSprite {entity_id} tex={texture} pos={position:?} scale={scale:?}"
            );
        }
        ScriptMessage::MoveEntity {
            entity_id,
            position,
        } => {
            tracing::debug!(mod_id, "MoveEntity {entity_id} pos={position:?}");
        }
        ScriptMessage::DestroyEntity { entity_id } => {
            tracing::debug!(mod_id, "DestroyEntity {entity_id}");
        }
        ScriptMessage::AssetRequest { request_id, path } => {
            tracing::debug!(mod_id, "AssetRequest {request_id} path={path}");
        }
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
            let reply = EngineMessage::FileResponse {
                request_id,
                data_base64: None,
                error: result.err().map(|e| e.to_string()),
            };
            script_host.send(mod_id, &reply, debug);
        }
        ScriptMessage::FileRead { request_id, path } => {
            let result = (|| -> anyhow::Result<String> {
                let full_path = resolve_mod_data_path(game_id, mod_id, &path)?;
                let bytes = std::fs::read(&full_path)?;
                Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
            })();
            let reply = EngineMessage::FileResponse {
                request_id,
                data_base64: result.as_ref().ok().cloned(),
                error: result.err().map(|e| e.to_string()),
            };
            script_host.send(mod_id, &reply, debug);
        }
        ScriptMessage::FileDelete { request_id, path } => {
            let result = (|| -> anyhow::Result<()> {
                let full_path = resolve_mod_data_path(game_id, mod_id, &path)?;
                std::fs::remove_file(&full_path)?;
                Ok(())
            })();
            let reply = EngineMessage::FileResponse {
                request_id,
                data_base64: None,
                error: result.err().map(|e| e.to_string()),
            };
            script_host.send(mod_id, &reply, debug);
        }
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
        ScriptMessage::ModMessageSend {
            target_mod_id,
            request_id,
            payload,
        } => {
            if !script_host.has_process(&target_mod_id) {
                tracing::warn!(mod_id, "ModMessageSend to unknown mod '{target_mod_id}'");
                return;
            }
            // Build a namespaced key so two mods using the same numeric counter can't collide.
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
            // Forward the opaque namespaced key to the target — their lib echoes it back verbatim.
            let outgoing = EngineMessage::ModMessageReceived {
                source_mod_id: mod_id.to_owned(),
                request_id: namespaced_id,
                payload,
            };
            script_host.send(&target_mod_id, &outgoing, debug);
        }
        ScriptMessage::RegisterCommand { name, .. } => {
            tracing::warn!(mod_id, "RegisterCommand for '{name}' reached dispatcher unexpectedly");
        }
        ScriptMessage::CommandResponse { request_id, .. } => {
            tracing::warn!(mod_id, "CommandResponse '{request_id}' reached dispatcher unexpectedly");
        }
        ScriptMessage::ModMessageReply {
            request_id,
            payload,
        } => {
            // request_id is the full namespaced key ("<sender_mod_id>-<original_id>") echoed by target.
            match script_host.take_pending_reply(&request_id) {
                Some(pending) => {
                    let reply = EngineMessage::ModMessageReplyDelivered {
                        request_id: pending.original_request_id,
                        payload,
                    };
                    script_host.send(&pending.sender_mod_id, &reply, debug);
                }
                None => tracing::warn!(mod_id, "ModMessageReply for unknown key '{request_id}'"),
            }
        }
    }
}

fn resolve_mod_data_path(
    game_id: &str,
    mod_id: &str,
    path: &str,
) -> anyhow::Result<std::path::PathBuf> {
    if path.split('/').any(|c| c == "..") {
        anyhow::bail!("path traversal rejected: {path}");
    }
    let base = dirs::data_local_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine local data directory"))?;
    Ok(base.join("Whatever").join(game_id).join(mod_id).join(path))
}
