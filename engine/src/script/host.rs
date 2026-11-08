//! Engine-local message routing and persistent Wasm instances. No processes or pipes.
use super::{EmbeddedRuntime, ipc::EngineMessage, runtime::ScriptPackage};
use crate::debug::DebugLogger;
use std::collections::{HashMap, HashSet, VecDeque};

pub struct PendingReply {
    pub sender_mod_id: String,
    pub original_request_id: String,
}
pub struct LoadedScript {
    pub package: ScriptPackage,
    pub entry: String,
    pub mode: crate::runtime::RuntimeMode,
    pub booted: bool,
}
pub struct ScriptHost {
    pub runtime: Option<EmbeddedRuntime>,
    pub scripts: HashMap<String, LoadedScript>,
    messages: HashMap<String, VecDeque<EngineMessage>>,
    pending_replies: HashMap<String, PendingReply>,
    dead: HashSet<String>,
}
impl ScriptHost {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            runtime: Some(EmbeddedRuntime::bundled()?),
            scripts: HashMap::new(),
            messages: HashMap::new(),
            pending_replies: HashMap::new(),
            dead: HashSet::new(),
        })
    }
    pub fn register(
        &mut self,
        id: &str,
        package: ScriptPackage,
        entry: &str,
        mode: crate::runtime::RuntimeMode,
    ) -> anyhow::Result<()> {
        let entry = entry.trim_start_matches("./");
        anyhow::ensure!(
            package.modules.contains_key(entry),
            "script entry '{entry}' is missing or outside the mod's source mount"
        );
        self.scripts.insert(
            id.into(),
            LoadedScript {
                package,
                entry: entry.into(),
                mode,
                booted: false,
            },
        );
        Ok(())
    }
    pub fn send(&mut self, id: &str, message: &EngineMessage, _debug: &mut DebugLogger) -> bool {
        if !self.contains_mod(id) {
            return false;
        }
        let queue = self.messages.entry(id.into()).or_default();
        if queue.len() >= 4096 {
            self.terminate(id);
            return false;
        }
        queue.push_back(message.clone());
        true
    }
    pub fn take_messages(&mut self, id: &str) -> Vec<EngineMessage> {
        self.messages
            .remove(id)
            .unwrap_or_default()
            .into_iter()
            .collect()
    }
    pub fn contains_mod(&self, id: &str) -> bool {
        self.scripts.contains_key(id) && !self.dead.contains(id)
    }
    pub fn is_ready(&self, id: &str) -> bool {
        self.contains_mod(id) && self.scripts.get(id).is_some_and(|script| script.booted)
    }
    pub fn terminate(&mut self, id: &str) {
        self.dead.insert(id.into());
        self.messages.remove(id);
    }
    pub fn reap_exited(&mut self) -> Vec<String> {
        let ids = self.dead.drain().collect::<Vec<_>>();
        for id in &ids {
            self.scripts.remove(id);
            if let Some(runtime) = &mut self.runtime {
                runtime.reset(id);
            }
        }
        ids
    }
    pub fn add_pending_reply(&mut self, sender: &str, request: &str, reply: PendingReply) {
        self.pending_replies
            .insert(format!("{sender}-{request}"), reply);
    }
    pub fn take_pending_reply(&mut self, key: &str) -> Option<PendingReply> {
        self.pending_replies.remove(key)
    }
    pub fn shutdown_all(&mut self, _code: i32, _debug: &mut DebugLogger) {
        self.scripts.clear();
        self.messages.clear();
    }
}
