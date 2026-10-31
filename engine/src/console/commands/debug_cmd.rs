use crate::console::types::{CommandContext, CommandNode, CommandResult, ParsedArgs};
use std::sync::Arc;

pub fn node() -> CommandNode {
    CommandNode::engine("debug", "Manage live debug logging").with_subcommands(vec![
        CommandNode::engine("disable", "Disable all debug logging")
            .with_handler(Arc::new(run_disable)),
        CommandNode::engine("all", "Enable all debug logging").with_handler(Arc::new(run_all)),
        CommandNode::engine("window", "Toggle window debug logging")
            .with_handler(Arc::new(run_window)),
        CommandNode::engine("modloader", "Toggle modloader debug logging")
            .with_handler(Arc::new(run_modloader)),
        CommandNode::engine("ipc", "Toggle IPC debug logging").with_handler(Arc::new(run_ipc)),
        CommandNode::engine("vfs", "Toggle VFS debug logging").with_handler(Arc::new(run_vfs)),
        CommandNode::engine("audio", "Toggle audio debug logging")
            .with_handler(Arc::new(run_audio)),
    ])
}

fn state(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

fn run_disable(_: ParsedArgs, ctx: &CommandContext) -> CommandResult {
    ctx.debug.set_window(false);
    ctx.debug.set_modloader(false);
    ctx.debug.set_ipc(false);
    ctx.debug.set_vfs(false);
    ctx.debug.set_audio(false);
    Ok(vec!["all debug logging disabled".into()])
}

fn run_all(_: ParsedArgs, ctx: &CommandContext) -> CommandResult {
    ctx.debug.set_window(true);
    ctx.debug.set_modloader(true);
    ctx.debug.set_ipc(true);
    ctx.debug.set_vfs(true);
    ctx.debug.set_audio(true);
    Ok(vec!["all debug logging enabled".into()])
}

fn run_window(_: ParsedArgs, ctx: &CommandContext) -> CommandResult {
    Ok(vec![format!(
        "window debug: {}",
        state(ctx.debug.toggle_window())
    )])
}

fn run_modloader(_: ParsedArgs, ctx: &CommandContext) -> CommandResult {
    Ok(vec![format!(
        "modloader debug: {}",
        state(ctx.debug.toggle_modloader())
    )])
}

fn run_ipc(_: ParsedArgs, ctx: &CommandContext) -> CommandResult {
    Ok(vec![format!(
        "ipc debug: {}",
        state(ctx.debug.toggle_ipc())
    )])
}

fn run_vfs(_: ParsedArgs, ctx: &CommandContext) -> CommandResult {
    Ok(vec![format!(
        "vfs debug: {}",
        state(ctx.debug.toggle_vfs())
    )])
}

fn run_audio(_: ParsedArgs, ctx: &CommandContext) -> CommandResult {
    Ok(vec![format!(
        "audio debug: {}",
        state(ctx.debug.toggle_audio())
    )])
}
