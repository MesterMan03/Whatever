use crate::console::types::{CommandContext, CommandNode, CommandResult, ParsedArgs};
use std::sync::Arc;

pub fn node() -> CommandNode {
    CommandNode::engine("engine", "Engine information").with_subcommands(vec![
        CommandNode::engine("version", "Show engine version").with_handler(Arc::new(run_version)),
        CommandNode::engine("fps", "Show current frames per second")
            .with_handler(Arc::new(run_fps)),
    ])
}

fn run_version(_args: ParsedArgs, _ctx: &CommandContext) -> CommandResult {
    Ok(vec![format!(
        "Whatever engine v{}",
        env!("CARGO_PKG_VERSION")
    )])
}

fn run_fps(_args: ParsedArgs, ctx: &CommandContext) -> CommandResult {
    Ok(vec![format!("{:.1} fps", ctx.fps)])
}
