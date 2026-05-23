use crate::console::commands::{debug_cmd, engine_cmd, markbench, mods_cmd, scene_cmd, vfs_cmd};
use crate::console::completer;
use crate::console::parser;
use crate::console::registry::CommandRegistry;
use crate::console::types::{CommandContext, CommandNode, CommandSource, OutputLine};
use crate::mods::ModRegistry;
use crate::script::ipc::EngineMessage;
use crate::vfs::Vfs;
use egui::{Color32, Context, FontId, Key, Modifiers, RichText, ScrollArea, TextEdit};
use std::sync::atomic::{AtomicU64, Ordering};

const INPUT_ROW_HEIGHT: f32 = 26.0;
const MAX_COMPLETIONS: usize = 8;

static SUGGEST_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct DevConsole {
    pub is_open: bool,
    input_buf: String,
    output: Vec<OutputLine>,
    history: Vec<String>,
    history_pos: Option<usize>,
    pub registry: CommandRegistry,
    pub fps: f32,
    pub fps_cap: Option<f64>,
    pub vsync: bool,
    completions: Vec<String>,
    completion_idx: usize,
    pub pending_invoke: Option<PendingInvoke>,
    needs_focus: bool,
    /// request_id of the most recently sent ArgSuggestRequest; used to discard stale responses.
    pending_suggest_id: Option<String>,
    /// Input prefix at the time the suggest request was sent; prepended to each suggestion to form
    /// a full replacement string when the response arrives.
    pending_suggest_prefix: String,
    /// IPC to fire this frame (set when the input changes and a mod arg suggest is needed).
    pending_suggest_ipc: Option<(String, EngineMessage)>,
}

pub struct PendingInvoke {
    pub request_id: String,
}

pub enum ConsoleAction {
    None,
    Quit,
    SendIpc {
        mod_id: String,
        message: EngineMessage,
    },
    EngineSettings(crate::console::types::EngineSettingAction),
}

impl DevConsole {
    pub fn new() -> Self {
        let mut registry = CommandRegistry::new();
        registry.register_engine(debug_cmd::node());
        registry.register_engine(engine_cmd::node());
        registry.register_engine(markbench::node());
        registry.register_engine(mods_cmd::node());
        registry.register_engine(scene_cmd::node());
        registry.register_engine(vfs_cmd::node());

        DevConsole {
            is_open: false,
            input_buf: String::new(),
            output: vec![
                OutputLine::Text("Whatever Engine Developer Console".into()),
                OutputLine::Text("Type 'help' for a list of commands.".into()),
            ],
            history: Vec::new(),
            history_pos: None,
            registry,
            fps: 0.0,
            fps_cap: None,
            vsync: true,
            completions: Vec::new(),
            completion_idx: 0,
            pending_invoke: None,
            needs_focus: false,
            pending_suggest_id: None,
            pending_suggest_prefix: String::new(),
            pending_suggest_ipc: None,
        }
    }

    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
        if self.is_open {
            self.needs_focus = true;
        } else {
            self.completions.clear();
        }
    }

    /// Escape key: clear completions first; close the console only when already clear.
    pub fn escape(&mut self) {
        if !self.completions.is_empty() {
            self.completions.clear();
            self.completion_idx = 0;
        } else {
            self.toggle();
        }
    }

    pub fn push_debug_line(&mut self, msg: String) {
        self.output.push(OutputLine::Debug(msg));
    }

    pub fn push_log_line(&mut self, level: &str, msg: String) {
        match level {
            "ERROR" => self
                .output
                .push(OutputLine::Error(format!("[ERROR] {msg}"))),
            "WARN" => self.output.push(OutputLine::Warn(format!("[WARN] {msg}"))),
            _ => self
                .output
                .push(OutputLine::Debug(format!("[{level}] {msg}"))),
        }
    }

    /// Called from engine.rs when an ArgSuggestResponse IPC arrives from a mod.
    /// Only applied if `request_id` matches the most recently sent suggest request.
    pub fn handle_arg_suggest_response(&mut self, request_id: &str, suggestions: Vec<String>) {
        if self.pending_suggest_id.as_deref() == Some(request_id) {
            let prefix = &self.pending_suggest_prefix;
            self.completions = suggestions
                .into_iter()
                .map(|s| {
                    if prefix.is_empty() {
                        s
                    } else {
                        format!("{prefix} {s}")
                    }
                })
                .collect();
            self.completion_idx = 0;
        }
    }

    pub fn handle_command_response(&mut self, output: Vec<String>, error: Option<String>) {
        if let Some(last) = self.output.last()
            && matches!(last, OutputLine::Text(s) if s == "(waiting for mod response…)")
        {
            self.output.pop();
        }
        self.pending_invoke = None;
        if let Some(err) = error {
            tracing::warn!("mod command error: {err}");
            self.output.push(OutputLine::Error(err));
        } else {
            for line in output {
                tracing::info!("{line}");
                self.output.push(OutputLine::Text(line));
            }
        }
    }

    pub fn render(
        &mut self,
        ctx: &Context,
        mod_registry: &ModRegistry,
        vfs: &dyn Vfs,
        debug: crate::debug::SharedDebugSwitches,
        world: &crate::ecs::World,
    ) -> ConsoleAction {
        if !self.is_open {
            return ConsoleAction::None;
        }

        let te_id = egui::Id::new("console_te");
        let mut action = ConsoleAction::None;
        let mut submitted_input: Option<String> = None;
        let mut apply_completion: Option<String> = None;

        egui::TopBottomPanel::top("dev_console")
            .resizable(true)
            .default_height(320.0)
            .show(ctx, |ui| {
                // Dark terminal background
                let bg = Color32::from_rgba_premultiplied(18, 18, 24, 200);
                ui.painter().rect_filled(ui.max_rect(), 0.0, bg);
                ui.visuals_mut().override_text_color = Some(Color32::from_rgb(210, 210, 210));
                ui.style_mut().spacing.item_spacing = egui::vec2(4.0, 1.0);

                // Calculate heights
                let completion_rows = self.completions.len().min(MAX_COMPLETIONS) as f32;
                let completion_area_h = if self.completions.is_empty() {
                    0.0
                } else {
                    // 18.0 per item, plus 1.0 spacing per item
                    // +6.0 is to fix a bug where the autocomplete menu
                    // would slowly shrink the console height
                    let mut h = completion_rows * 19.0 + 6.0;

                    // Account for the "+ X more" label
                    if self.completions.len() > MAX_COMPLETIONS {
                        h += 15.0; // Approximate height of the extra label + spacing
                    }

                    h
                };
                let output_h = (ui.available_height() - INPUT_ROW_HEIGHT - completion_area_h - 8.0) // separator + padding
                    .max(40.0);

                // ── Output scroll area ─────────────────────────────────────────
                ScrollArea::vertical()
                    .id_salt("console_output")
                    .max_height(output_h)
                    .min_scrolled_height(output_h)
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        for line in &self.output {
                            match line {
                                OutputLine::Input(s) => {
                                    ui.label(
                                        RichText::new(s)
                                            .font(FontId::monospace(13.0))
                                            .color(Color32::from_rgb(80, 180, 255)),
                                    );
                                }
                                OutputLine::Text(s) => {
                                    ui.label(RichText::new(s).font(FontId::monospace(13.0)));
                                }
                                OutputLine::Error(s) => {
                                    ui.label(
                                        RichText::new(s)
                                            .font(FontId::monospace(13.0))
                                            .color(Color32::from_rgb(255, 80, 80)),
                                    );
                                }
                                OutputLine::Warn(s) => {
                                    ui.label(
                                        RichText::new(s)
                                            .font(FontId::monospace(13.0))
                                            .color(Color32::from_rgb(255, 190, 60)),
                                    );
                                }
                                OutputLine::Debug(s) => {
                                    ui.label(
                                        RichText::new(s)
                                            .font(FontId::monospace(12.0))
                                            .color(Color32::from_rgb(110, 140, 120)),
                                    );
                                }
                            }
                        }
                        ui.add_space(2.0);
                    });

                ui.separator();

                // ── Completion dropdown list ───────────────────────────────────
                if !self.completions.is_empty() {
                    let shown = self.completions.len().min(MAX_COMPLETIONS);
                    for i in 0..shown {
                        let c = &self.completions[i];
                        let selected = i == self.completion_idx;

                        let item_bg = if selected {
                            Color32::from_rgb(35, 65, 115)
                        } else {
                            Color32::TRANSPARENT
                        };

                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 18.0),
                            egui::Sense::click(),
                        );
                        ui.painter().rect_filled(rect, 2.0, item_bg);
                        ui.painter().text(
                            rect.left_center() + egui::vec2(6.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            c,
                            FontId::monospace(12.0),
                            if selected {
                                Color32::WHITE
                            } else {
                                Color32::from_rgb(160, 160, 160)
                            },
                        );

                        if response.clicked() {
                            apply_completion = Some(c.clone());
                        }
                    }
                    if self.completions.len() > MAX_COMPLETIONS {
                        ui.label(
                            RichText::new(format!(
                                "  … {} more",
                                self.completions.len() - MAX_COMPLETIONS
                            ))
                            .font(FontId::monospace(11.0))
                            .color(Color32::DARK_GRAY),
                        );
                    }
                }

                // ── Key handling (before TextEdit so Tab doesn't trigger focus traversal) ──
                let tab = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab));
                let enter = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
                let up = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp));
                let down = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown));
                // Tab applies the currently highlighted completion
                if tab && !self.completions.is_empty() {
                    apply_completion = Some(self.completions[self.completion_idx].clone());
                }

                // Arrow keys: navigate completions when visible, history otherwise
                if up {
                    if !self.completions.is_empty() {
                        let max = self.completions.len().min(MAX_COMPLETIONS);
                        self.completion_idx = if self.completion_idx == 0 {
                            max - 1
                        } else {
                            self.completion_idx - 1
                        };
                    } else if !self.history.is_empty() {
                        let pos = match self.history_pos {
                            None => self.history.len() - 1,
                            Some(0) => 0,
                            Some(n) => n - 1,
                        };
                        self.history_pos = Some(pos);
                        self.input_buf = self.history[pos].clone();
                        self.completions.clear();
                        self.completion_idx = 0;
                    }
                }

                if down {
                    if !self.completions.is_empty() {
                        let max = self.completions.len().min(MAX_COMPLETIONS);
                        self.completion_idx = (self.completion_idx + 1) % max;
                    } else {
                        match self.history_pos {
                            None => {}
                            Some(n) if n + 1 >= self.history.len() => {
                                self.history_pos = None;
                                self.input_buf.clear();
                                self.completions.clear();
                            }
                            Some(n) => {
                                self.history_pos = Some(n + 1);
                                self.input_buf = self.history[n + 1].clone();
                                self.completions.clear();
                                self.completion_idx = 0;
                            }
                        }
                    }
                }

                if enter {
                    let trimmed = self.input_buf.trim().to_owned();
                    if !trimmed.is_empty() {
                        submitted_input = Some(trimmed);
                        self.input_buf.clear();
                        self.completions.clear();
                        self.completion_idx = 0;
                        self.history_pos = None;
                    }
                }

                // ── Input row ─────────────────────────────────────────────────
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("> ")
                            .font(FontId::monospace(13.0))
                            .color(Color32::from_rgb(80, 180, 255)),
                    );

                    let te = TextEdit::singleline(&mut self.input_buf)
                        .font(FontId::monospace(13.0))
                        .desired_width(f32::INFINITY)
                        .frame(false)
                        .id(te_id);
                    let response = ui.add_sized([ui.available_width(), INPUT_ROW_HEIGHT], te);

                    if self.needs_focus || !response.has_focus() {
                        response.request_focus();
                        self.needs_focus = false;
                    }

                    if response.changed() {
                        self.completions =
                            completer::complete(&self.input_buf, &self.registry.roots);
                        self.completion_idx = 0;
                        self.history_pos = None;
                        self.pending_suggest_ipc = None;
                        if let Some(ctx) =
                            completer::arg_suggest_context(&self.input_buf, &self.registry.roots)
                        {
                            let id = SUGGEST_COUNTER.fetch_add(1, Ordering::Relaxed);
                            let request_id = format!("suggest_{id}");
                            self.pending_suggest_id = Some(request_id.clone());
                            self.pending_suggest_prefix = ctx.prefix;
                            self.pending_suggest_ipc = Some((
                                ctx.mod_id,
                                EngineMessage::ArgSuggestRequest {
                                    request_id,
                                    command_path: ctx.command_path,
                                    arg_index: ctx.arg_index,
                                    current: ctx.current,
                                },
                            ));
                        } else {
                            self.pending_suggest_id = None;
                        }
                    }
                });

                // Fill any remaining space so egui's Resize widget never auto-shrinks the panel.
                let remaining = ui.available_height();
                if remaining > 0.0 {
                    ui.allocate_space(egui::vec2(ui.available_width(), remaining));
                }
            });

        // Apply clicked / Tab'd completion outside the closure
        if let Some(c) = apply_completion {
            self.input_buf = if c.contains('<') { c } else { format!("{c} ") };
            self.completions = completer::complete(&self.input_buf, &self.registry.roots);
            self.completion_idx = 0;
            self.pending_suggest_ipc = None;
            if let Some(ctx) = completer::arg_suggest_context(&self.input_buf, &self.registry.roots)
            {
                let id = SUGGEST_COUNTER.fetch_add(1, Ordering::Relaxed);
                let request_id = format!("suggest_{id}");
                self.pending_suggest_id = Some(request_id.clone());
                self.pending_suggest_prefix = ctx.prefix;
                self.pending_suggest_ipc = Some((
                    ctx.mod_id,
                    EngineMessage::ArgSuggestRequest {
                        request_id,
                        command_path: ctx.command_path,
                        arg_index: ctx.arg_index,
                        current: ctx.current,
                    },
                ));
            } else {
                self.pending_suggest_id = None;
            }

            // Move cursor to end of the newly applied text
            let char_count = self.input_buf.chars().count();
            if let Some(mut state) = TextEdit::load_state(ctx, te_id) {
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(char_count),
                        egui::text::CCursor::new(char_count),
                    )));
                TextEdit::store_state(ctx, te_id, state);
            }
        }

        if let Some(input) = submitted_input {
            self.history.push(input.clone());
            action = self.execute(&input, mod_registry, vfs, debug, world);
        }

        if matches!(action, ConsoleAction::None)
            && let Some((mod_id, message)) = self.pending_suggest_ipc.take()
        {
            return ConsoleAction::SendIpc { mod_id, message };
        }

        action
    }

    fn execute(
        &mut self,
        input: &str,
        mod_registry: &ModRegistry,
        vfs: &dyn Vfs,
        debug: crate::debug::SharedDebugSwitches,
        world: &crate::ecs::World,
    ) -> ConsoleAction {
        self.output.push(OutputLine::Input(format!("> {input}")));

        let tokens = parser::tokenize_pub(input);
        if tokens.is_empty() {
            return ConsoleAction::None;
        }

        // Built-in special commands (need registry access, handled inline)
        match tokens[0].as_str() {
            "clear" => {
                self.output.clear();
                return ConsoleAction::None;
            }
            "help" => {
                let lines = self.run_help(tokens.get(1).map(String::as_str));
                for line in lines {
                    self.output.push(OutputLine::Text(line));
                }
                return ConsoleAction::None;
            }
            "quit" => {
                return ConsoleAction::Quit;
            }
            _ => {}
        }

        let root_name = &tokens[0];
        let root_node = match self.registry.roots.iter().find(|n| n.name == *root_name) {
            Some(n) => n,
            None => {
                tracing::warn!("console: unknown command '{root_name}'");
                self.output.push(OutputLine::Error(format!(
                    "unknown command '{root_name}' — type 'help'"
                )));
                return ConsoleAction::None;
            }
        };

        // Walk subcommands
        let mut node: &CommandNode = root_node;
        let mut arg_start = 1usize;
        loop {
            if arg_start >= tokens.len() {
                break;
            }
            if let Some(sub) = node
                .subcommands
                .iter()
                .find(|s| s.name == tokens[arg_start])
            {
                node = sub;
                arg_start += 1;
            } else {
                break;
            }
        }

        let node = node.clone();
        let raw_args: Vec<String> = tokens[arg_start..].to_vec();

        if node.handler.is_none() {
            let subs: Vec<&str> = node.subcommands.iter().map(|s| s.name.as_str()).collect();
            let msg = format!("incomplete command — subcommands: {}", subs.join(", "));
            tracing::warn!("console: {msg}");
            self.output.push(OutputLine::Error(msg));
            return ConsoleAction::None;
        }
        let parsed = match parser::parse_args(&raw_args, &node.args) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("console: bad arguments: {e}");
                self.output.push(OutputLine::Error(e));
                return ConsoleAction::None;
            }
        };

        match &node.source {
            CommandSource::Engine => {
                let pending_action = std::sync::Arc::new(std::sync::Mutex::new(
                    None::<crate::console::types::EngineSettingAction>,
                ));
                let ctx = CommandContext {
                    mod_registry,
                    vfs,
                    world,
                    fps: self.fps,
                    fps_cap: self.fps_cap,
                    vsync: self.vsync,
                    debug,
                    pending_action: std::sync::Arc::clone(&pending_action),
                };
                // we've verified handler exists, so unwrap() should be safe
                // but probably best to refactor asap
                match node.handler.unwrap()(parsed, &ctx) {
                    Ok(lines) => {
                        for line in lines {
                            tracing::info!("{line}");
                            self.output.push(OutputLine::Text(line));
                        }
                    }
                    Err(e) => {
                        tracing::warn!("console: command failed: {e}");
                        self.output.push(OutputLine::Error(e));
                    }
                }
                if let Ok(mut guard) = pending_action.lock()
                    && let Some(action) = guard.take()
                {
                    return ConsoleAction::EngineSettings(action);
                }
                ConsoleAction::None
            }
            CommandSource::Mod(mod_id) => {
                let request_id = format!(
                    "cmd_{mod_id}_{:x}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or(0)
                );
                let command_path: Vec<String> = tokens[0..arg_start].to_vec();
                let args_json: Vec<serde_json::Value> = parsed
                    .positional
                    .iter()
                    .map(|v| match v {
                        crate::console::types::ArgValue::String(s) => {
                            serde_json::Value::String(s.clone())
                        }
                        crate::console::types::ArgValue::Int(n) => {
                            serde_json::Value::Number((*n).into())
                        }
                        crate::console::types::ArgValue::Float(n) => {
                            serde_json::Number::from_f64(*n)
                                .map(serde_json::Value::Number)
                                .unwrap_or(serde_json::Value::Null)
                        }
                        crate::console::types::ArgValue::Bool(b) => serde_json::Value::Bool(*b),
                    })
                    .collect();

                self.output
                    .push(OutputLine::Text("(waiting for mod response…)".into()));
                self.pending_invoke = Some(PendingInvoke {
                    request_id: request_id.clone(),
                });

                ConsoleAction::SendIpc {
                    mod_id: mod_id.clone(),
                    message: EngineMessage::CommandInvoke {
                        request_id,
                        command_path,
                        args: args_json,
                    },
                }
            }
        }
    }

    fn run_help(&self, command: Option<&str>) -> Vec<String> {
        let builtins = [
            ("help [command]", "Show this help or details for a command"),
            ("clear", "Clear console output"),
            ("quit", "Shutdown the engine"),
        ];

        if let Some(cmd) = command {
            if let Some(node) = self.registry.roots.iter().find(|n| n.name == cmd) {
                return describe_node(node);
            }
            for (name, desc) in &builtins {
                if name.starts_with(cmd) {
                    return vec![format!("{name}  —  {desc}")];
                }
            }
            return vec![format!("no command named '{cmd}'")];
        }

        let mut lines = vec!["Available commands:".into(), String::new()];
        for (name, desc) in &builtins {
            lines.push(format!("  {name:<30}{desc}"));
        }
        for node in &self.registry.roots {
            if node.subcommands.is_empty() {
                lines.push(format!("  {:<30}{}", format_usage(node), node.description));
            } else {
                lines.push(format!("  {:<30}{}", node.name, node.description));
                for sub in &node.subcommands {
                    lines.push(format!(
                        "    {:<28}{}",
                        format!("{} {}", node.name, format_usage(sub)),
                        sub.description
                    ));
                }
            }
        }
        lines
    }
}

fn format_usage(node: &CommandNode) -> String {
    let mut s = node.name.clone();
    for arg in &node.args {
        if arg.required {
            s.push_str(&format!(" <{}>", arg.name));
        } else {
            s.push_str(&format!(" [{}]", arg.name));
        }
    }
    s
}

fn describe_node(node: &CommandNode) -> Vec<String> {
    let mut lines = vec![format!("  {}", node.description), String::new()];
    lines.push(format!("usage:  {}", format_usage(node)));
    if !node.subcommands.is_empty() {
        lines.push(String::new());
        lines.push("subcommands:".into());
        for sub in &node.subcommands {
            lines.push(format!("  {:<20}{}", format_usage(sub), sub.description));
        }
    }
    if !node.args.is_empty() {
        lines.push(String::new());
        lines.push("arguments:".into());
        for arg in &node.args {
            let req = if arg.required { "required" } else { "optional" };
            lines.push(format!("  <{}>  ({req})  {}", arg.name, arg.description));
        }
    }
    lines
}
