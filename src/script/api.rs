use super::host::ScriptHost;
use super::ipc::{EngineMessage, ScriptMessage};
use crate::debug::DebugLogger;
use base64::Engine as _;
use std::sync::Arc;
use winit::window::Window;

pub fn dispatch(
    mod_id: &str,
    msg: ScriptMessage,
    window: &Arc<Window>,
    script_host: &mut ScriptHost,
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
