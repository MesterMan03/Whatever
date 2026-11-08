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
    script::{
        EngineContext, ScriptHost, dispatch,
        ipc::{EngineMessage, ScriptMessage},
    },
    vfs::VfsHandle,
    watchdog::Watchdog,
};
use anyhow::Context;
use std::{
    collections::HashMap,
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
    command_count: usize,
    #[cfg(test)]
    script_messages: Vec<(String, ScriptMessage)>,
    timings: super::TickTimings,
    initial_scene: Option<crate::scene::LoadedScene>,
    physics: crate::physics::PhysicsRegistry,
    audio: AudioManager,
    clock: TickClock,
    tick_meter: crate::presentation::RateMeter,
    tick_number: u64,
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
            debug,
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
            let mod_id = &loaded_mod.manifest.meta.id;
            let package = crate::script::compile_sources(&loaded_mod.root, mod_id)?;
            script_host.register(mod_id, package, script_entry, mode)?;
        }
        let mut world = World::new();
        for (owner, script) in &script_host.scripts {
            for definition in &script.package.definitions {
                world.register_component(owner, definition.clone())?;
            }
        }

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
            command_count: 0,
            #[cfg(test)]
            script_messages: Vec::new(),
            timings: Default::default(),
            initial_scene,
            physics,
            audio,
            clock,
            tick_meter: Default::default(),
            tick_number: 0,
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
        let order = self
            .registry
            .iter()
            .map(|loaded| loaded.manifest.meta.id.clone())
            .collect::<Vec<_>>();
        for id in order {
            if self.script_host.contains_mod(&id) {
                self.script_host.send(
                    &id,
                    &EngineMessage::Shutdown { exit_code: 0 },
                    &mut self.debug,
                );
                if let Err(error) = self.run_script(&id, false) {
                    tracing::warn!(mod_id=%id,%error,"mod shutdown callback failed");
                }
            }
        }
        self.script_host.shutdown_all(0, &mut self.debug);
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
                        Some("command's mod is unavailable".into()),
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
        #[cfg(test)]
        self.script_messages.push((id.clone(), message.clone()));
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
        self.command_count = 0;
        let tick_delta = self.tick_commands();
        self.timings = super::TickTimings {
            tick_number: self.tick_number,
            tick_rate: 1.0 / self.clock.interval.as_secs_f64(),
            scripts_ms: super::performance::elapsed(start),
            commands: self.command_count,
            hosts: self.script_host.scripts.len(),
            build: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
            .into(),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            ..Default::default()
        };
        self.finish_tick(tick_delta);
        self.timings.total_ms = super::performance::elapsed(start);
        self.timings.entities = self.world.allocator.alive_entity_ids().count();
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

    /// Gather external state, then run each mod directly in dependency order.
    fn tick_commands(&mut self) -> f64 {
        self.initialize_scripts();
        self.tick_number += 1;
        self.physics.begin_tick(self.tick_number);
        let tick_delta = self.clock.interval.as_secs_f64();
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
        let (keys_pressed, mouse_delta, mouse_captured, mouse_buttons, mouse_buttons_pressed) =
            if let Ok(mut input) = self.shared.input.lock() {
                input.take_tick()
            } else {
                (vec![], (0.0, 0.0), false, vec![], vec![])
            };
        let tick = EngineMessage::Tick {
            tick_number: self.tick_number,
            delta_seconds: tick_delta,
            keys_pressed,
            mouse_delta: [mouse_delta.0, mouse_delta.1],
            mouse_captured,
            mouse_buttons,
            mouse_buttons_pressed,
        };
        // The registry supplies dependency/load order.
        let order: Vec<String> = self
            .registry
            .iter()
            .map(|loaded| loaded.manifest.meta.id.clone())
            .filter(|id| self.script_host.is_ready(id))
            .collect();
        for id in &order {
            self.script_host.send(id, &tick, &mut self.debug);
            if let Err(error) = self.run_script(id, false) {
                tracing::error!(mod_id=%id,error=%format!("{error:#}"),"mod execution failed");
                self.script_host.terminate(id);
            }
        }
        tick_delta
    }

    fn initialize_scripts(&mut self) {
        if self.scripts_initialized {
            return;
        }
        let order = self
            .registry
            .iter()
            .map(|loaded| loaded.manifest.meta.id.clone())
            .collect::<Vec<_>>();
        for id in order {
            if !self.script_host.contains_mod(&id) {
                continue;
            }
            let result = self.run_script(&id, true);
            if let Err(error) = result {
                tracing::error!(mod_id=%id,%error,"mod initialization failed");
                self.script_host.terminate(&id);
                continue;
            }
            self.script_host.send(
                &id,
                &EngineMessage::Init {
                    initial_scene: self.initial_scene.clone(),
                    mod_id: id.clone(),
                    engine_version: env!("CARGO_PKG_VERSION").into(),
                    mode: self.mode,
                },
                &mut self.debug,
            );
            if let Err(error) = self.run_script(&id, false) {
                tracing::error!(mod_id=%id,%error,"mod init callback failed");
                self.script_host.terminate(&id);
            }
        }
        self.scripts_initialized = true;
    }

    fn run_script(&mut self, id: &str, boot: bool) -> anyhow::Result<()> {
        let package = if boot {
            let script = self.script_host.scripts.get(id).context("mod sources")?;
            Some(serde_json::to_string(
                &serde_json::json!({"modules":script.package.modules,"definitions":script.package.definitions,"entry":script.entry,"mode":script.mode.to_string()}),
            )?)
        } else {
            None
        };
        let mut runtime = self
            .script_host
            .runtime
            .take()
            .context("runtime already executing")?;
        let messages = self.script_host.take_messages(id);
        let mut aliases = HashMap::new();
        let mut handler = |mut request: serde_json::Value| -> anyhow::Result<serde_json::Value> {
            if request["op"] == "command" {
                let message: ScriptMessage = serde_json::from_value(request["message"].take())?;
                self.command_count += 1;
                self.message(id.into(), message, &mut aliases);
                return Ok(serde_json::to_value(self.script_host.take_messages(id))?);
            }
            self.script_query(&request)
        };
        let result = (|| -> anyhow::Result<()> {
            if let Some(package) = package {
                runtime.scoped(id, "load_package", &package, &mut handler)?;
                runtime.scoped(id,"evaluate","if(globalThis.__bootError)throw new Error(__bootError); if(!globalThis.__ready)throw new Error('Module initialization did not complete'); null",&mut handler)?;
            }
            for message in messages {
                runtime.scoped(
                    id,
                    "evaluate",
                    &format!("__deliver({});null", serde_json::to_string(&message)?),
                    &mut handler,
                )?;
            }
            Ok(())
        })();
        self.script_host.runtime = Some(runtime);
        if result.is_ok()
            && boot
            && let Some(script) = self.script_host.scripts.get_mut(id)
        {
            script.booted = true;
        }
        result
    }

    fn script_query(&mut self, request: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
        use serde_json::json;
        match request["op"].as_str().context("missing host operation")? {
            "component_row" => {
                let Some(id) = request["id"].as_str().and_then(EntityId::parse) else {
                    return Ok(serde_json::Value::Null);
                };
                let kind = request["kind"].as_str().context("invalid component type")?;
                Ok(self.world.get_component(&id, kind).map_or(
                    serde_json::Value::Null,
                    |data| json!({
                        "data": data,
                        "instance_id": self.world.component_instances.get(kind)
                            .and_then(|instances| instances.get(&id.index)).map(ToString::to_string),
                    }),
                ))
            }
            "component_patch" => {
                let id = request["id"]
                    .as_str()
                    .and_then(EntityId::parse)
                    .context("invalid entity handle")?;
                let kind = request["kind"].as_str().context("invalid component type")?;
                anyhow::ensure!(
                    !crate::physics::PhysicsRegistry::is_read_only(kind),
                    "component is read-only"
                );
                let mut data = self
                    .world
                    .get_component(&id, kind)
                    .context("component handle is stale")?;
                let fields = data.as_object_mut().context("invalid component data")?;
                for (key, value) in request["data"]
                    .as_object()
                    .context("invalid component patch")?
                {
                    fields.insert(key.clone(), value.clone());
                }
                crate::ecs::resolve_references(&mut data, &mut |reference| {
                    EntityId::parse(reference).filter(|id| self.world.is_alive(id))
                })?;
                let data = self
                    .physics
                    .normalize_component(&mut self.world, id, kind, data)?;
                anyhow::ensure!(
                    self.world.set_component(&id, kind, data),
                    "component update was rejected"
                );
                self.physics.component_changed(&self.world, id, kind);
                if matches!(
                    kind,
                    "core:transform"
                        | "core:sprite_renderer"
                        | "core:text_renderer"
                        | "core:mesh_renderer"
                ) {
                    self.render_cmds
                        .extend(RenderCommand::upsert_hierarchy(&self.world, [id]));
                }
                self.changed = true;
                Ok(serde_json::Value::Null)
            }
            "query_entities" => {
                let values = request["kinds"]
                    .as_array()
                    .context("invalid component query")?;
                anyhow::ensure!(values.len() <= 64, "component query exceeds limit");
                let kinds = values
                    .iter()
                    .map(|value| value.as_str().context("invalid component type"))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                Ok(json!(
                    self.world
                        .query(&kinds)
                        .into_iter()
                        .map(|(id, _)| id.to_string())
                        .collect::<Vec<_>>()
                ))
            }
            "query_tags" => {
                let query = &request["query"];
                let filters = |field: &str| -> anyhow::Result<Vec<String>> {
                    match query.get(field) {
                        None => Ok(Vec::new()),
                        Some(value) => {
                            let values = value.as_array().context("invalid tag filter")?;
                            anyhow::ensure!(values.len() <= 32, "tag filter exceeds limit");
                            values
                                .iter()
                                .map(|value| {
                                    value.as_str().map(str::to_owned).context("invalid tag")
                                })
                                .collect()
                        }
                    }
                };
                let all = filters("all")?;
                let any = filters("any")?;
                let none = filters("none")?;
                Ok(json!(
                    self.world
                        .allocator
                        .alive_entity_ids()
                        .filter(|id| {
                            if query["includeDisabled"] != true && !self.world.is_active(id) {
                                return false;
                            }
                            let tags = self.world.tags.get(&id.index);
                            let has = |tag: &String| tags.is_some_and(|tags| tags.contains(tag));
                            all.iter().all(has)
                                && (any.is_empty() || any.iter().any(has))
                                && !none.iter().any(has)
                        })
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>()
                ))
            }
            "entity_record" => Ok(serde_json::to_value(
                request["id"]
                    .as_str()
                    .and_then(EntityId::parse)
                    .and_then(|id| crate::ecs::entity_snapshot(&self.world, id)),
            )?),
            "entity_metadata" => Ok(serde_json::to_value(
                request["id"]
                    .as_str()
                    .and_then(EntityId::parse)
                    .and_then(|id| crate::ecs::entity_metadata(&self.world, id)),
            )?),
            "entity_list" => Ok(json!(
                self.world
                    .allocator
                    .alive_entity_ids()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
            )),
            "component_rows" => {
                #[derive(serde::Serialize)]
                struct Row<'a> {
                    entity_id: String,
                    component_type: &'a str,
                    instance_id: Option<String>,
                    data: &'a serde_json::Value,
                    active: bool,
                }

                let kinds = request["kinds"]
                    .as_array()
                    .context("expected component types")?;
                anyhow::ensure!(kinds.len() <= 10000, "too many component types");
                let mut rows = Vec::new();
                for kind in kinds {
                    let kind = kind.as_str().context("invalid component type")?;
                    let Some(components) = self.world.custom.get(kind) else {
                        continue;
                    };
                    let instances = self.world.component_instances.get(kind);
                    let mut entries: Vec<_> = components.iter().collect();
                    entries.sort_unstable_by_key(|(index, _)| **index);
                    rows.extend(entries.into_iter().filter_map(|(&index, data)| {
                        let id = self.world.allocator.live_id(index)?;
                        self.world.is_active(&id).then(|| Row {
                            entity_id: id.to_string(),
                            component_type: kind,
                            instance_id: instances
                                .and_then(|instances| instances.get(&index))
                                .map(ToString::to_string),
                            data,
                            active: true,
                        })
                    }));
                }
                Ok(serde_json::to_value(rows)?)
            }
            "scene_info" => Ok(
                json!({"revision":self.tick_number,"camera":self.main_camera.map(|id|id.to_string())}),
            ),
            _ => anyhow::bail!("unsupported engine operation"),
        }
    }

    /// Execute once, reserve the next phase for physics, then publish all results.
    fn finish_tick(&mut self, tick_delta: f64) {
        if self.stopped() {
            return;
        }
        let command_start = Instant::now();
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
        // Input/events -> synchronous scripts and editor commands -> physics -> presentation.
        // Rendering never advances this clock.
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
        let presentation_start = Instant::now();
        self.publish();
        self.world.scene_changes = Default::default();
        self.timings.presentation_ms = super::performance::elapsed(presentation_start);
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
        let now = Instant::now();
        self.motion.capture(&self.world, self.main_camera, now);
        let update = Presentation {
            motion: self.motion.clone(),
            world: self.world.clone(),
            main_camera: self.main_camera,
            ticks_per_second: self.tick_meter.record(now),
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
    #[ignore = "runs the complete embedded client runtime; run in the mod-runtime CI job"]
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
        // Asset validation still runs even though this fixture never renders them.
        std::fs::write(directory.join("core/assets/unused.png"), b"fixture").expect("texture");
        std::fs::write(directory.join("core/assets/unused.wgsl"), b"// fixture").expect("shader");
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
            "console scene load must publish world and response without redraws: {:?}",
            shared
                .presentation
                .lock()
                .expect("snapshot")
                .as_ref()
                .map(|frame| &frame.events)
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
