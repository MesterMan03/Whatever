use crate::mods::ModManifest;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// --- Mod manifest DTO (flat shape matching the TypeScript ModManifest type) ---

#[derive(Debug, Clone, Serialize)]
pub struct ModManifestDto {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    pub license: String,
    pub dependencies: HashMap<String, String>,
    pub load_order: LoadOrderDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script: Option<ScriptConfigDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadOrderDto {
    pub after: Vec<String>,
    pub before: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScriptConfigDto {
    pub entry: String,
    pub runtime: String,
}

impl From<&ModManifest> for ModManifestDto {
    fn from(m: &ModManifest) -> Self {
        ModManifestDto {
            id: m.meta.id.clone(),
            name: m.meta.name.clone(),
            version: m.meta.version.clone(),
            description: m.meta.description.clone(),
            authors: m.meta.authors.clone(),
            license: m.meta.license.clone(),
            dependencies: m.dependencies.clone(),
            load_order: LoadOrderDto {
                after: m.load_order.after.clone(),
                before: m.load_order.before.clone(),
            },
            script: m.script.as_ref().map(|s| ScriptConfigDto {
                entry: s.entry.clone(),
                runtime: s.runtime.clone(),
            }),
        }
    }
}

// --- Engine → Script messages ---

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
pub enum EngineMessage {
    Init {
        mod_id: String,
        engine_version: String,
    },
    Frame {
        delta_seconds: f32,
        frame_number: u64,
    },
    Input {
        keys_pressed: Vec<String>,
        mouse_delta: [f32; 2],
    },
    AssetResponse {
        request_id: String,
        path: String,
        data_base64: Option<String>,
        error: Option<String>,
    },
    FileResponse {
        request_id: String,
        data_base64: Option<String>,
        error: Option<String>,
    },
    ModListResponse {
        request_id: String,
        mods: Vec<ModManifestDto>,
    },
    ModGetResponse {
        request_id: String,
        manifest: Option<ModManifestDto>,
        error: Option<String>,
    },
    ModMessageReceived {
        source_mod_id: String,
        request_id: Option<String>,
        payload: serde_json::Value,
    },
    ModMessageReplyDelivered {
        request_id: String,
        payload: serde_json::Value,
    },
    Shutdown {
        exit_code: i32,
    },
}

// --- Script → Engine messages ---

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ScriptMessage {
    Subscribe {
        events: Vec<String>,
    },
    AssetRequest {
        request_id: String,
        path: String,
    },
    SpawnSprite {
        entity_id: String,
        texture: String,
        position: [f32; 3],
        scale: [f32; 3],
    },
    MoveEntity {
        entity_id: String,
        position: [f32; 3],
    },
    DestroyEntity {
        entity_id: String,
    },
    Log {
        level: String,
        message: String,
    },
    SetWindowTitle {
        title: String,
    },
    FileWrite {
        request_id: String,
        path: String,
        data_base64: String,
    },
    FileRead {
        request_id: String,
        path: String,
    },
    FileDelete {
        request_id: String,
        path: String,
    },
    ModListRequest {
        request_id: String,
    },
    ModGetRequest {
        request_id: String,
        mod_id: String,
    },
    ModMessageSend {
        target_mod_id: String,
        request_id: Option<String>,
        payload: serde_json::Value,
    },
    ModMessageReply {
        request_id: String,
        payload: serde_json::Value,
    },
}
