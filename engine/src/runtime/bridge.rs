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
    pub mouse_captured: bool,
    pub mouse_buttons: Vec<String>,
    pub mouse_buttons_pressed: Vec<String>,
}
impl RuntimeInput {
    pub fn take_tick(&mut self) -> (Vec<String>, (f32, f32), bool, Vec<String>, Vec<String>) {
        let mouse = std::mem::take(&mut self.mouse);
        let pressed = std::mem::take(&mut self.mouse_buttons_pressed);
        (
            self.keys.clone(),
            if self.mouse_captured {
                mouse
            } else {
                (0.0, 0.0)
            },
            self.mouse_captured,
            if self.mouse_captured {
                self.mouse_buttons.clone()
            } else {
                vec![]
            },
            if self.mouse_captured { pressed } else { vec![] },
        )
    }
}
pub enum RuntimeCommand {
    Editor(super::editor::EditorControl),
    ConsoleInput(String),
    LoadScene {
        path: String,
        request_id: String,
    },
    Ready,
    Ipc {
        mod_id: String,
        message: EngineMessage,
    },
}
pub struct Presentation {
    pub world: World,
    pub(crate) motion: crate::presentation::MotionHistory,
    pub main_camera: Option<EntityId>,
    pub tick_rate: f64,
    pub editor_sequence: u64,
    pub render_cmds: std::collections::HashMap<(u8, u32), RenderCommand>,
    pub events: Vec<(String, ScriptMessage)>,
    pub client_commands: Vec<ScriptMessage>,
}
impl Presentation {
    pub fn merge(&mut self, newer: Self) {
        self.world = newer.world;
        self.motion = newer.motion;
        self.main_camera = newer.main_camera;
        self.tick_rate = newer.tick_rate;
        self.editor_sequence = newer.editor_sequence;
        self.render_cmds.extend(newer.render_cmds);
        self.events.extend(newer.events);
        self.client_commands.extend(newer.client_commands);
    }
}
pub struct Shared {
    pub input: Mutex<RuntimeInput>,
    pub presentation: Mutex<Option<Presentation>>,
    pub stop: AtomicBool,
    pub frame_timings: Mutex<Option<super::FrameTimings>>,
    pub editor_snapshot: Mutex<Option<std::sync::Weak<super::editor_transport::EditorMailbox>>>,
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
            frame_timings: Mutex::new(None),
            editor_snapshot: Mutex::new(None),
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
    pub fn load_scene(&self, path: String, request_id: String) {
        let _ = self
            .commands
            .send(RuntimeCommand::LoadScene { path, request_id });
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
            motion: Default::default(),
            main_camera: None,
            tick_rate: 60.0,
            editor_sequence: 0,
            render_cmds: std::iter::once((command.key(), command)).collect(),
            events: vec![],
            client_commands: vec![],
        }
    }
    #[test]
    fn button_edges_are_drained_once_and_capture_loss_discards_them() {
        let mut input = RuntimeInput {
            mouse_captured: true,
            mouse_buttons: vec!["Left".into()],
            mouse_buttons_pressed: vec!["Left".into()],
            ..Default::default()
        };
        let tick = input.take_tick();
        assert_eq!(tick.3, vec!["Left"]);
        assert_eq!(tick.4, vec!["Left"]);
        assert!(input.take_tick().4.is_empty());
        input.mouse_buttons_pressed.push("Right".into());
        input.mouse_captured = false;
        let tick = input.take_tick();
        assert!(tick.3.is_empty());
        assert!(tick.4.is_empty());
        input.mouse_captured = true;
        assert!(input.take_tick().4.is_empty());
    }
    #[test]
    fn uncaptured_ticks_discard_queued_mouse_motion() {
        let mut input = RuntimeInput {
            mouse: (10.0, -4.0),
            ..Default::default()
        };
        assert_eq!(input.take_tick().1, (0.0, 0.0));
        input.mouse_captured = true;
        input.mouse = (2.0, 3.0);
        assert_eq!(input.take_tick().1, (2.0, 3.0));
        assert_eq!(input.take_tick().1, (0.0, 0.0));
        input.mouse = (9.0, 1.0);
        input.mouse_captured = false;
        assert_eq!(input.take_tick().1, (0.0, 0.0));
        input.mouse_captured = true;
        assert_eq!(input.take_tick().1, (0.0, 0.0));
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
