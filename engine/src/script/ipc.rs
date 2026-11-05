fn default_recursive() -> bool {
    true
}

use crate::mods::ModManifest;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// --- Mod manifest DTO (flat shape matching the TypeScript ModManifest type) ---

#[derive(Debug, Clone, Serialize)]
pub struct ModManifestDto {
    pub id: String,
    pub sides: Vec<crate::runtime::RuntimeMode>,
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
    pub entry: Option<String>,
    pub client: Option<String>,
    pub server: Option<String>,
    pub runtime: String,
}

impl From<&ModManifest> for ModManifestDto {
    fn from(m: &ModManifest) -> Self {
        ModManifestDto {
            id: m.meta.id.clone(),
            sides: m.meta.sides.clone(),
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
                client: s.client.clone(),
                server: s.server.clone(),
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
    Performance {
        sample: Box<crate::runtime::TickTimings>,
    },
    SceneDelta {
        delta: crate::ecs::SceneDelta,
    },
    EntityTagsResponse {
        request_id: String,
        tags: Vec<String>,
        error: Option<String>,
    },
    SceneLoaded {
        request_id: String,
        scene: Option<crate::scene::LoadedScene>,
        error: Option<String>,
    },
    MainCameraResponse {
        request_id: String,
        entity_id: Option<String>,
    },
    PhysicsResponse {
        request_id: String,
        result: Option<serde_json::Value>,
        error: Option<String>,
    },
    PhysicsEvent {
        event: serde_json::Value,
    },
    CommandResults {
        tick_number: u64,
        messages: Vec<EngineMessage>,
    },
    NetworkEvent {
        #[serde(flatten)]
        payload: serde_json::Value,
    },
    NetworkResponse {
        request_id: String,
        result: Option<serde_json::Value>,
        error: Option<String>,
    },
    Init {
        initial_scene: Option<crate::scene::LoadedScene>,
        mod_id: String,
        engine_version: String,
        mode: crate::runtime::RuntimeMode,
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
        mouse_captured: bool,
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
    EntityCreateFailed {
        request_id: String,
        error: String,
    },
    EntityCreated {
        request_id: String,
        entity_id: String,
    },
    EntityListResponse {
        request_id: String,
        entity_ids: Vec<String>,
    },
    EntityParentResponse {
        request_id: String,
        entity_id: String,
        /// `None` means the entity has no parent.
        parent_id: Option<String>,
    },
    EntityChildrenResponse {
        request_id: String,
        entity_id: String,
        child_ids: Vec<String>,
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
    AudioLoaded {
        request_id: String,
        audio_id: String,
        duration_ms: Option<u64>,
        sample_rate: u32,
        channels: u16,
        error: Option<String>,
    },
    AudioState {
        request_id: String,
        audio_id: String,
        position_ms: u64,
        volume: f32,
        speed: f32,
        is_playing: bool,
        is_looping: bool,
        error: Option<String>,
    },
    AudioClose {
        audio_id: String,
    },
    CommandInvoke {
        request_id: String,
        command_path: Vec<String>,
        args: Vec<serde_json::Value>,
    },
    ArgSuggestRequest {
        request_id: String,
        command_path: Vec<String>,
        arg_index: usize,
        current: String,
    },
}

/// One row returned by a `ComponentQuery` response.
#[derive(Debug, Clone, Serialize)]
pub struct QueryResultDto {
    pub entity_id: String,
    pub components: HashMap<String, serde_json::Value>,
}

// --- Script → Engine messages ---

/// Transport envelopes. Application commands are accepted only inside a tick batch.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ScriptPacket {
    RuntimeReady {
        protocol: u32,
    },
    RuntimeFailed {
        message: String,
    },
    CommandBatch {
        tick_number: u64,
        commands: Vec<ScriptMessage>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ScriptMessage {
    PrefabSpawn {
        request_id: String,
        path: String,
    },
    SceneLoad {
        request_id: String,
        path: String,
    },
    GetMainCamera {
        request_id: String,
    },
    PhysicsRequest {
        request_id: String,
        request: serde_json::Value,
    },
    NetworkRequest {
        request_id: String,
        request: serde_json::Value,
    },
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
    ArgSuggestResponse {
        request_id: String,
        suggestions: Vec<String>,
    },
    EntityClone {
        request_id: String,
        entity_id: String,
        #[serde(default = "default_recursive")]
        recursive: bool,
        enabled: Option<bool>,
    },
    EntitySetEnabled {
        entity_id: String,
        enabled: bool,
    },
    EntityCreate {
        request_id: String,
    },
    EntityDestroy {
        entity_id: String,
    },
    EntityMove {
        entity_id: String,
        position: [f32; 3],
    },
    EntityListRequest {
        request_id: String,
    },
    /// Attach or detach a parent. `parent_id = None` detaches the entity from its current parent.
    EntitySetParent {
        entity_id: String,
        parent_id: Option<String>,
    },
    EntityGetParent {
        request_id: String,
        entity_id: String,
    },
    EntityGetChildren {
        request_id: String,
        entity_id: String,
    },
    EntityTagAdd {
        entity_id: String,
        tag: String,
    },
    EntityTagRemove {
        entity_id: String,
        tag: String,
    },
    EntityTagsGet {
        request_id: String,
        entity_id: String,
    },
    EntityTagQuery {
        request_id: String,
        query: crate::ecs::TagQuery,
    },
    /// Fire-and-forget: sets a component on an entity.
    ComponentPatch {
        entity_id: String,
        component_type: String,
        data: serde_json::Value,
    },
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
    AudioLoad {
        request_id: String,
        audio_id: String,
        path: String,
        play: bool,
        volume: f32,
        speed: f32,
        loop_: bool,
        close_strategy: String,
    },
    AudioPlay {
        request_id: String,
        audio_id: String,
        volume: Option<f32>,
        speed: Option<f32>,
    },
    AudioPause {
        request_id: String,
        audio_id: String,
    },
    AudioStop {
        audio_id: String,
    },
    AudioSeekTo {
        request_id: String,
        audio_id: String,
        position_ms: u64,
    },
    AudioSeek {
        request_id: String,
        audio_id: String,
        offset_ms: i64,
    },
    AudioSetLoop {
        audio_id: String,
        loop_: bool,
    },
    AudioQuery {
        request_id: String,
        audio_id: String,
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
    /// Set the main camera entity.  The entity must have a `core:camera`
    /// component and a `core:transform` component to be used as a camera.
    /// Pass an empty string to clear the camera (black screen).
    SetMainCamera {
        entity_id: String,
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
    #[serde(default)]
    pub has_suggest: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_commands_require_a_non_nested_tick_envelope() {
        use serde_json::json;
        let log = json!({"type":"Log","level":"info","message":"hello"});
        assert!(serde_json::from_value::<ScriptPacket>(log.clone()).is_err());
        let batch = json!({"type":"CommandBatch","tick_number":7,"commands":[log]});
        assert!(matches!(
            serde_json::from_value::<ScriptPacket>(batch.clone()),
            Ok(ScriptPacket::CommandBatch { tick_number: 7, .. })
        ));
        assert!(
            serde_json::from_value::<ScriptPacket>(
                json!({"type":"CommandBatch","tick_number":7,"commands":[batch]})
            )
            .is_err()
        );
        assert!(serde_json::from_value::<ScriptPacket>(json!({"type":"CommandBatch","tick_number":7,"commands":[{"type":"RuntimeReady","protocol":6}]})).is_err());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentInstance {
    pub instance_id: String,
    pub entity_id: String,
    pub component_type: String,
    pub data: serde_json::Value,
    pub active: bool,
}
