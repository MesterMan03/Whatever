use crate::console::commands::{engine_cmd, markbench, mods_cmd, vfs_cmd};
use crate::console::completer;
use crate::console::parser;
use crate::console::registry::CommandRegistry;
use crate::console::types::{CommandContext, CommandNode, CommandSource, OutputLine};
use crate::mods::ModRegistry;
use crate::script::ipc::EngineMessage;
use crate::vfs::Vfs;
use egui::{Color32, Context, FontId, Key, Modifiers, RichText, ScrollArea, TextEdit};

pub struct DevConsole {
    pub is_open: bool,
    input_buf: String,
    output: Vec<OutputLine>,
    history: Vec<String>,
    history_pos: Option<usize>,
    pub registry: CommandRegistry,
    pub fps: f32,
    completions: Vec<String>,
    completion_idx: usize,
    pub pending_invoke: Option<PendingInvoke>,
}

pub struct PendingInvoke {
    pub request_id: String,
    pub mod_id: String,
}

pub enum ConsoleAction {
    None,
    SendIpc { mod_id: String, message: EngineMessage },
}

impl DevConsole {
    pub fn new() -> Self {
        let mut registry = CommandRegistry::new();
        registry.register_engine(engine_cmd::version_node());
        registry.register_engine(markbench::node());
        registry.register_engine(mods_cmd::node());
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
            completions: Vec::new(),
            completion_idx: 0,
            pending_invoke: None,
        }
    }

    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
    }

    pub fn handle_command_response(&mut self, output: Vec<String>, error: Option<String>) {
        // Remove the "waiting…" placeholder line
        if let Some(last) = self.output.last() {
            if matches!(last, OutputLine::Text(s) if s == "(waiting for mod response…)") {
                self.output.pop();
            }
        }
        self.pending_invoke = None;
        if let Some(err) = error {
            self.output.push(OutputLine::Error(err));
        } else {
            for line in output {
                self.output.push(OutputLine::Text(line));
            }
        }
    }

    /// Render the console panel. Returns any IPC action the engine must perform.
    pub fn render(
        &mut self,
        ctx: &Context,
        mod_registry: &ModRegistry,
        vfs: &dyn Vfs,
    ) -> ConsoleAction {
        if !self.is_open {
            return ConsoleAction::None;
        }

        let mut action = ConsoleAction::None;
        let mut submitted_input: Option<String> = None;

        egui::TopBottomPanel::top("dev_console")
            .resizable(true)
            .min_height(200.0)
            .max_height(600.0)
            .frame(egui::Frame::none().fill(Color32::from_rgba_premultiplied(20, 20, 25, 230)))
            .show(ctx, |ui| {
                ui.visuals_mut().override_text_color = Some(Color32::from_rgb(220, 220, 220));
                ui.style_mut().spacing.item_spacing = egui::vec2(4.0, 2.0);

                let output_height = ui.available_height() - 52.0;

                // Output history
                ScrollArea::vertical()
                    .id_salt("console_output")
                    .max_height(output_height.max(80.0))
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
                                            .color(Color32::from_rgb(100, 200, 255)),
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
                            }
                        }
                        ui.add_space(4.0);
                    });

                ui.separator();

                // Completion suggestions
                if !self.completions.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(6.0);
                        for (i, c) in self.completions.iter().enumerate() {
                            let color = if i == self.completion_idx {
                                Color32::from_rgb(255, 220, 80)
                            } else {
                                Color32::from_rgb(140, 140, 140)
                            };
                            ui.label(RichText::new(c).font(FontId::monospace(12.0)).color(color));
                            if i + 1 < self.completions.len() {
                                ui.label(
                                    RichText::new("  ")
                                        .font(FontId::monospace(12.0)),
                                );
                            }
                        }
                    });
                }

                // Input row
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("> ")
                            .font(FontId::monospace(13.0))
                            .color(Color32::from_rgb(100, 200, 255)),
                    );

                    let te = TextEdit::singleline(&mut self.input_buf)
                        .font(FontId::monospace(13.0))
                        .desired_width(f32::INFINITY)
                        .frame(false);
                    let response = ui.add_sized([ui.available_width(), 22.0], te);
                    response.request_focus();

                    if response.changed() {
                        self.completions =
                            completer::complete(&self.input_buf, &self.registry.roots);
                        self.completion_idx = 0;
                        self.history_pos = None;
                    }

                    // Key handling
                    let tab = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab));
                    let enter = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
                    let up = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp));
                    let down = ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown));

                    if tab && !self.completions.is_empty() {
                        let c = self.completions[self.completion_idx].clone();
                        self.input_buf = if c.contains('<') { c } else { format!("{c} ") };
                        self.completions =
                            completer::complete(&self.input_buf, &self.registry.roots);
                        self.completion_idx = 0;
                    }

                    if up && !self.history.is_empty() {
                        let pos = match self.history_pos {
                            None => self.history.len() - 1,
                            Some(0) => 0,
                            Some(n) => n - 1,
                        };
                        self.history_pos = Some(pos);
                        self.input_buf = self.history[pos].clone();
                        self.completions.clear();
                    }

                    if down {
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
                            }
                        }
                    }

                    if enter {
                        let trimmed = self.input_buf.trim().to_owned();
                        if !trimmed.is_empty() {
                            submitted_input = Some(trimmed);
                            self.input_buf.clear();
                            self.completions.clear();
                            self.history_pos = None;
                        }
                    }
                });
            });

        if let Some(input) = submitted_input {
            self.history.push(input.clone());
            action = self.execute(&input, mod_registry, vfs);
        }

        action
    }

    fn execute(
        &mut self,
        input: &str,
        mod_registry: &ModRegistry,
        vfs: &dyn Vfs,
    ) -> ConsoleAction {
        self.output.push(OutputLine::Input(format!("> {input}")));

        let tokens = parser::tokenize_pub(input);
        if tokens.is_empty() {
            return ConsoleAction::None;
        }

        // Built-in special commands handled here (need registry access)
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
            "echo" => {
                let text = tokens[1..].join(" ");
                self.output.push(OutputLine::Text(text));
                return ConsoleAction::None;
            }
            _ => {}
        }

        // Registry lookup
        let root_name = &tokens[0];
        let root_node = match self.registry.roots.iter().find(|n| n.name == *root_name) {
            Some(n) => n,
            None => {
                self.output.push(OutputLine::Error(format!(
                    "unknown command '{root_name}' — type 'help' for a list"
                )));
                return ConsoleAction::None;
            }
        };

        // Walk subcommands to find deepest matching node
        // We need owned data to avoid borrow issues
        let mut path_indices: Vec<usize> = Vec::new(); // indices into roots/subcommands
        let mut node: &CommandNode = root_node;
        let mut arg_start = 1usize;

        'walk: loop {
            if arg_start >= tokens.len() {
                break;
            }
            for (idx, sub) in node.subcommands.iter().enumerate() {
                if sub.name == tokens[arg_start] {
                    path_indices.push(idx);
                    node = &node.subcommands[path_indices.last().copied().unwrap()];
                    arg_start += 1;
                    continue 'walk;
                }
            }
            break;
        }

        // Re-resolve node (borrow checker) by cloning what we need
        let node_clone = node.clone();
        let raw_args: Vec<String> = tokens[arg_start..].to_vec();

        if node_clone.handler.is_none() {
            let subs: Vec<&str> = node_clone.subcommands.iter().map(|s| s.name.as_str()).collect();
            self.output.push(OutputLine::Error(format!(
                "incomplete command — subcommands: {}",
                subs.join(", ")
            )));
            return ConsoleAction::None;
        }

        match &node_clone.source {
            CommandSource::Engine => {
                let parsed = match parser::parse_args(&raw_args, &node_clone.args) {
                    Ok(p) => p,
                    Err(e) => {
                        self.output.push(OutputLine::Error(e));
                        return ConsoleAction::None;
                    }
                };
                let ctx = CommandContext {
                    mod_registry,
                    vfs,
                    fps: self.fps,
                };
                let handler = node_clone.handler.clone().unwrap();
                match handler(parsed, &ctx) {
                    Ok(lines) => {
                        for line in lines {
                            self.output.push(OutputLine::Text(line));
                        }
                    }
                    Err(e) => self.output.push(OutputLine::Error(e)),
                }
                ConsoleAction::None
            }
            CommandSource::Mod(mod_id) => {
                let request_id = format!(
                    "cmd_{:x}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or(0)
                );
                // Include root name so the script knows which command was invoked
                let command_path: Vec<String> = tokens[0..arg_start].to_vec();
                let args_json: Vec<serde_json::Value> = raw_args
                    .iter()
                    .map(|a| serde_json::Value::String(a.clone()))
                    .collect();

                self.output
                    .push(OutputLine::Text("(waiting for mod response…)".into()));
                self.pending_invoke = Some(PendingInvoke {
                    request_id: request_id.clone(),
                    mod_id: mod_id.clone(),
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
        // Add the special built-ins that aren't in the registry
        let builtins = [
            ("help [command]", "Show this help or details for a command"),
            ("clear", "Clear the console output"),
            ("echo <text…>", "Echo text back to the console"),
        ];

        if let Some(cmd) = command {
            // Try to find in registry
            if let Some(node) = self.registry.roots.iter().find(|n| n.name == cmd) {
                return describe_node(node, &[]);
            }
            // Check builtins
            for (name, desc) in &builtins {
                if name.starts_with(cmd) {
                    return vec![format!("{name}  —  {desc}")];
                }
            }
            return vec![format!("no command named '{cmd}'")];
        }

        let mut lines = vec!["Available commands:".into(), String::new()];
        for (name, desc) in &builtins {
            lines.push(format!("  {name:<28}{desc}"));
        }
        for node in &self.registry.roots {
            if node.subcommands.is_empty() {
                let usage = format_usage(node);
                lines.push(format!("  {usage:<28}{}", node.description));
            } else {
                lines.push(format!("  {:<28}{}", node.name, node.description));
                for sub in &node.subcommands {
                    let usage = format!("{} {}", node.name, format_usage(sub));
                    lines.push(format!("    {usage:<26}{}", sub.description));
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

fn describe_node(node: &CommandNode, path: &[&str]) -> Vec<String> {
    let mut lines = vec![
        format!("  {}", node.description),
        String::new(),
        format!("usage: {} {}", path.join(" "), format_usage(node)).trim().to_owned(),
    ];
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
