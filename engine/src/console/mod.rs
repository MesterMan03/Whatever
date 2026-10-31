mod commands;
mod completer;
mod parser;
pub(crate) mod registry;
mod types;
mod widget;

pub use registry::command_node_from_spec;
pub use types::CommandSource;
pub use widget::{ConsoleAction, DevConsole};
