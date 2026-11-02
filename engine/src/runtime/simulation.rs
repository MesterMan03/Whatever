use super::{
    LoadedGame, RuntimeMode,
    bridge::{Presentation, RuntimeCommand, Shared},
    scheduler::TickClock,
};
use crate::script::RenderCommand;
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
use anyhow::Context;
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
    console: crate::console::DevConsole,
    fatal_error: Option<String>,
    world: World,
    scene_publisher: crate::ecs::ScenePublisher,
    timings: super::TickTimings,
    initial_scene: Option<crate::scene::LoadedScene>,
    physics: crate::physics::PhysicsRegistry,
    audio: AudioManager,
    clock: TickClock,
    tick_number: u64,
    startup_packets: Vec<(String, ScriptPacket)>,
    scripts_initialized: bool,
    pending_controls: Vec<RuntimeCommand>,
    editor_edits: Vec<super::editor::EditorControl>,
    editor_sequence: u64,
    editor_published: Instant,
    ready: bool,
    watchdog: Watchdog,
    shared: Arc<Shared>,
    commands: mpsc::Receiver<RuntimeCommand>,
    render_cmds: Vec<crate::script::RenderCommand>,
    events: Vec<(String, ScriptMessage)>,
    client_commands: Vec<ScriptMessage>,
    main_camera: Option<EntityId>,
    changed: bool,
    motion: crate::presentation::MotionHistory,
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

        let mut world = World::new();
        crate::script::load_component_types(&mut world, &registry)?;
        let initial_scene = if let Some(path) = registry.startup_scene()? {
            Some(
                crate::scene::SceneDocument::load_checked(
                    vfs.as_ref(),
                    path,
                    &mut world,
                    &registry,
                )
                .context("preload core initial_scene")?,
            )
        } else {
            None
        };
        let render_cmds =
            RenderCommand::upsert_hierarchy(&world, world.allocator.alive_entity_ids());
        let main_camera = world
            .allocator
            .alive_entity_ids()
            .find(|id| world.is_active(id) && world.camera_components.contains_key(&id.index));
        let mut physics = crate::physics::PhysicsRegistry::default();
        physics.invalidate();
        Ok(Self {
            mode,
            meta,
            registry,
            vfs,
            debug,
            script_host,
            network: crate::network::NetworkRegistry::new(network),
            console: crate::console::DevConsole::new(),
            fatal_error: None,
            world,
            scene_publisher: Default::default(),
            timings: Default::default(),
            initial_scene,
            physics,
            audio,
            clock,
            tick_number: 0,
            startup_packets: vec![],
            scripts_initialized: false,
            pending_controls: vec![],
            editor_edits: vec![],
            editor_sequence: 0,
            editor_published: Instant::now() - Duration::from_secs(1),
            ready: false,
            watchdog: Watchdog::new(Duration::from_millis(500)),
            shared,
            commands,
            render_cmds,
            events: vec![],
            client_commands: vec![],
            main_camera,
            changed: true,
            motion: Default::default(),
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
        self.fatal_error.is_some() || self.shared.stop.load(Ordering::Relaxed)
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
                RuntimeCommand::ConsoleInput(input) if self.console.is_shutdown_command(&input) => {
                    self.shared.stop.store(true, Ordering::Relaxed);
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
        if let Some(error) = self.fatal_error {
            anyhow::bail!(error);
        }
        Ok(())
    }
    fn server_command(&mut self, input: &str) {
        let console = &mut self.console;
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
            crate::console::ConsoleAction::LoadScene { path, request_id } => {
                self.load_scene(&path, request_id);
            }
            crate::console::ConsoleAction::SetFpsCap(_)
            | crate::console::ConsoleAction::SetVsync(_) => {
                tracing::warn!("presentation settings are unavailable on a headless server")
            }
            crate::console::ConsoleAction::None => {}
        }
    }
    fn load_scene(&mut self, path: &str, request_id: String) {
        let (output, error) = match crate::scene::SceneDocument::load_checked(
            self.vfs.as_ref(),
            path,
            &mut self.world,
            &self.registry,
        ) {
            Ok(scene) => {
                self.physics.invalidate();
                self.render_cmds.extend(RenderCommand::upsert_hierarchy(
                    &self.world,
                    scene.entity_ids.iter().filter_map(|id| EntityId::parse(id)),
                ));
                (
                    vec![format!(
                        "Loaded {} entities in {:.2} ms",
                        scene.entity_ids.len(),
                        scene.load_time
                    )],
                    None,
                )
            }
            Err(error) => (vec![], Some(format!("{error:#}"))),
        };
        let response = ScriptMessage::CommandResponse {
            request_id,
            output,
            error,
        };
        if self.mode == RuntimeMode::Client {
            self.events.push(("core".into(), response));
        } else {
            self.console_message("core", &response);
        }
        self.changed = true;
    }

    fn console_message(&mut self, mod_id: &str, message: &ScriptMessage) {
        let console = &mut self.console;
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
                if let Err(error) = console.registry.register_mod(
                    mod_id,
                    crate::console::command_node_from_spec(&spec, mod_id),
                ) {
                    tracing::error!("command registration: {error}");
                    self.fatal_error = Some(error);
                }
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
                // Validate registrations on the simulation worker in both modes.
                self.console_message(&id, &message);
                if self.stopped() {
                    return;
                }
                if self.mode == RuntimeMode::Client {
                    self.events.push((id, message));
                    self.changed = true;
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
        let start = Instant::now();
        let batches = self.tick_commands();
        self.timings = super::TickTimings {
            tick_number: self.tick_number,
            tick_rate: 1.0 / self.clock.interval.as_secs_f64(),
            scripts_ms: super::performance::elapsed(start),
            commands: batches.iter().map(|(_, commands)| commands.len()).sum(),
            hosts: batches.len(),
            build: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
            .into(),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            ..Default::default()
        };
        self.finish_tick(batches);
        self.timings.total_ms = super::performance::elapsed(start);
        self.timings.entities = self.scene_publisher.entity_count();
        self.timings.frame = self
            .shared
            .frame_timings
            .lock()
            .ok()
            .and_then(|frame| frame.clone());
        for loaded in self.registry.iter() {
            self.script_host.send(
                &loaded.manifest.meta.id,
                &EngineMessage::Performance {
                    sample: Box::new(self.timings.clone()),
                },
                &mut self.debug,
            );
        }
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
                RuntimeCommand::LoadScene { path, request_id } => {
                    self.load_scene(&path, request_id);
                }
                RuntimeCommand::Ipc { mod_id, message } => {
                    self.script_host.send(&mod_id, &message, &mut self.debug);
                }
                RuntimeCommand::Editor(edit) => self.editor_edits.push(edit),
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
        let (keys_pressed, mouse_delta, mouse_captured) =
            if let Ok(mut input) = self.shared.input.lock() {
                input.take_tick()
            } else {
                (vec![], (0.0, 0.0), false)
            };
        let tick = EngineMessage::Tick {
            tick_number: self.tick_number,
            delta_seconds: self.clock.interval.as_secs_f64(),
            keys_pressed,
            mouse_delta: [mouse_delta.0, mouse_delta.1],
            mouse_captured,
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
        let snapshot = self
            .scene_publisher
            .reset(&mut self.world, 0, self.main_camera);
        if let Err(error) = self.script_host.broadcast_scene(snapshot, &mut self.debug) {
            tracing::error!(%error, "initial scene snapshot failed");
        }
        for id in order {
            if self.script_host.is_ready(&id) {
                self.script_host.send(
                    &id,
                    &EngineMessage::Init {
                        initial_scene: self.initial_scene.clone(),
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
        let command_start = Instant::now();
        let tick_delta = self.clock.interval.as_secs_f64();
        self.physics.begin_tick(self.tick_number);
        for (id, commands) in batches {
            let mut batch_entities = HashMap::new();
            for message in commands {
                self.message(id.clone(), message, &mut batch_entities);
                if self.stopped() {
                    return;
                }
            }
        }
        for edit in std::mem::take(&mut self.editor_edits) {
            let removals = RenderCommand::remove_hierarchy(
                &self.world,
                self.world.allocator.alive_entity_ids(),
            );
            self.editor_sequence = edit.sequence;
            let result = super::editor::apply_editor_batch(&mut self.world, &edit.batch);
            if result.is_ok() {
                self.physics.invalidate();
                self.render_cmds.extend(removals);
                self.render_cmds.extend(RenderCommand::upsert_hierarchy(
                    &self.world,
                    self.world.allocator.alive_entity_ids(),
                ));
            }
            let reply = match result {
                Ok(created) => super::editor::EditorReply {
                    sequence: edit.sequence,
                    created,
                    error: None,
                },
                Err(error) => super::editor::EditorReply {
                    sequence: edit.sequence,
                    created: Default::default(),
                    error: Some(format!("{error:#}")),
                },
            };
            let _ = edit.reply.try_send(reply);
        }
        self.timings.commands_ms = super::performance::elapsed(command_start);
        let physics_start = Instant::now();
        // Fixed simulation order: input/events -> mod batches -> commands -> physics
        // -> presentation and results. Rendering never advances this clock.
        match self.physics.step(&mut self.world, tick_delta) {
            Ok(changed) => {
                for id in &changed {
                    self.world.mark_component(*id, "core:transform");
                    self.world.mark_hierarchy(*id);
                }
                if self.mode == RuntimeMode::Client {
                    self.render_cmds
                        .extend(crate::script::RenderCommand::upsert_hierarchy(
                            &self.world,
                            changed,
                        ));
                }
            }
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
        self.timings.physics_ms = super::performance::elapsed(physics_start);
        self.changed = true;
        let snapshot_start = Instant::now();
        let delta =
            self.scene_publisher
                .publish(&mut self.world, self.tick_number, self.main_camera);
        match self.script_host.broadcast_scene(delta, &mut self.debug) {
            Ok(bytes) => self.timings.snapshot_bytes = bytes,
            Err(error) => tracing::error!(%error, "scene delta publication failed"),
        }
        self.timings.snapshot_ms = super::performance::elapsed(snapshot_start);
        let presentation_start = Instant::now();
        self.publish();
        self.timings.presentation_ms = super::performance::elapsed(presentation_start);
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
        if self.editor_published.elapsed() >= Duration::from_millis(33)
            && let Ok(mailbox) = self.shared.editor_snapshot.lock()
            && let Some(mailbox) = mailbox.as_ref().and_then(std::sync::Weak::upgrade)
            && let Ok(mut slot) = mailbox.lock()
        {
            self.editor_published = Instant::now();
            *slot = Some(super::editor_transport::EditorSnapshot {
                world: self.world.clone(),
                camera: self.main_camera,
                sequence: self.editor_sequence,
            });
        }
        self.motion
            .capture(&self.world, self.main_camera, Instant::now());
        let update = Presentation {
            motion: self.motion.clone(),
            world: self.world.clone(),
            main_camera: self.main_camera,
            tick_rate: 1.0 / self.clock.interval.as_secs_f64(),
            editor_sequence: self.editor_sequence,
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
    #[ignore = "requires the bundled host; run in editor/runtime CI"]
    fn editor_controls_apply_before_physics_and_publish_live_values() {
        use crate::{debug::DebugConfig, runtime::LaunchOptions};
        use serde_json::json;
        let directory = std::env::temp_dir().join(format!(
            "whatever-editor-tick-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(directory.join("core")).expect("fixture");
        std::fs::write(
            directory.join("core/mod.toml"),
            "[mod]\nid='core'\nname='Core'\nversion='1.0.0'\n[assets]\nroot='.'\n",
        )
        .expect("manifest");
        std::fs::write(
            directory.join("core/meta.toml"),
            "[game]\nid='editor_tick_fixture'\nname='Editor Fixture'\ntick_rate=60\n",
        )
        .expect("meta");
        let log = std::fs::File::create(directory.join("fixture.log")).expect("log");
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
            editor_snapshot: Default::default(),
            frame_timings: Default::default(),
        });
        let (tx, rx) = mpsc::channel();
        let mut simulation =
            Simulation::with_audio(game, Arc::clone(&shared), rx, AudioManager::headless())
                .expect("simulation");
        let id = simulation.world.create_entity();
        simulation.world.set_component(
            &id,
            "core:transform",
            json!({"position":[0,10,0],"rotation":[0,0,0,1],"scale":[1,1,1]}),
        );
        for (kind, value) in [
            ("core:rigid_body", json!({"type":"dynamic"})),
            (
                "core:collider",
                json!({"shape":{"type":"box","size":[1,1,1]}}),
            ),
        ] {
            let value = simulation
                .physics
                .normalize_component(&mut simulation.world, id, kind, value)
                .expect("component");
            assert!(simulation.world.set_component(&id, kind, value));
        }
        let (reply, answer) = mpsc::sync_channel(1);
        tx.send(RuntimeCommand::Editor(
            super::super::editor::EditorControl {
                sequence: 7,
                batch: super::super::editor::EditorBatch {
                    edits: vec![super::super::editor::EntityEdit {
                        id: id.to_string(),
                        name: Some("Live body".into()),
                        enabled: None,
                        tags: None,
                        components: std::collections::BTreeMap::from([(
                            "core:transform".into(),
                            vec![super::super::editor::ValueEdit {
                                path: vec!["position".into(), "1".into()],
                                value: Some(json!(7)),
                            }],
                        )]),
                    }],
                    ..Default::default()
                },
                reply,
            },
        ))
        .expect("request");
        tx.send(RuntimeCommand::Ready).expect("ready");
        simulation.controls();
        simulation.tick();
        assert!(
            answer
                .recv_timeout(Duration::from_secs(1))
                .expect("reply")
                .error
                .is_none()
        );
        let update = shared
            .presentation
            .lock()
            .expect("snapshot")
            .take()
            .expect("published");
        assert_eq!(update.world.names[&id.index], "Live body");
        let y = update.world.transforms[&id.index].position[1];
        assert!(
            y < 7.0 && y > 6.9,
            "physics should step the edited pose before publication: {y}"
        );
        assert!(update.world.rigid_body_states.contains_key(&id.index));
        drop(simulation);
        std::fs::remove_dir_all(directory).expect("cleanup");
    }
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
import {Engine,Scene,BuiltInComponents} from "@whatever-engine/api";
let entity;
Engine.setFpsCap(1);
Engine.on("init",async()=>{
    entity=await Scene.createEntity().get();
    entity.setComponent(new BuiltInComponents.SpriteRenderer({texture:"core://unused.png",shader:"core://unused.wgsl"}));
});
Engine.on("tick",({tick_number})=>{
    if(entity)entity.setComponent(new BuiltInComponents.Transform({position:[tick_number,0,0],rotation:[0,0,0,1],scale:[1,1,1]}));
});
"#).expect("script");
        std::fs::create_dir_all(directory.join("core/scenes")).expect("scenes");
        std::fs::write(
            directory.join("core/scenes/console.wscene"),
            r#"{
            "version":1,"entities":[{"name":"Console spawn","components":{
                "core:transform":{"position":[1,2,3],"rotation":[0,0,0,1],"scale":[1,1,1]},
                "core:sprite_renderer":{"texture":"core://unused.png","shader":"core://unused.wgsl"}
            }}]
        }"#,
        )
        .expect("console scene");
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
            editor_snapshot: Default::default(),
            frame_timings: Default::default(),
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
        tx.send(RuntimeCommand::LoadScene {
            path: "core://console".into(),
            request_id: "console_load_test".into(),
        })
        .expect("load scene");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut loaded = false;
        while Instant::now() < deadline {
            if let Ok(slot) = shared.presentation.lock() {
                loaded = slot.as_ref().is_some_and(|frame| {
                    frame
                        .world
                        .names
                        .values()
                        .any(|name| name == "Console spawn")
                        && frame.events.iter().any(|(_, message)| {
                            matches!(message,
                            ScriptMessage::CommandResponse {request_id, error: None, ..}
                            if request_id == "console_load_test")
                        })
                });
            }
            if loaded || worker.is_finished() {
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
        assert!(
            loaded,
            "console scene load must publish world and response without redraws"
        );
        let frame = shared
            .presentation
            .lock()
            .expect("mailbox")
            .take()
            .expect("snapshot");
        assert_eq!(
            frame.render_cmds.len(),
            2,
            "render updates should coalesce while frames are paused"
        );
        assert!(
            frame
                .client_commands
                .iter()
                .any(|command| matches!(command, ScriptMessage::SetFpsCap { fps: Some(1.0) }))
        );
        assert_eq!(frame.world.transforms[&1].position, [1.0, 2.0, 3.0]);

        // Registration validation belongs to the worker, even if no client redraws
        // ever consume the registry events.
        std::fs::write(
            directory.join("core/index.ts"),
            r#"
            import {Console} from "@whatever-engine/api";
            Console.register({name:"scene",handler:()=>"must not run"});
        "#,
        )
        .expect("colliding script");
        let log = std::fs::File::create(directory.join("collision.log")).expect("log");
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
        .expect("collision game");
        shared.stop.store(false, Ordering::Relaxed);
        let (_tx, rx) = mpsc::channel();
        let mut simulation =
            Simulation::with_audio(game, Arc::clone(&shared), rx, AudioManager::headless())
                .expect("client simulation");
        simulation.tick();
        assert!(simulation.stopped());
        assert!(
            simulation
                .fatal_error
                .as_deref()
                .is_some_and(|error| error.contains("core:scene"))
        );
        assert!(
            simulation
                .run()
                .expect_err("fatal registration")
                .to_string()
                .contains("core:scene")
        );
        std::fs::remove_dir_all(&directory).expect("cleanup fixture");
    }
}
