use super::{
    LoadedGame, RuntimeMode,
    bridge::{Presentation, RuntimeCommand, Shared},
    scheduler::TickClock,
};
use crate::{
    audio::AudioManager,
    debug::DebugLogger,
    ecs::{EntityId, World},
    mods::{GameMeta, ModRegistry},
    sandbox::SandboxConfig,
    script::{
        EngineContext, RecvOutcome, ScriptHost, dispatch,
        ipc::{EngineMessage, ScriptMessage, ScriptPacket},
        mod_data_root,
    },
    vfs::VfsHandle,
    watchdog::Watchdog,
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, atomic::Ordering, mpsc},
    time::{Duration, Instant},
};
#[cfg(test)]
#[path = "network_demo_tests.rs"]
mod network_demo_tests;
pub struct Simulation {
    mode: RuntimeMode,
    meta: GameMeta,
    registry: Arc<ModRegistry>,
    vfs: VfsHandle,
    debug: DebugLogger,
    script_host: ScriptHost,
    network: crate::network::NetworkRegistry,
    server_console: Option<crate::console::DevConsole>,
    world: World,
    physics: crate::physics::PhysicsRegistry,
    audio: AudioManager,
    clock: TickClock,
    tick_number: u64,
    startup_packets: Vec<(String, ScriptPacket)>,
    scripts_initialized: bool,
    pending_controls: Vec<RuntimeCommand>,
    ready: bool,
    watchdog: Watchdog,
    shared: Arc<Shared>,
    commands: mpsc::Receiver<RuntimeCommand>,
    render_cmds: Vec<crate::script::RenderCommand>,
    events: Vec<(String, ScriptMessage)>,
    client_commands: Vec<ScriptMessage>,
    main_camera: Option<EntityId>,
    changed: bool,
}
impl Simulation {
    pub fn new(
        game: LoadedGame,
        shared: Arc<Shared>,
        commands: mpsc::Receiver<RuntimeCommand>,
    ) -> anyhow::Result<Self> {
        let audio = if game.mode == RuntimeMode::Client {
            AudioManager::new()?
        } else {
            AudioManager::headless()
        };
        Self::with_audio(game, shared, commands, audio)
    }
    fn with_audio(
        game: LoadedGame,
        shared: Arc<Shared>,
        commands: mpsc::Receiver<RuntimeCommand>,
        audio: AudioManager,
    ) -> anyhow::Result<Self> {
        let LoadedGame {
            mode,
            network,
            meta,
            registry,
            vfs,
            mut debug,
        } = game;
        let clock = TickClock::new(meta.game.tick_rate)?;
        let mut script_host = ScriptHost::new()?;
        for loaded_mod in registry.iter() {
            let Some(ref script_cfg) = loaded_mod.manifest.script else {
                continue;
            };
            let Some(script_entry) = script_cfg.entry_for(mode) else {
                continue;
            };
            let entry = loaded_mod.root.join(script_entry);
            let mod_id = &loaded_mod.manifest.meta.id;

            let mod_data_dir = match mod_data_root(&meta.game.id, mod_id) {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!(mod_id, "could not determine mod data dir: {e:#}");
                    continue;
                }
            };
            if let Err(e) = std::fs::create_dir_all(&mod_data_dir) {
                tracing::error!(mod_id, "could not create mod data dir: {e:#}");
                continue;
            }

            let sandbox_cfg = SandboxConfig {
                mod_id: mod_id.clone(),
                mod_root: loaded_mod.root.clone(),
                runtime_executable: script_host.runtime_path().to_path_buf(),
            };

            if let Err(e) = script_host.spawn(mod_id, &entry, mode, &sandbox_cfg, &mut debug) {
                tracing::error!(mod_id, entry = %entry.display(), "script spawn failed: {e:#}");
            }
        }

        Ok(Self {
            mode,
            meta,
            registry,
            vfs,
            debug,
            script_host,
            network: crate::network::NetworkRegistry::new(network),
            server_console: (mode == RuntimeMode::Server).then(crate::console::DevConsole::new),
            world: World::new(),
            physics: crate::physics::PhysicsRegistry::default(),
            audio,
            clock,
            tick_number: 0,
            startup_packets: vec![],
            scripts_initialized: false,
            pending_controls: vec![],
            ready: false,
            watchdog: Watchdog::new(Duration::from_millis(500)),
            shared,
            commands,
            render_cmds: vec![],
            events: vec![],
            client_commands: vec![],
            main_camera: None,
            changed: true,
        })
    }
    fn network_events(&mut self) {
        for (owner, message) in self.network.poll() {
            if !self.script_host.send(&owner, &message, &mut self.debug) {
                self.network.remove_mod(&owner);
            }
        }
    }
    fn stopped(&self) -> bool {
        self.shared.stop.load(Ordering::Relaxed)
    }
    fn controls(&mut self) {
        while let Ok(command) = self.commands.try_recv() {
            match command {
                RuntimeCommand::Ready => {
                    if !self.ready {
                        self.ready = true;
                        self.clock.next = Instant::now() + self.clock.interval;
                    }
                }
                command => self.pending_controls.push(command),
            }
        }
    }
    pub fn run(mut self) -> anyhow::Result<()> {
        while !self.stopped() {
            self.controls();
            if self.ready && Instant::now() >= self.clock.next {
                self.tick();
                self.clock.complete(Instant::now());
            }
            let wait = if self.ready {
                self.clock
                    .next
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(5))
            } else {
                Duration::from_millis(5)
            };
            if !wait.is_zero() {
                std::thread::sleep(wait);
            }
        }
        self.network.shutdown();
        self.network_events();
        let messages = self.script_host.shutdown_all(0, &mut self.debug);
        for (id, packet) in messages {
            if let ScriptPacket::CommandBatch { commands, .. } = packet {
                for message in commands {
                    if let ScriptMessage::Log { .. } = message {
                        self.message(id.clone(), message, &mut HashMap::new());
                    }
                }
            }
        }
        tracing::info!(ticks = self.tick_number, "simulation stopped");
        Ok(())
    }
    fn server_command(&mut self, input: &str) {
        let Some(console) = self.server_console.as_mut() else {
            return;
        };
        let action = console.execute_headless(
            input,
            &self.registry,
            self.vfs.as_ref(),
            self.debug.shared_switches(),
            &self.world,
        );
        match action {
            crate::console::ConsoleAction::Quit => self.shared.stop.store(true, Ordering::Relaxed),
            crate::console::ConsoleAction::SendIpc { mod_id, message } => {
                if !self.script_host.send(&mod_id, &message, &mut self.debug) {
                    console.handle_command_response(
                        vec![],
                        Some("command's mod process is unavailable".into()),
                    );
                    console.discard_output();
                }
            }
            crate::console::ConsoleAction::EngineSettings(_) => {
                tracing::warn!("presentation settings are unavailable on a headless server")
            }
            crate::console::ConsoleAction::None => {}
        }
    }
    fn server_console_message(&mut self, mod_id: &str, message: &ScriptMessage) {
        let Some(console) = self.server_console.as_mut() else {
            return;
        };
        match message {
            ScriptMessage::RegisterCommand {
                name,
                description,
                subcommands,
                args,
                has_handler,
            } => {
                let spec = crate::script::ipc::CommandNodeSpec {
                    name: name.clone(),
                    description: description.clone(),
                    subcommands: subcommands.clone(),
                    args: args.clone(),
                    has_handler: *has_handler,
                };
                console.registry.register_mod(
                    mod_id,
                    crate::console::command_node_from_spec(&spec, mod_id),
                );
            }
            ScriptMessage::CommandResponse {
                request_id,
                output,
                error,
            } if console
                .pending_invoke
                .as_ref()
                .is_some_and(|pending| pending.request_id == *request_id) =>
            {
                console.handle_command_response(output.clone(), error.clone());
                console.discard_output();
            }
            _ => {}
        }
    }
    fn message(
        &mut self,
        id: String,
        message: ScriptMessage,
        batch_entities: &mut HashMap<String, EntityId>,
    ) {
        if let ScriptMessage::GetMainCamera { request_id } = &message {
            self.script_host.send(
                &id,
                &EngineMessage::MainCameraResponse {
                    request_id: request_id.clone(),
                    entity_id: self
                        .main_camera
                        .filter(|id| self.world.is_alive(id))
                        .map(|id| id.to_string()),
                },
                &mut self.debug,
            );
            return;
        }
        match &message {
            ScriptMessage::RegisterCommand { .. }
            | ScriptMessage::CommandResponse { .. }
            | ScriptMessage::ArgSuggestResponse { .. } => {
                if self.mode == RuntimeMode::Client {
                    self.events.push((id, message));
                    self.changed = true;
                } else {
                    self.server_console_message(&id, &message);
                }
                return;
            }
            _ => {}
        }
        let result = dispatch(
            &id,
            message,
            EngineContext {
                mode: self.mode,
                network: &mut self.network,
                script_host: &mut self.script_host,
                registry: &self.registry,
                game_id: &self.meta.game.id,
                debug: &mut self.debug,
                world: &mut self.world,
                physics: &mut self.physics,
                vfs: self.vfs.as_ref(),
                audio: &mut self.audio,
                batch_entities,
            },
        );
        if let Some(rate) = result.new_tick_rate
            && let Err(error) = self.clock.set_rate(rate)
        {
            tracing::warn!("ignoring invalid tick rate: {error}");
        }
        if let Some(camera) = result.set_main_camera {
            self.main_camera = camera;
        }
        if let Some(fps) = result.new_fps_cap
            && fps.is_none_or(|rate| TickClock::interval_for_rate(rate).is_ok())
        {
            self.client_commands.push(ScriptMessage::SetFpsCap { fps });
        }
        if let Some(enabled) = result.new_vsync {
            self.client_commands
                .push(ScriptMessage::SetVsync { enabled });
        }
        self.render_cmds.extend(result.render_cmds);
        self.client_commands.extend(result.client_commands);
        self.changed = true;
    }
    fn tick(&mut self) {
        self.watchdog.start_tick();
        let batches = self.tick_commands();
        self.finish_tick(batches);
        self.watchdog.end_tick();
    }

    /// Gather external state, then request exactly one buffer from each initialized mod.
    fn tick_commands(&mut self) -> Vec<(String, Vec<ScriptMessage>)> {
        self.initialize_scripts();
        self.tick_number += 1;
        let mut packets = std::mem::take(&mut self.startup_packets);
        packets.extend(self.script_host.drain_messages(&mut self.debug));
        for (id, packet) in packets {
            match packet {
                ScriptPacket::RuntimeReady { protocol } => {
                    if protocol != crate::script::HOST_PROTOCOL {
                        tracing::warn!(mod_id=%id, "incompatible mod host protocol");
                        self.script_host.terminate(&id);
                    }
                }
                ScriptPacket::RuntimeFailed { message } => tracing::error!(mod_id=%id, "{message}"),
                ScriptPacket::CommandBatch { .. } => {
                    tracing::warn!(mod_id=%id, "unsolicited tick batch");
                    self.script_host.terminate(&id);
                }
            }
        }
        // Reap even if no process can accept a Tick: the receive barrier may be empty.
        for id in self.script_host.reap_exited() {
            self.network.remove_mod(&id);
        }
        for command in std::mem::take(&mut self.pending_controls) {
            match command {
                RuntimeCommand::ConsoleInput(input) => self.server_command(&input),
                RuntimeCommand::Ipc { mod_id, message } => {
                    self.script_host.send(&mod_id, &message, &mut self.debug);
                }
                RuntimeCommand::Ready => {}
            }
        }
        self.network_events();
        for (mod_id, audio_id) in self.audio.poll_finished() {
            self.script_host.send(
                &mod_id,
                &EngineMessage::AudioClose { audio_id },
                &mut self.debug,
            );
        }
        let (keys_pressed, mouse_delta) = if let Ok(mut input) = self.shared.input.lock() {
            (input.keys.clone(), std::mem::take(&mut input.mouse))
        } else {
            (vec![], (0.0, 0.0))
        };
        let tick = EngineMessage::Tick {
            tick_number: self.tick_number,
            delta_seconds: self.clock.interval.as_secs_f64(),
            keys_pressed,
            mouse_delta: [mouse_delta.0, mouse_delta.1],
        };
        // Registry iteration is dependency/load order, independent of pipe scheduling.
        let order: Vec<String> = self
            .registry
            .iter()
            .map(|loaded| loaded.manifest.meta.id.clone())
            .filter(|id| self.script_host.is_ready(id))
            .collect();
        let mut pending: HashSet<String> = HashSet::new();
        let mut batches = HashMap::new();
        for id in &order {
            if self.script_host.send(id, &tick, &mut self.debug) {
                pending.insert(id.clone());
            }
        }
        while !pending.is_empty() && !self.stopped() {
            self.controls();
            match self
                .script_host
                .recv_blocking(Duration::from_millis(5), &mut self.debug)
            {
                RecvOutcome::Message(
                    id,
                    ScriptPacket::CommandBatch {
                        tick_number,
                        commands,
                    },
                ) => {
                    if tick_number == self.tick_number && pending.remove(&id) {
                        batches.insert(id, commands);
                    } else {
                        tracing::warn!(mod_id=%id, tick_number, "duplicate or mismatched tick batch");
                        self.script_host.terminate(&id);
                    }
                }
                RecvOutcome::Message(id, packet) => self.startup_packets.push((id, packet)),
                RecvOutcome::Disconnected => break,
                RecvOutcome::Timeout => {}
            }
            for id in self.script_host.reap_exited() {
                pending.remove(&id);
                self.network.remove_mod(&id);
            }
        }
        order
            .into_iter()
            .filter_map(|id| batches.remove(&id).map(|commands| (id, commands)))
            .collect()
    }

    fn initialize_scripts(&mut self) {
        if self.scripts_initialized {
            return;
        }
        let order: Vec<String> = self
            .registry
            .iter()
            .map(|loaded| loaded.manifest.meta.id.clone())
            .collect();
        let mut pending: HashSet<String> = order
            .iter()
            .filter(|id| self.script_host.has_process(id))
            .cloned()
            .collect();
        while !pending.is_empty() && !self.stopped() {
            self.controls();
            match self
                .script_host
                .recv_blocking(Duration::from_millis(5), &mut self.debug)
            {
                RecvOutcome::Message(id, ScriptPacket::RuntimeReady { protocol })
                    if protocol == crate::script::HOST_PROTOCOL && pending.remove(&id) =>
                {
                    self.script_host.mark_ready(&id);
                }
                RecvOutcome::Message(id, ScriptPacket::RuntimeFailed { message }) => {
                    tracing::error!(mod_id=%id, "{message}");
                    self.script_host.terminate(&id);
                    pending.remove(&id);
                }
                RecvOutcome::Message(id, _) => {
                    tracing::warn!(mod_id=%id, "invalid mod startup handshake");
                    self.script_host.terminate(&id);
                    pending.remove(&id);
                }
                RecvOutcome::Disconnected => break,
                RecvOutcome::Timeout => {}
            }
            for id in self.script_host.reap_exited() {
                pending.remove(&id);
            }
        }
        for id in order {
            if self.script_host.is_ready(&id) {
                self.script_host.send(
                    &id,
                    &EngineMessage::Init {
                        mod_id: id.clone(),
                        engine_version: env!("CARGO_PKG_VERSION").to_owned(),
                        mode: self.mode,
                    },
                    &mut self.debug,
                );
            }
        }
        self.scripts_initialized = true;
    }

    /// Execute once, reserve the next phase for physics, then publish all results.
    fn finish_tick(&mut self, batches: Vec<(String, Vec<ScriptMessage>)>) {
        if self.stopped() {
            return;
        }
        self.script_host.begin_responses();
        let tick_delta = self.clock.interval.as_secs_f64();
        self.physics.begin_tick(self.tick_number);
        for (id, commands) in batches {
            let mut batch_entities = HashMap::new();
            for message in commands {
                self.message(id.clone(), message, &mut batch_entities);
            }
        }
        // Fixed simulation order: input/events -> mod batches -> commands -> physics
        // -> presentation and results. Rendering never advances this clock.
        match self.physics.step(&mut self.world, tick_delta) {
            Ok(changed) if self.mode == RuntimeMode::Client && !changed.is_empty() => {
                self.render_cmds
                    .extend(crate::script::RenderCommand::upsert_hierarchy(
                        &self.world,
                        changed,
                    ));
            }
            Ok(_) => {}
            Err(error) => tracing::error!(%error,"physics tick failed"),
        }
        for event in self.physics.take_events() {
            let message = EngineMessage::PhysicsEvent { event };
            for loaded in self.registry.iter() {
                let id = &loaded.manifest.meta.id;
                if self.physics.subscribers.contains(id) {
                    self.script_host.send(id, &message, &mut self.debug);
                }
            }
        }
        self.changed = true;
        self.publish();
        self.script_host
            .flush_responses(self.tick_number, &mut self.debug);
    }
    fn publish(&mut self) {
        if !self.changed {
            return;
        }
        self.changed = false;
        if self.mode == RuntimeMode::Server {
            self.render_cmds.clear();
            self.client_commands.clear();
            return;
        }
        let update = Presentation {
            world: self.world.clone(),
            main_camera: self.main_camera,
            tick_rate: 1.0 / self.clock.interval.as_secs_f64(),
            render_cmds: self
                .render_cmds
                .drain(..)
                .map(|command| (command.key(), command))
                .collect(),
            events: std::mem::take(&mut self.events),
            client_commands: std::mem::take(&mut self.client_commands),
        };
        if let Ok(mut slot) = self.shared.presentation.lock() {
            if let Some(previous) = slot.as_mut() {
                previous.merge(update);
            } else {
                *slot = Some(update);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a built bundled mod host; run in the mod-runtime CI job"]
    fn client_ticks_without_redraws_or_audio_hardware() {
        use crate::{debug::DebugConfig, runtime::LaunchOptions};
        let directory = std::env::temp_dir().join(format!(
            "whatever-client-worker-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(directory.join("core/assets")).expect("fixture");
        std::fs::write(
            directory.join("core/meta.toml"),
            "[game]\nid='client_worker_fixture'\nname='Fixture'\ntick_rate=100\n",
        )
        .expect("metadata");
        std::fs::write(
            directory.join("core/mod.toml"),
            "[mod]\nid='core'\nname='Core'\nversion='1.0.0'\n[script]\nentry='index.ts'\n",
        )
        .expect("manifest");
        std::fs::write(directory.join("core/index.ts"),r#"
import {Engine,Scene} from "@whatever-engine/api";
let entity;
Engine.setFpsCap(1);
Engine.on("init",async()=>{
    entity=await Scene.createEntity().get();
    entity.setComponent("core:sprite_renderer",{texture:"core://unused.png",shader:"core://unused.wgsl"});
});
Engine.on("tick",({tick_number})=>{
    if(entity)entity.setComponent("core:transform",{position:[tick_number,0,0],rotation:[0,0,0,1],scale:[1,1,1]});
});
"#).expect("script");
        let log = std::fs::File::create(directory.join("test.log")).expect("log");
        let game = LoadedGame::load(
            &LaunchOptions {
                mode: Some(RuntimeMode::Client),
                mods: Some(directory.clone()),
                ..Default::default()
            },
            &DebugConfig::from_args(&[]),
            &directory,
            Arc::new(std::sync::Mutex::new(std::io::BufWriter::new(log))),
        )
        .expect("game");
        let shared = Arc::new(Shared {
            input: Default::default(),
            presentation: Default::default(),
            stop: std::sync::atomic::AtomicBool::new(false),
        });
        struct Stop(Arc<Shared>);
        impl Drop for Stop {
            fn drop(&mut self) {
                self.0.stop.store(true, Ordering::Relaxed);
            }
        }
        let _stop = Stop(Arc::clone(&shared));
        let (tx, rx) = mpsc::channel();
        let worker_shared = Arc::clone(&shared);
        let worker = std::thread::spawn(move || {
            Simulation::with_audio(game, worker_shared, rx, AudioManager::headless())?.run()
        });
        tx.send(RuntimeCommand::Ready).expect("ready");
        // Leave the presentation mailbox unread: there is no renderer or redraw loop.
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut advanced = false;
        while Instant::now() < deadline {
            if let Ok(slot) = shared.presentation.lock() {
                advanced = slot
                    .as_ref()
                    .and_then(|frame| frame.world.transforms.get(&0))
                    .is_some_and(|transform| transform.position[0] >= 10.0);
            }
            if advanced || worker.is_finished() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        shared.stop.store(true, Ordering::Relaxed);
        worker
            .join()
            .expect("worker did not panic")
            .expect("runtime completed");
        assert!(
            advanced,
            "client simulation must advance with a 1 FPS cap and zero redraws"
        );
        let frame = shared
            .presentation
            .lock()
            .expect("mailbox")
            .take()
            .expect("snapshot");
        assert_eq!(
            frame.render_cmds.len(),
            1,
            "render updates should coalesce while frames are paused"
        );
        assert!(
            frame
                .client_commands
                .iter()
                .any(|command| matches!(command, ScriptMessage::SetFpsCap { fps: Some(1.0) }))
        );
        std::fs::remove_dir_all(&directory).expect("cleanup fixture");
    }
}
