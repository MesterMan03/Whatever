use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
pub enum EngineMessage {
    Init { mod_id: String, engine_version: String },
    Frame { delta_seconds: f32, frame_number: u64 },
    Input { keys_pressed: Vec<String>, mouse_delta: [f32; 2] },
    AssetResponse { request_id: String, path: String, data_base64: Option<String>, error: Option<String> },
    Shutdown { exit_code: i32 },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ScriptMessage {
    Subscribe { events: Vec<String> },
    AssetRequest { request_id: String, path: String },
    SpawnSprite { entity_id: String, texture: String, position: [f32; 3], scale: [f32; 3] },
    MoveEntity { entity_id: String, position: [f32; 3] },
    DestroyEntity { entity_id: String },
    Log { level: String, message: String },
    SetWindowTitle { title: String },
}