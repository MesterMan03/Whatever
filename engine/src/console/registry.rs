use crate::console::types::{ArgSpec, ArgType, CommandHandler, CommandNode, CommandSource};
use std::sync::Arc;

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
}

/// Convert an IPC CommandNodeSpec (from script) into a CommandNode tree.
pub fn command_node_from_spec(
    spec: &crate::script::ipc::CommandNodeSpec,
    mod_id: &str,
) -> CommandNode {
    // A sentinel marks that the script side has a handler; it is never called directly.
    let handler: Option<CommandHandler> = if spec.has_handler {
        Some(Arc::new(|_, _| Ok(vec![])))
    } else {
        None
    };
    CommandNode {
        name: spec.name.clone(),
        description: spec.description.clone(),
        subcommands: spec
            .subcommands
            .iter()
            .map(|s| command_node_from_spec(s, mod_id))
            .collect(),
        args: spec
            .args
            .iter()
            .map(|a| ArgSpec {
                name: a.name.clone(),
                arg_type: match a.arg_type.as_str() {
                    "int" => ArgType::Int,
                    "float" => ArgType::Float,
                    "bool" => ArgType::Bool,
                    "mod_id" => ArgType::ModId,
                    "entity_id" => ArgType::EntityId,
                    _ => ArgType::String,
                },
                required: a.required,
                description: a.description.clone(),
                has_suggest: a.has_suggest,
                suggest: None,
            })
            .collect(),
        handler,
        source: CommandSource::Mod(mod_id.to_owned()),
    }
}

pub struct CommandRegistry {
    pub roots: Vec<CommandNode>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        CommandRegistry { roots: Vec::new() }
    }

    /// Built-ins are installed before any mod registrations.
    pub fn register_engine(&mut self, node: CommandNode) {
        self.roots.push(node);
    }

    /// A collision removes the plain name and qualifies every participant.
    /// Two registrations in one namespace cannot be disambiguated.
    pub fn register_mod(&mut self, mod_id: &str, mut node: CommandNode) -> Result<String, String> {
        if !valid_name(&node.name) {
            return Err(format!(
                "mod '{mod_id}' registered invalid command '{}' (only [a-z_] allowed)",
                node.name
            ));
        }

        let plain = node.name.clone();
        let namespaced = format!("{mod_id}:{plain}");

        let conflicts: Vec<_> = self
            .roots
            .iter()
            .enumerate()
            .filter_map(|(index, root)| {
                let name = root
                    .name
                    .split_once(':')
                    .map_or(root.name.as_str(), |(_, name)| name);
                (name == plain).then_some(index)
            })
            .collect();
        for &index in &conflicts {
            let namespace = match &self.roots[index].source {
                CommandSource::Engine => "core",
                CommandSource::Mod(owner) => owner,
            };
            if namespace == mod_id {
                return Err(format!(
                    "command namespace collision: '{namespaced}' is already registered; engine commands use the 'core' namespace"
                ));
            }
        }
        if !conflicts.is_empty() {
            for index in conflicts {
                let root = &mut self.roots[index];
                if !root.name.contains(':') {
                    let namespace = match &root.source {
                        CommandSource::Engine => "core",
                        CommandSource::Mod(owner) => owner,
                    };
                    root.name = format!("{namespace}:{plain}");
                }
            }
            node.name = namespaced;
            tracing::info!(
                command = plain,
                "command collision: all registrations now require a namespace"
            );
        }
        let registered = node.name.clone();
        node.source = CommandSource::Mod(mod_id.to_owned());
        self.roots.push(node);
        Ok(registered)
    }

    #[allow(dead_code)]
    pub fn find(&self, path: &[&str]) -> Option<&CommandNode> {
        if path.is_empty() {
            return None;
        }
        let mut node = self.roots.iter().find(|n| n.name == path[0])?;
        for seg in &path[1..] {
            node = node.subcommands.iter().find(|n| n.name == *seg)?;
        }
        Some(node)
    }

    /// Return completion candidates given a partial path (as string slices).
    /// `partial_path` is the already-complete segments; `partial_last` is what
    /// the user has typed so far for the next segment (may be empty).
    #[allow(dead_code)]
    pub fn completions(&self, partial_path: &[&str], partial_last: &str) -> Vec<String> {
        let candidates: &[CommandNode] = if partial_path.is_empty() {
            &self.roots
        } else {
            match self.find(partial_path) {
                Some(n) => &n.subcommands,
                None => return Vec::new(),
            }
        };
        candidates
            .iter()
            .filter(|n| n.name.starts_with(partial_last))
            .map(|n| n.name.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_collisions_rename_earlier_commands_and_include_later_registrations() {
        for order in [["first", "second", "third"], ["third", "second", "first"]] {
            let mut registry = CommandRegistry::new();
            for (index, owner) in order.iter().enumerate() {
                let node = CommandNode::engine("shared", "")
                    .with_subcommands(vec![CommandNode::engine("child", "")]);
                let name = registry.register_mod(owner, node).expect("distinct owners");
                if index == 0 {
                    assert_eq!(name, "shared");
                } else {
                    assert_eq!(name, format!("{owner}:shared"));
                    assert!(registry.find(&["shared"]).is_none());
                }
            }
            for owner in order {
                let qualified = format!("{owner}:shared");
                assert!(registry.find(&[&qualified, "child"]).is_some());
                assert!(
                    matches!(&registry.find(&[&qualified]).expect("root").source,
                    CommandSource::Mod(id) if id == owner)
                );
            }
        }
    }

    #[test]
    fn same_namespace_collisions_fail_without_mutating_the_registry() {
        for qualify_first in [false, true] {
            let mut registry = CommandRegistry::new();
            registry.register_engine(CommandNode::engine("scene", ""));
            if qualify_first {
                registry
                    .register_mod("debug", CommandNode::engine("scene", ""))
                    .expect("other namespace");
            }
            let names: Vec<_> = registry
                .roots
                .iter()
                .map(|node| node.name.clone())
                .collect();
            let error = registry
                .register_mod("core", CommandNode::engine("scene", ""))
                .expect_err("core shares the engine namespace");
            assert!(error.contains("core:scene"));
            assert_eq!(
                names,
                registry
                    .roots
                    .iter()
                    .map(|node| node.name.clone())
                    .collect::<Vec<_>>()
            );
            registry
                .register_mod("core", CommandNode::engine("unique", ""))
                .expect("core may still add unique commands");
            assert!(
                registry
                    .register_mod("core", CommandNode::engine("unique", ""))
                    .is_err()
            );
        }
    }
}
