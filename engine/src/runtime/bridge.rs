use super::{LoadedGame, simulation::Simulation};
use crate::{
    ecs::{EntityId, World},
    script::{
        RenderCommand,
        ipc::{EngineMessage, ScriptMessage},
    },
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::JoinHandle;
#[derive(Default)]
pub struct RuntimeInput {
    pub keys: Vec<String>,
    pub mouse: (f32, f32),
}
pub enum RuntimeCommand {
    ConsoleInput(String),
    Ready,
    Ipc {
        mod_id: String,
        message: EngineMessage,
    },
}
pub struct Presentation {
    pub world: World,
    pub main_camera: Option<EntityId>,
    pub tick_rate: f64,
    pub render_cmds: std::collections::HashMap<(u8, u32), RenderCommand>,
    pub events: Vec<(String, ScriptMessage)>,
    pub client_commands: Vec<ScriptMessage>,
}
impl Presentation {
    pub fn merge(&mut self, newer: Self) {
        self.world = newer.world;
        self.main_camera = newer.main_camera;
        self.tick_rate = newer.tick_rate;
        self.render_cmds.extend(newer.render_cmds);
        self.events.extend(newer.events);
        self.client_commands.extend(newer.client_commands);
    }
}
pub struct Shared {
    pub input: Mutex<RuntimeInput>,
    pub presentation: Mutex<Option<Presentation>>,
    pub stop: AtomicBool,
}
pub struct SimulationHandle {
    pub shared: Arc<Shared>,
    pub commands: mpsc::Sender<RuntimeCommand>,
    worker: Option<JoinHandle<anyhow::Result<()>>>,
}
impl SimulationHandle {
    pub fn spawn(game: LoadedGame) -> anyhow::Result<Self> {
        let shared = Arc::new(Shared {
            input: Mutex::new(RuntimeInput::default()),
            presentation: Mutex::new(None),
            stop: AtomicBool::new(false),
        });
        let (commands, rx) = mpsc::channel();
        let worker_shared = Arc::clone(&shared);
        let worker = std::thread::Builder::new()
            .name("whatever-simulation".into())
            .spawn(move || Simulation::new(game, worker_shared, rx)?.run())?;
        Ok(Self {
            shared,
            commands,
            worker: Some(worker),
        })
    }
    pub fn ready(&self) {
        let _ = self.commands.send(RuntimeCommand::Ready);
    }
    pub fn send(&self, mod_id: String, message: EngineMessage) {
        let _ = self.commands.send(RuntimeCommand::Ipc { mod_id, message });
    }
    pub fn take_presentation(&self) -> Option<Presentation> {
        self.shared.presentation.lock().ok()?.take()
    }
    pub fn is_finished(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }
    pub fn shutdown(&mut self) -> anyhow::Result<()> {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("simulation thread panicked"))??;
        }
        Ok(())
    }
}
impl Drop for SimulationHandle {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown() {
            tracing::error!("simulation shutdown: {error:#}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(command: RenderCommand) -> Presentation {
        Presentation {
            world: World::new(),
            main_camera: None,
            tick_rate: 60.0,
            render_cmds: std::iter::once((command.key(), command)).collect(),
            events: vec![],
            client_commands: vec![],
        }
    }
    #[test]
    fn skipped_frames_keep_latest_world_and_latest_render_operation() {
        let mut first = snapshot(RenderCommand::UpsertSprite { entity_idx: 0 });
        first.world.allocator.alloc();
        first.merge(snapshot(RenderCommand::RemoveSprite { entity_idx: 0 }));
        assert_eq!(first.world.allocator.alive_entity_ids().count(), 0);
        assert_eq!(first.render_cmds.len(), 1);
        assert!(matches!(
            first.render_cmds.get(&(0, 0)),
            Some(RenderCommand::RemoveSprite { entity_idx: 0 })
        ));
        first.merge(snapshot(RenderCommand::UpsertMesh { entity_idx: 1 }));
        assert_eq!(first.render_cmds.len(), 2);
    }
}
