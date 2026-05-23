use crate::console::types::{
    ArgSpec, ArgType, ArgValue, CommandContext, CommandNode, CommandResult, ParsedArgs,
};
use std::sync::Arc;

pub fn node() -> CommandNode {
    CommandNode::engine("mods", "Query the mod registry")
        .with_subcommands(vec![list_node(), get_node()])
}

fn list_node() -> CommandNode {
    CommandNode::engine("list", "List all loaded mods").with_handler(Arc::new(run_list))
}

fn get_node() -> CommandNode {
    CommandNode::engine("get", "Show details for a specific mod")
        .with_args(vec![ArgSpec {
            name: "mod_id".into(),
            arg_type: ArgType::String,
            required: true,
            description: "mod identifier".into(),
            has_suggest: false,
            suggest: None,
        }])
        .with_handler(Arc::new(run_get))
}

fn run_list(_args: ParsedArgs, ctx: &CommandContext) -> CommandResult {
    let mut lines = vec![format!("{:<20} {:<20} {}", "ID", "NAME", "VERSION")];
    lines.push("-".repeat(52));
    for m in ctx.mod_registry.iter() {
        lines.push(format!(
            "{:<20} {:<20} {}",
            m.manifest.meta.id, m.manifest.meta.name, m.manifest.meta.version
        ));
    }
    Ok(lines)
}

fn run_get(args: ParsedArgs, ctx: &CommandContext) -> CommandResult {
    let id = match args.positional.first() {
        Some(ArgValue::String(s)) => s.clone(),
        _ => return Err("expected mod_id argument".into()),
    };

    let m = ctx
        .mod_registry
        .get(&id)
        .ok_or_else(|| format!("mod '{id}' not found"))?;

    let meta = &m.meta;
    let mut lines = vec![
        format!("id:          {}", meta.id),
        format!("name:        {}", meta.name),
        format!("version:     {}", meta.version),
        format!("description: {}", meta.description),
    ];
    if !meta.authors.is_empty() {
        lines.push(format!("authors:     {}", meta.authors.join(", ")));
    }
    if !meta.license.is_empty() {
        lines.push(format!("license:     {}", meta.license));
    }
    if !m.dependencies.is_empty() {
        lines.push("dependencies:".into());
        for (dep, ver) in &m.dependencies {
            lines.push(format!("  {dep} {ver}"));
        }
    }
    if let Some(ref script) = m.script {
        lines.push(format!("script:      {}", script.entry));
    }
    Ok(lines)
}
