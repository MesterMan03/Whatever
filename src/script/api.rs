use super::ipc::ScriptMessage;
use crate::debug::DebugLogger;
use std::sync::Arc;
use winit::window::Window;

pub fn dispatch(mod_id: &str, msg: ScriptMessage, window: &Arc<Window>, debug: &mut DebugLogger) {
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
    }
}
