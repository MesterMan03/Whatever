use std::sync::Arc;

pub enum OutputLine {
    Input(String),
    Text(String),
    Error(String),
    Debug(String),
}

#[derive(Debug, Clone)]
pub enum ArgType {
    String,
    Int,
    Float,
    Bool,
}

#[derive(Debug, Clone)]
pub struct ArgSpec {
    pub name: String,
    pub arg_type: ArgType,
    pub required: bool,
    pub description: String,
}

#[derive(Debug, Clone)]
pub enum ArgValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl std::fmt::Display for ArgValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArgValue::String(s) => write!(f, "{s}"),
            ArgValue::Int(n) => write!(f, "{n}"),
            ArgValue::Float(n) => write!(f, "{n}"),
            ArgValue::Bool(b) => write!(f, "{b}"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ParsedArgs {
    pub positional: Vec<ArgValue>,
}

pub type CommandResult = Result<Vec<String>, String>;

pub struct CommandContext<'a> {
    pub mod_registry: &'a crate::mods::ModRegistry,
    pub vfs: &'a dyn crate::vfs::Vfs,
    pub fps: f32,
}

pub type CommandHandler =
    Arc<dyn Fn(ParsedArgs, &CommandContext) -> CommandResult + Send + Sync>;

#[derive(Clone)]
pub enum CommandSource {
    Engine,
    Mod(String),
}

#[derive(Clone)]
pub struct CommandNode {
    pub name: String,
    pub description: String,
    pub subcommands: Vec<CommandNode>,
    pub args: Vec<ArgSpec>,
    pub handler: Option<CommandHandler>,
    pub source: CommandSource,
}

impl CommandNode {
    pub fn engine(name: impl Into<String>, description: impl Into<String>) -> Self {
        CommandNode {
            name: name.into(),
            description: description.into(),
            subcommands: Vec::new(),
            args: Vec::new(),
            handler: None,
            source: CommandSource::Engine,
        }
    }

    pub fn with_subcommands(mut self, subs: Vec<CommandNode>) -> Self {
        self.subcommands = subs;
        self
    }

    pub fn with_args(mut self, args: Vec<ArgSpec>) -> Self {
        self.args = args;
        self
    }

    pub fn with_handler(mut self, h: CommandHandler) -> Self {
        self.handler = Some(h);
        self
    }
}
