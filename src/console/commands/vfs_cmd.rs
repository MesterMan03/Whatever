use crate::console::types::{
    ArgSpec, ArgType, ArgValue, CommandContext, CommandNode, CommandResult, ParsedArgs,
};
use crate::vfs::VfsPath;
use std::sync::Arc;

pub fn node() -> CommandNode {
    CommandNode::engine("vfs", "Inspect the virtual filesystem")
        .with_subcommands(vec![list_node(), read_node()])
}

fn list_node() -> CommandNode {
    CommandNode::engine(
        "list",
        "List files for a mod (optionally filtered by prefix)",
    )
    .with_args(vec![
        ArgSpec {
            name: "mod_id".into(),
            arg_type: ArgType::String,
            required: true,
            description: "mod identifier".into(),
            has_suggest: false,
            suggest: None,
        },
        ArgSpec {
            name: "prefix".into(),
            arg_type: ArgType::String,
            required: false,
            description: "path prefix to filter by".into(),
            has_suggest: false,
            suggest: None,
        },
    ])
    .with_handler(Arc::new(run_list))
}

fn read_node() -> CommandNode {
    CommandNode::engine("read", "Read a VFS file (first 50 lines)")
        .with_args(vec![ArgSpec {
            name: "path".into(),
            arg_type: ArgType::String,
            required: true,
            description: "VFS path in mod_id://relative/path form".into(),
            has_suggest: false,
            suggest: None,
        }])
        .with_handler(Arc::new(run_read))
}

fn run_list(args: ParsedArgs, ctx: &CommandContext) -> CommandResult {
    let mod_id = match args.positional.first() {
        Some(ArgValue::String(s)) => s.clone(),
        _ => return Err("expected mod_id argument".into()),
    };
    let prefix = match args.positional.get(1) {
        Some(ArgValue::String(s)) => s.as_str().to_owned(),
        _ => String::new(),
    };

    let entries = ctx
        .vfs
        .list(&mod_id, &prefix)
        .map_err(|e| format!("vfs error: {e}"))?;

    if entries.is_empty() {
        return Ok(vec![format!("(no files found for {mod_id}://)")]);
    }

    let mut lines: Vec<String> = entries.iter().map(|p| format!("{mod_id}://{p}")).collect();
    lines.insert(0, format!("{} file(s):", entries.len()));
    Ok(lines)
}

fn run_read(args: ParsedArgs, ctx: &CommandContext) -> CommandResult {
    let raw_path = match args.positional.first() {
        Some(ArgValue::String(s)) => s.clone(),
        _ => return Err("expected path argument".into()),
    };

    let vfs_path = VfsPath::parse(&raw_path).ok_or_else(|| {
        format!("invalid VFS path '{raw_path}' — expected mod_id://relative/path")
    })?;

    let bytes = ctx
        .vfs
        .read(&vfs_path)
        .map_err(|e| format!("vfs error: {e}"))?;

    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<String> = text.lines().take(50).map(str::to_owned).collect();
    if text.lines().count() > 50 {
        lines.push("... (truncated at 50 lines)".into());
    }
    Ok(lines)
}
