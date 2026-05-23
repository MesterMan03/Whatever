use crate::console::types::{
    ArgSpec, ArgType, CommandContext, CommandNode, CommandResult, EngineSettingAction, ParsedArgs,
};
use std::sync::Arc;

pub fn node() -> CommandNode {
    CommandNode::engine("engine", "Engine information and settings").with_subcommands(vec![
        CommandNode::engine("version", "Show engine version").with_handler(Arc::new(run_version)),
        CommandNode::engine("fps", "Show current frames per second")
            .with_handler(Arc::new(run_fps)),
        CommandNode::engine("fpscap", "Get or set the FPS cap ('off' to disable)")
            .with_args(vec![ArgSpec {
                name: "fps".into(),
                arg_type: ArgType::String,
                required: false,
                description: "Target FPS or 'off'".into(),
            }])
            .with_handler(Arc::new(run_fps_cap)),
        CommandNode::engine("vsync", "Get or set vertical sync ('on' or 'off')")
            .with_args(vec![ArgSpec {
                name: "state".into(),
                arg_type: ArgType::String,
                required: false,
                description: "'on' or 'off'".into(),
            }])
            .with_handler(Arc::new(run_vsync)),
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

fn run_fps_cap(args: ParsedArgs, ctx: &CommandContext) -> CommandResult {
    if args.positional.is_empty() {
        let current = ctx
            .fps_cap
            .map_or_else(|| "off".to_owned(), |c| format!("{c:.0}"));
        return Ok(vec![format!("fps-cap: {current}")]);
    }
    let raw = args.positional[0].to_string();
    let new_cap = if raw == "off" || raw == "0" {
        None
    } else {
        match raw.parse::<f64>() {
            Ok(n) if n > 0.0 => Some(n),
            _ => {
                return Err(format!(
                    "invalid fps value '{raw}' — use a positive number or 'off'"
                ));
            }
        }
    };
    if let Ok(mut guard) = ctx.pending_action.lock() {
        *guard = Some(EngineSettingAction::SetFpsCap(new_cap));
    }
    let msg = new_cap.map_or_else(
        || "FPS cap disabled".to_owned(),
        |c| format!("FPS cap set to {c:.0}"),
    );
    Ok(vec![msg])
}

fn run_vsync(args: ParsedArgs, ctx: &CommandContext) -> CommandResult {
    if args.positional.is_empty() {
        return Ok(vec![format!(
            "vsync: {}",
            if ctx.vsync { "on" } else { "off" }
        )]);
    }
    let raw = args.positional[0].to_string();
    let enabled = match raw.as_str() {
        "on" | "true" | "1" => true,
        "off" | "false" | "0" => false,
        _ => return Err(format!("invalid value '{raw}' — use 'on' or 'off'")),
    };
    if let Ok(mut guard) = ctx.pending_action.lock() {
        *guard = Some(EngineSettingAction::SetVsync(enabled));
    }
    Ok(vec![format!(
        "vsync {}",
        if enabled { "enabled" } else { "disabled" }
    )])
}
