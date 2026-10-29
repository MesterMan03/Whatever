use crate::console::commands::{debug_cmd, engine_cmd, mods_cmd, scene_cmd, vfs_cmd};
use crate::console::completer;
use crate::console::parser;
use crate::console::registry::CommandRegistry;
use crate::console::types::{
    ArgSpec, ArgType, ArgumentContext, CommandContext, CommandNode, CommandSource, OutputLine,
};
use crate::mods::ModRegistry;
use crate::script::ipc::EngineMessage;
use crate::vfs::Vfs;
use egui::{Color32, Context, FontId, Key, Modifiers, RichText, ScrollArea, TextEdit};
use std::sync::atomic::{AtomicU64, Ordering};

const INPUT_ROW_HEIGHT: f32 = 26.0;
const MAX_COMPLETIONS: usize = 8;
const COMPLETION_ROW_HEIGHT: f32 = 19.0;

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
        // These execute inline because they operate on the console itself, but still
        // belong in the registry for completion, help and command-name conflicts.
        registry.register_engine(
            CommandNode::engine("help", "Show this help or details for a command").with_args(vec![
                ArgSpec {
                    name: "command".into(),
                    arg_type: ArgType::String,
                    required: false,
                    description: "Command to describe".into(),
                    has_suggest: false,
                    suggest: None,
                },
            ]),
        );
        registry.register_engine(CommandNode::engine("clear", "Clear console output"));
        registry.register_engine(CommandNode::engine("quit", "Shutdown the engine"));
        registry.register_engine(debug_cmd::node());
        registry.register_engine(engine_cmd::node());
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
            self.dismiss_completions();
        }
    }

    /// Escape key: clear completions first; close the console only when already clear.
    pub fn escape(&mut self) {
        if !self.completions.is_empty() {
            self.dismiss_completions();
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
            self.completions.sort_unstable();
            self.completions.dedup();
            self.completion_idx = 0;
            self.pending_suggest_id = None;
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

    fn dismiss_completions(&mut self) {
        self.completions.clear();
        self.completion_idx = 0;
        self.pending_suggest_id = None;
        self.pending_suggest_ipc = None;
    }

    fn refresh_completions(&mut self, context: &ArgumentContext) {
        self.dismiss_completions();
        self.completions = completer::complete(&self.input_buf, &self.registry.roots, context);
        if let Some(context) = completer::arg_suggest_context(&self.input_buf, &self.registry.roots)
        {
            let id = SUGGEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            let request_id = format!("suggest_{id}");
            self.pending_suggest_id = Some(request_id.clone());
            self.pending_suggest_prefix = context.prefix;
            self.pending_suggest_ipc = Some((
                context.mod_id,
                EngineMessage::ArgSuggestRequest {
                    request_id,
                    command_path: context.command_path,
                    arg_index: context.arg_index,
                    current: context.current,
                },
            ));
        }
    }

    fn navigate(&mut self, backwards: bool) {
        if !self.completions.is_empty() {
            let count = self.completions.len();
            self.completion_idx = if backwards {
                (self.completion_idx + count - 1) % count
            } else {
                (self.completion_idx + 1) % count
            };
        } else if backwards && !self.history.is_empty() {
            let pos = self
                .history_pos
                .map_or(self.history.len() - 1, |n| n.saturating_sub(1));
            self.history_pos = Some(pos);
            self.input_buf = self.history[pos].clone();
            self.dismiss_completions();
        } else if !backwards && let Some(pos) = self.history_pos {
            self.history_pos = (pos + 1 < self.history.len()).then_some(pos + 1);
            self.input_buf = self
                .history_pos
                .map_or_else(String::new, |n| self.history[n].clone());
            self.dismiss_completions();
        }
    }

    /// Keep the selected row visible while allowing navigation over the entire list.
    fn completion_range(&self, capacity: usize) -> std::ops::Range<usize> {
        let shown = self.completions.len().min(capacity);
        let start = self
            .completion_idx
            .saturating_sub(shown / 2)
            .min(self.completions.len() - shown);
        start..start + shown
    }

    fn completion_suffix(&self) -> Option<&str> {
        self.completions
            .get(self.completion_idx)?
            .strip_prefix(&self.input_buf)
            .filter(|suffix| !suffix.is_empty())
    }

    fn apply_completion(
        &mut self,
        ctx: &Context,
        te_id: egui::Id,
        completion: String,
        context: &ArgumentContext,
    ) {
        self.input_buf = if completion.contains('<') {
            completion
        } else {
            format!("{completion} ")
        };
        self.history_pos = None;
        self.refresh_completions(context);
        let cursor = egui::text::CCursor::new(self.input_buf.chars().count());
        if let Some(mut state) = TextEdit::load_state(ctx, te_id) {
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(cursor)));
            TextEdit::store_state(ctx, te_id, state);
        }
        self.needs_focus = true;
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

        let context = &ArgumentContext {
            mod_registry,
            world,
        };
        let te_id = egui::Id::new("console_te");
        // Consume navigation before TextEdit can move the caret or transfer focus.
        let (tab, enter, up, down) = ctx.input_mut(|input| {
            (
                input.consume_key(Modifiers::NONE, Key::Tab),
                input.consume_key(Modifiers::NONE, Key::Enter),
                input.consume_key(Modifiers::NONE, Key::ArrowUp),
                input.consume_key(Modifiers::NONE, Key::ArrowDown),
            )
        });
        if up {
            self.navigate(true);
        }
        if down {
            self.navigate(false);
        }
        if tab && let Some(completion) = self.completions.get(self.completion_idx).cloned() {
            self.apply_completion(ctx, te_id, completion, context);
        }

        let submitted_input = if enter && !self.input_buf.trim().is_empty() {
            let input = self.input_buf.trim().to_owned();
            self.input_buf.clear();
            self.dismiss_completions();
            self.history_pos = None;
            Some(input)
        } else {
            None
        };
        let mut clicked_completion = None;

        egui::TopBottomPanel::top("dev_console")
            .resizable(true)
            .default_height(320.0)
            .min_height(100.0)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgba_unmultiplied(18, 18, 24, 235))
                    .inner_margin(egui::Margin::symmetric(8.0, 4.0)),
            )
            .show(ctx, |ui| {
                ui.visuals_mut().override_text_color = Some(Color32::from_rgb(210, 210, 210));
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                // Claim the panel once. Child content cannot resize it when suggestions change.
                let bounds = ui.available_rect_before_wrap();
                ui.allocate_rect(bounds, egui::Sense::hover());
                let input_rect = egui::Rect::from_min_max(
                    egui::pos2(
                        bounds.left(),
                        (bounds.bottom() - INPUT_ROW_HEIGHT).max(bounds.top()),
                    ),
                    bounds.max,
                );
                let mut input_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt("console_input_row")
                        .max_rect(input_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                input_ui.set_clip_rect(input_rect.intersect(ui.clip_rect()));
                input_ui.label(
                    RichText::new("> ")
                        .font(FontId::monospace(13.0))
                        .color(Color32::from_rgb(80, 180, 255)),
                );
                let width = input_ui.available_width();
                let edit = TextEdit::singleline(&mut self.input_buf)
                    .font(FontId::monospace(13.0))
                    .desired_width(width)
                    .min_size(egui::vec2(width, INPUT_ROW_HEIGHT))
                    .margin(egui::vec2(0.0, 0.0))
                    .vertical_align(egui::Align::Center)
                    .frame(false)
                    .id(te_id)
                    .show(&mut input_ui);
                if self.needs_focus || !edit.response.has_focus() {
                    edit.response.request_focus();
                    self.needs_focus = false;
                }
                if edit.response.changed() {
                    self.history_pos = None;
                    self.refresh_completions(context);
                }
                // Paint only the untyped suffix, using TextEdit's own galley position so
                // the preview also follows horizontal scrolling and Unicode caret offsets.
                if let Some(cursor) = edit.cursor_range
                    && cursor.is_empty()
                    && cursor.primary.ccursor.index == self.input_buf.chars().count()
                    && let Some(suffix) = self.completion_suffix()
                {
                    let end = edit.galley.pos_from_cursor(&cursor.primary);
                    input_ui.painter().with_clip_rect(edit.text_clip_rect).text(
                        edit.galley_pos + end.min.to_vec2(),
                        egui::Align2::LEFT_TOP,
                        suffix,
                        FontId::monospace(13.0),
                        Color32::from_white_alpha(95),
                    );
                }

                // Fit whole rows above the fixed input, preserving room for output.
                let available_rows = ((input_rect.top() - bounds.top() - 32.0).max(0.0)
                    / COMPLETION_ROW_HEIGHT) as usize;
                let footer_rows =
                    usize::from(self.completions.len() > available_rows.min(MAX_COMPLETIONS));
                let capacity = available_rows
                    .saturating_sub(footer_rows)
                    .min(MAX_COMPLETIONS);
                let range = self.completion_range(capacity);
                let completion_height = (range.len() + footer_rows) as f32 * COMPLETION_ROW_HEIGHT;
                let list_top = input_rect.top() - completion_height;
                let list_rect = egui::Rect::from_min_max(
                    egui::pos2(bounds.left(), list_top),
                    input_rect.right_top(),
                );
                let painter = ui
                    .painter()
                    .with_clip_rect(list_rect.intersect(ui.clip_rect()));
                for (row, index) in range.clone().enumerate() {
                    let rect = egui::Rect::from_min_size(
                        list_rect.min + egui::vec2(0.0, row as f32 * COMPLETION_ROW_HEIGHT),
                        egui::vec2(list_rect.width(), COMPLETION_ROW_HEIGHT),
                    );
                    let selected = index == self.completion_idx;
                    if selected {
                        painter.rect_filled(rect, 2.0, Color32::from_rgb(35, 65, 115));
                    }
                    painter.text(
                        rect.left_center() + egui::vec2(6.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        &self.completions[index],
                        FontId::monospace(12.0),
                        if selected {
                            Color32::WHITE
                        } else {
                            Color32::from_gray(160)
                        },
                    );
                    if ui
                        .interact(
                            rect.intersect(list_rect),
                            egui::Id::new(("console_completion", index)),
                            egui::Sense::click(),
                        )
                        .clicked()
                    {
                        clicked_completion = Some(self.completions[index].clone());
                    }
                }
                if footer_rows > 0 {
                    painter.text(
                        list_rect.min + egui::vec2(6.0, range.len() as f32 * COMPLETION_ROW_HEIGHT),
                        egui::Align2::LEFT_TOP,
                        format!(
                            "… {} above · {} below · ↑/↓ to select",
                            range.start,
                            self.completions.len() - range.end
                        ),
                        FontId::monospace(11.0),
                        Color32::from_gray(130),
                    );
                }
                let separator_y = list_top - 2.0;
                ui.painter().hline(
                    bounds.x_range(),
                    separator_y,
                    ui.visuals().widgets.noninteractive.bg_stroke,
                );
                let output_rect = egui::Rect::from_min_max(
                    bounds.min,
                    egui::pos2(bounds.right(), (separator_y - 2.0).max(bounds.top())),
                );
                self.render_output(ui, output_rect);
            });

        if let Some(completion) = clicked_completion {
            self.apply_completion(ctx, te_id, completion, context);
        }
        let action = if let Some(input) = submitted_input {
            self.history.push(input.clone());
            self.execute(&input, mod_registry, vfs, debug, world)
        } else {
            ConsoleAction::None
        };
        if matches!(action, ConsoleAction::None)
            && let Some((mod_id, message)) = self.pending_suggest_ipc.take()
        {
            return ConsoleAction::SendIpc { mod_id, message };
        }
        action
    }

    fn render_output(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        let mut output_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("console_output_region")
                .max_rect(rect),
        );
        output_ui.set_clip_rect(rect.intersect(ui.clip_rect()));
        ScrollArea::vertical()
            .id_salt("console_output")
            .max_height(rect.height())
            .min_scrolled_height(rect.height())
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(&mut output_ui, |ui| {
                for line in &self.output {
                    let (text, color, size) = match line {
                        OutputLine::Input(s) => (s, Color32::from_rgb(80, 180, 255), 13.0),
                        OutputLine::Text(s) => (s, Color32::from_rgb(210, 210, 210), 13.0),
                        OutputLine::Error(s) => (s, Color32::from_rgb(255, 80, 80), 13.0),
                        OutputLine::Warn(s) => (s, Color32::from_rgb(255, 190, 60), 13.0),
                        OutputLine::Debug(s) => (s, Color32::from_rgb(110, 140, 120), 12.0),
                    };
                    ui.add(
                        egui::Label::new(
                            RichText::new(text)
                                .font(FontId::monospace(size))
                                .color(color),
                        )
                        .wrap(),
                    );
                }
            });
    }

    pub(crate) fn discard_output(&mut self) {
        self.output.clear();
    }

    pub(crate) fn execute_headless(
        &mut self,
        input: &str,
        registry: &ModRegistry,
        vfs: &dyn Vfs,
        debug: crate::debug::SharedDebugSwitches,
        world: &crate::ecs::World,
    ) -> ConsoleAction {
        if self.pending_invoke.is_some() {
            tracing::warn!("a console command is still awaiting its mod response");
            return ConsoleAction::None;
        }
        self.output.clear();
        let action = self.execute(input, registry, vfs, debug, world);
        // Regular command results/errors are already logged by execute/response.
        // Help is rendered inline and needs its output mirrored explicitly.
        if input.split_whitespace().next() == Some("help") {
            for line in &self.output {
                if let OutputLine::Text(text) = line {
                    tracing::info!("{text}");
                }
            }
        }
        self.output.clear();
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
                let path = tokens[1..].join(" ");
                let lines = self.run_help((!path.is_empty()).then_some(path.as_str()));
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
        let parsed = match parser::parse_args(
            &raw_args,
            &node.args,
            &ArgumentContext {
                mod_registry,
                world,
            },
        ) {
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
                let Some(handler) = node.handler else {
                    return ConsoleAction::None;
                };
                match handler(parsed, &ctx) {
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
                let command_path = parser::mod_command_path(&tokens[..arg_start]);
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
        if let Some(cmd) = command {
            let path: Vec<_> = cmd.split_whitespace().collect();
            if let Some(node) = self.registry.find(&path) {
                let mut described = node.clone();
                described.name = cmd.to_owned();
                return describe_node(&described);
            }
            return vec![format!("no command named '{cmd}'")];
        }

        let mut lines = vec!["Available commands:".into(), String::new()];
        let mut roots: Vec<_> = self.registry.roots.iter().collect();
        roots.sort_unstable_by_key(|node| &node.name);
        for node in roots {
            append_help_entry(&mut lines, node, "", 2);
            let mut subs: Vec<_> = node.subcommands.iter().collect();
            subs.sort_unstable_by_key(|sub| &sub.name);
            for sub in subs {
                append_help_entry(&mut lines, sub, &format!("{} ", node.name), 4);
            }
            lines.push(String::new());
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
        let mut subs: Vec<_> = node.subcommands.iter().collect();
        subs.sort_unstable_by_key(|sub| &sub.name);
        for sub in subs {
            append_help_entry(&mut lines, sub, &format!("{} ", node.name), 2);
        }
    }
    if !node.args.is_empty() {
        lines.push(String::new());
        lines.push("arguments:".into());
        for arg in &node.args {
            let req = if arg.required { "required" } else { "optional" };
            lines.push(format!("  <{}> ({req})", arg.name));
            lines.push(format!("    {}", arg.description));
        }
    }
    lines
}

/// Give signatures their own line: no column width can safely bound mod-defined usage.
fn append_help_entry(lines: &mut Vec<String>, node: &CommandNode, prefix: &str, indent: usize) {
    lines.push(format!("{:indent$}{prefix}{}", "", format_usage(node)));
    if !node.description.is_empty() {
        lines.push(format!(
            "{:width$}{}",
            "",
            node.description,
            width = indent + 2
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::debug::DebugSwitches;
    use crate::vfs::LayeredVfs;
    use std::sync::Arc;

    fn frame(
        console: &mut DevConsole,
        ctx: &Context,
        events: Vec<egui::Event>,
        height: f32,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, height),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                console.render(
                    ctx,
                    &ModRegistry::new(),
                    &LayeredVfs::new(),
                    Arc::new(DebugSwitches::new(false, false, false, false, false)),
                    &crate::ecs::World::new(),
                );
                let rect = ctx
                    .read_response(egui::Id::new("console_te"))
                    .expect("console input is present")
                    .rect;
                let panel_bottom = ctx.available_rect().top();
                ctx.data_mut(|data| {
                    data.insert_temp(egui::Id::new("test_input_rect"), rect);
                    data.insert_temp(egui::Id::new("test_panel_bottom"), panel_bottom);
                });
            },
        )
    }

    fn completions(console: &DevConsole, input: &str) -> Vec<String> {
        completer::complete(
            input,
            &console.registry.roots,
            &ArgumentContext {
                mod_registry: &ModRegistry::new(),
                world: &crate::ecs::World::new(),
            },
        )
    }

    fn key(key: Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }
    }

    fn input_rect(ctx: &Context) -> egui::Rect {
        ctx.data(|data| data.get_temp(egui::Id::new("test_input_rect")))
            .expect("input measured during the frame")
    }

    #[test]
    fn builtins_are_completed_once_and_reserved_against_mods() {
        let mut console = DevConsole::new();
        assert_eq!(completions(&console, "h"), ["help"]);
        assert_eq!(completions(&console, "help en"), ["help engine"]);
        let commands = completions(&console, "");
        assert!(commands.windows(2).all(|pair| pair[0] < pair[1]));
        for command in ["help", "clear", "quit"] {
            assert!(commands.iter().any(|name| name == command));
        }
        assert_eq!(
            console
                .registry
                .register_mod("demo", CommandNode::engine("help", "Mod help")),
            Some("demo:help".into())
        );
        let help = console.run_help(None);
        assert_eq!(
            help.iter()
                .filter(|line| line.trim() == "help [command]")
                .count(),
            1
        );
    }

    #[test]
    fn navigation_reaches_hidden_rows_wraps_and_tab_accepts_the_selected_preview() {
        let mut console = DevConsole::new();
        console.toggle();
        console.input_buf = "choice_".into();
        console.completions = (0..27).map(|i| format!("choice_{i:02}")).collect();
        let ctx = Context::default();
        let _ = frame(&mut console, &ctx, vec![], 600.0);
        for index in 1..27 {
            let _ = frame(&mut console, &ctx, vec![key(Key::ArrowDown)], 600.0);
            assert_eq!(console.completion_idx, index);
            for capacity in [1, 3, MAX_COMPLETIONS] {
                assert!(console.completion_range(capacity).contains(&index));
            }
            assert_eq!(console.input_buf, "choice_");
        }
        let _ = frame(&mut console, &ctx, vec![key(Key::ArrowDown)], 600.0);
        assert_eq!(console.completion_idx, 0);
        let _ = frame(&mut console, &ctx, vec![key(Key::ArrowUp)], 600.0);
        assert_eq!(console.completion_idx, 26);
        assert_eq!(console.completion_suffix(), Some("26"));
        let _ = frame(&mut console, &ctx, vec![key(Key::Tab)], 600.0);
        assert_eq!(console.input_buf, "choice_26 ");
        assert!(console.history.is_empty());
    }

    #[test]
    fn suggestions_do_not_move_input_or_resize_panel_and_ghost_text_is_only_a_preview() {
        let mut console = DevConsole::new();
        console.toggle();
        let ctx = Context::default();
        let _ = frame(&mut console, &ctx, vec![], 600.0);
        let original_input = input_rect(&ctx);
        let original_panel_bottom = ctx
            .data(|data| data.get_temp::<f32>(egui::Id::new("test_panel_bottom")))
            .expect("panel measured during the frame");
        for _ in 0..8 {
            let _ = frame(
                &mut console,
                &ctx,
                vec![egui::Event::Text("h".into())],
                600.0,
            );
            let output = frame(&mut console, &ctx, vec![], 600.0);
            assert_eq!(input_rect(&ctx), original_input);
            assert_eq!(
                ctx.data(|data| data.get_temp::<f32>(egui::Id::new("test_panel_bottom")))
                    .expect("panel measured during the frame"),
                original_panel_bottom
            );
            assert_eq!(console.input_buf, "h");
            assert!(output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::epaint::Shape::Text(text)
                    if text.galley.job.text == "elp" && text.galley.job.sections[0].format.color.a() == 95)
            }));
            console.escape();
            console.input_buf.clear();
            let _ = frame(&mut console, &ctx, vec![], 600.0);
            assert_eq!(input_rect(&ctx), original_input);
        }
        // The preview goes away while editing in the middle of the line.
        console.input_buf = "h".into();
        console.refresh_completions(&ArgumentContext {
            mod_registry: &ModRegistry::new(),
            world: &crate::ecs::World::new(),
        });
        let output = frame(&mut console, &ctx, vec![key(Key::Home)], 600.0);
        assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::epaint::Shape::Text(text) if text.galley.job.text == "elp")));
    }

    #[test]
    fn delayed_suggestions_are_sorted_and_cannot_reopen_a_dismissed_list() {
        let mut console = DevConsole::new();
        console.toggle();
        console.pending_suggest_id = Some("current".into());
        console.pending_suggest_prefix = "demo".into();
        console.handle_arg_suggest_response("old", vec!["wrong".into()]);
        assert!(console.completions.is_empty());
        console.handle_arg_suggest_response(
            "current",
            vec!["zebra".into(), "apple".into(), "apple".into()],
        );
        assert_eq!(console.completions, ["demo apple", "demo zebra"]);
        console.pending_suggest_id = Some("late".into());
        console.escape();
        console.handle_arg_suggest_response("late", vec!["late".into()]);
        assert!(console.completions.is_empty());
        assert!(console.is_open);
        console.pending_suggest_id = Some("closed".into());
        console.toggle();
        console.handle_arg_suggest_response("closed", vec!["late".into()]);
        assert!(console.completions.is_empty());
    }

    #[test]
    fn long_help_signatures_and_descriptions_have_separate_wrapping_lines() {
        let mut console = DevConsole::new();
        let description = "Spawn a point light at a world position.";
        let long = CommandNode::engine("point", description).with_args(
            ["x", "y", "z", "r", "g", "b", "intensity", "range"]
                .into_iter()
                .map(|name| ArgSpec {
                    name: name.into(),
                    arg_type: ArgType::Float,
                    required: true,
                    description: "Argument description".into(),
                    has_suggest: false,
                    suggest: None,
                })
                .collect(),
        );
        console.registry.register_engine(
            CommandNode::engine("spawnlight", "Spawn a light").with_subcommands(vec![long]),
        );
        for lines in [console.run_help(None), console.run_help(Some("spawnlight"))] {
            let index = lines
                .iter()
                .position(|line| line.contains("spawnlight point <x>"))
                .expect("long usage is listed");
            assert!(!lines[index].contains(description));
            assert_eq!(lines[index + 1].trim(), description);
        }
        console.toggle();
        console.output = console
            .run_help(None)
            .into_iter()
            .map(OutputLine::Text)
            .collect();
        let ctx = Context::default();
        let _ = frame(&mut console, &ctx, vec![], 600.0);
        let _ = frame(&mut console, &ctx, vec![], 180.0);
        assert!(input_rect(&ctx).bottom() <= 180.0);
    }
}

#[cfg(test)]
mod argument_tests {
    use super::*;
    use crate::console::registry::command_node_from_spec;
    use crate::runtime::{LaunchOptions, LoadedGame, RuntimeMode};
    use std::sync::{Arc, Mutex};

    struct Fixture(std::path::PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn contextual_arguments_complete_and_validate_before_mod_invocation() {
        let fixture = Fixture(std::env::temp_dir().join(format!(
            "whatever-console-{}-{}",
            std::process::id(),
            SUGGEST_COUNTER.fetch_add(1, Ordering::Relaxed)
        )));
        std::fs::create_dir_all(&fixture.0).expect("fixture");
        for (name, sides) in [
            ("core", "['client', 'server']"),
            ("active", "['server']"),
            ("client_only", "['client']"),
        ] {
            let root = fixture.0.join(name);
            std::fs::create_dir(&root).expect("mod directory");
            std::fs::write(
                root.join("mod.toml"),
                format!("[mod]\nid='{name}'\nname='{name}'\nversion='1.0.0'\nsides={sides}\n"),
            )
            .expect("manifest");
        }
        let log = std::fs::File::create(fixture.0.join("test.log")).expect("log");
        let game = LoadedGame::load(
            &LaunchOptions {
                mode: Some(RuntimeMode::Server),
                mods: Some(fixture.0.clone()),
            },
            &crate::debug::DebugConfig::from_args(&[]),
            &fixture.0,
            Arc::new(Mutex::new(std::io::BufWriter::new(log))),
        )
        .expect("load active mods");
        let spec: crate::script::ipc::CommandNodeSpec = serde_json::from_value(serde_json::json!({
            "name": "target", "description": "Target a loaded mod", "subcommands": [], "has_handler": true,
            "args": [{"name":"mod", "type":"mod_id", "required":true, "has_suggest":true}]
        })).expect("script command spec");
        let mut console = DevConsole::new();
        console
            .registry
            .register_mod("core", command_node_from_spec(&spec, "core"));
        let mut world = crate::ecs::World::new();
        let context = ArgumentContext {
            mod_registry: &game.registry,
            world: &world,
        };
        assert_eq!(
            completer::complete("target ", &console.registry.roots, &context),
            ["target active", "target core"]
        );
        assert_eq!(
            completer::complete("vfs list a", &console.registry.roots, &context),
            ["vfs list active"]
        );
        assert_eq!(
            completer::complete("mods get c", &console.registry.roots, &context),
            ["mods get core"]
        );
        assert!(completer::arg_suggest_context("target ", &console.registry.roots).is_none());
        let invalid = console.execute(
            "target client_only",
            &game.registry,
            game.vfs.as_ref(),
            game.debug.shared_switches(),
            &world,
        );
        assert!(matches!(invalid, ConsoleAction::None));
        assert!(console.pending_invoke.is_none());
        assert!(
            matches!(console.output.last(), Some(OutputLine::Error(error)) if error.contains("not loaded"))
        );
        let valid = console.execute(
            "target active",
            &game.registry,
            game.vfs.as_ref(),
            game.debug.shared_switches(),
            &world,
        );
        assert!(
            matches!(valid, ConsoleAction::SendIpc { mod_id, message: EngineMessage::CommandInvoke {args, ..} }
            if mod_id == "core" && args == vec![serde_json::json!("active")])
        );

        // Built-in names are reserved; a conflicting mod command remains usable
        // through its namespace while invocation targets its original handler key.
        let alias_spec = crate::script::ipc::CommandNodeSpec {
            name: "help".into(),
            ..spec.clone()
        };
        console
            .registry
            .register_mod("core", command_node_from_spec(&alias_spec, "core"));
        let aliased = console.execute(
            "core:help core",
            &game.registry,
            game.vfs.as_ref(),
            game.debug.shared_switches(),
            &world,
        );
        assert!(
            matches!(aliased, ConsoleAction::SendIpc {message: EngineMessage::CommandInvoke {command_path, ..}, ..}
            if command_path == ["help"])
        );
        let mut suggest_spec = alias_spec;
        suggest_spec.args[0].arg_type = "string".into();
        let alias = command_node_from_spec(&suggest_spec, "core");
        let mut alias_registry = CommandRegistry::new();
        alias_registry.register_engine(CommandNode::engine("help", ""));
        alias_registry.register_mod("core", alias);
        assert_eq!(
            completer::arg_suggest_context("core:help co", &alias_registry.roots)
                .expect("custom suggester context")
                .command_path,
            ["help"]
        );

        let id = world.create_entity();
        let scene = scene_cmd::node();
        let context = ArgumentContext {
            mod_registry: &game.registry,
            world: &world,
        };
        assert_eq!(
            completer::complete("scene inspect ", std::slice::from_ref(&scene), &context),
            [format!("scene inspect {id}")]
        );
        let entity_args = &scene
            .subcommands
            .iter()
            .find(|node| node.name == "inspect")
            .expect("inspect command")
            .args;
        assert!(parser::parse_args(&[id.to_string()], entity_args, &context).is_ok());
        assert!(parser::parse_args(&["malformed".into()], entity_args, &context).is_err());
        world.destroy_entity(id);
        let next_id = world.create_entity();
        let context = ArgumentContext {
            mod_registry: &game.registry,
            world: &world,
        };
        assert!(parser::parse_args(&[id.to_string()], entity_args, &context).is_err());
        assert!(parser::parse_args(&[next_id.to_string()], entity_args, &context).is_ok());
        assert_eq!(
            completer::complete("scene inspect ", &[scene], &context),
            [format!("scene inspect {next_id}")]
        );

        let boolean = CommandNode::engine("toggle", "").with_args(vec![ArgSpec {
            name: "enabled".into(),
            arg_type: ArgType::Bool,
            required: true,
            description: String::new(),
            has_suggest: false,
            suggest: None,
        }]);
        assert_eq!(
            completer::complete("toggle ", std::slice::from_ref(&boolean), &context),
            ["toggle false", "toggle true"]
        );
        assert!(matches!(
            parser::parse_args(&["yes".into()], &boolean.args, &context)
                .expect("boolean alias")
                .positional[0],
            crate::console::types::ArgValue::Bool(true)
        ));
    }
}
