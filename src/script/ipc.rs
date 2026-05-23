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
    Tick {
        tick_number: u64,
        delta_seconds: f64,
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
        manifest: Option<Box<ModManifestDto>>,
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
    EntityCreated {
        request_id: String,
        entity_id: String,
    },
    EntityListResponse {
        request_id: String,
        entity_ids: Vec<String>,
    },
    ComponentGetResponse {
        request_id: String,
        entity_id: String,
        component_type: String,
        data: Option<serde_json::Value>,
        error: Option<String>,
    },
    ComponentQueryResponse {
        request_id: String,
        results: Vec<QueryResultDto>,
    },
    Shutdown {
        exit_code: i32,
    },
    CommandInvoke {
        request_id: String,
        command_path: Vec<String>,
        args: Vec<serde_json::Value>,
    },
}

/// One row returned by a `ComponentQuery` response.
#[derive(Debug, Clone, Serialize)]
pub struct QueryResultDto {
    pub entity_id: String,
    pub components: HashMap<String, serde_json::Value>,
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
    RegisterCommand {
        name: String,
        description: String,
        subcommands: Vec<CommandNodeSpec>,
        args: Vec<ArgSpecDto>,
        #[serde(default)]
        has_handler: bool,
    },
    CommandResponse {
        request_id: String,
        output: Vec<String>,
        error: Option<String>,
    },
    EntityCreate {
        request_id: String,
    },
    EntityDestroy {
        entity_id: String,
    },
    EntityListRequest {
        request_id: String,
    },
    /// Fire-and-forget: sets a component on an entity.
    ComponentSet {
        entity_id: String,
        component_type: String,
        data: serde_json::Value,
    },
    /// Fire-and-forget: removes a component from an entity.
    ComponentRemove {
        entity_id: String,
        component_type: String,
    },
    ComponentGet {
        request_id: String,
        entity_id: String,
        component_type: String,
    },
    ComponentQuery {
        request_id: String,
        component_types: Vec<String>,
    },
    SetWindowSize {
        width: u32,
        height: u32,
    },
    SetWindowMode {
        mode: String,
    },
    SetTickRate {
        ticks_per_second: f64,
    },
    SetFpsCap {
        fps: Option<f64>,
    },
    SetVsync {
        enabled: bool,
    },
    TickDone {
        tick_number: u64,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommandNodeSpec {
    pub name: String,
    pub description: String,
    pub subcommands: Vec<CommandNodeSpec>,
    pub args: Vec<ArgSpecDto>,
    #[serde(default)]
    pub has_handler: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArgSpecDto {
    pub name: String,
    #[serde(rename = "type")]
    pub arg_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: String,
}
