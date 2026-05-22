use crate::console::types::{
    ArgSpec, ArgType, ArgValue, CommandContext, CommandNode, CommandResult, ParsedArgs,
};
use crate::ecs::EntityId;
use std::sync::Arc;

pub fn node() -> CommandNode {
    CommandNode::engine("scene", "Inspect scene entities and components").with_subcommands(vec![
        CommandNode::engine("entities", "List all living entity IDs")
            .with_handler(Arc::new(run_entities)),
        CommandNode::engine("inspect", "Show all components on an entity")
            .with_args(vec![ArgSpec {
                name: "entity_id".into(),
                arg_type: ArgType::String,
                required: true,
                description: "Entity ID in index:generation format".into(),
            }])
            .with_handler(Arc::new(run_inspect)),
    ])
}

fn run_entities(_args: ParsedArgs, ctx: &CommandContext) -> CommandResult {
    let ids: Vec<String> = ctx
        .world
        .allocator
        .alive_entity_ids()
        .map(|id| id.to_string())
        .collect();
    if ids.is_empty() {
        return Ok(vec!["(no entities)".into()]);
    }
    Ok(ids)
}

fn run_inspect(args: ParsedArgs, ctx: &CommandContext) -> CommandResult {
    let id_str = match args.positional.first() {
        Some(ArgValue::String(s)) => s.clone(),
        _ => return Err("expected entity_id argument".into()),
    };
    let id = EntityId::parse(&id_str)
        .ok_or_else(|| format!("invalid entity_id '{id_str}' (expected index:generation)"))?;
    if !ctx.world.is_alive(&id) {
        return Err(format!("entity '{id_str}' is not alive"));
    }

    let mut lines = vec![format!("entity {id_str}:")];

    if let Some(t) = ctx.world.transforms.get(&id.index) {
        lines.push(format!(
            "  core:transform  position={:?}  rotation={:?}  scale={:?}",
            t.position, t.rotation, t.scale
        ));
    }
    if let Some(s) = ctx.world.sprite_renderers.get(&id.index) {
        lines.push(format!(
            "  core:sprite_renderer  texture={}  z_index={}",
            s.texture, s.z_index
        ));
    }
    for (comp_type, entities) in &ctx.world.custom {
        if let Some(data) = entities.get(&id.index) {
            lines.push(format!("  {comp_type}  {data}"));
        }
    }

    if lines.len() == 1 {
        lines.push("  (no components)".into());
    }
    Ok(lines)
}
