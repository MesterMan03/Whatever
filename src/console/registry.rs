use crate::console::types::{ArgSpec, ArgType, CommandNode, CommandSource};

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
}

/// Convert an IPC CommandNodeSpec (from script) into a CommandNode tree.
pub fn command_node_from_spec(
    spec: &crate::script::ipc::CommandNodeSpec,
    mod_id: &str,
) -> CommandNode {
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
                    _ => ArgType::String,
                },
                required: a.required,
                description: a.description.clone(),
            })
            .collect(),
        handler: None,
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

    /// Register an engine built-in. Always succeeds under its plain name.
    pub fn register_engine(&mut self, node: CommandNode) {
        self.roots.push(node);
    }

    /// Register a mod command. Returns the final registered name (may be namespaced).
    pub fn register_mod(&mut self, mod_id: &str, mut node: CommandNode) -> Option<String> {
        if !valid_name(&node.name) {
            tracing::warn!(
                mod_id,
                name = node.name,
                "mod tried to register command with invalid name (only [a-z_] allowed)"
            );
            return None;
        }

        let plain = node.name.clone();
        let namespaced = format!("{mod_id}:{plain}");

        if !self.roots.iter().any(|r| r.name == plain) {
            node.source = CommandSource::Mod(mod_id.to_owned());
            self.roots.push(node);
            return Some(plain);
        }

        // Plain name taken — try namespaced form
        if self.roots.iter().any(|r| r.name == namespaced) {
            tracing::warn!(mod_id, name = plain, "command name conflict and namespaced form also taken; skipping");
            return None;
        }

        tracing::warn!(
            mod_id,
            "command '{plain}' conflicts; registering as '{namespaced}'"
        );
        node.name = namespaced.clone();
        node.source = CommandSource::Mod(mod_id.to_owned());
        self.roots.push(node);
        Some(namespaced)
    }

    /// Walk the path (first element = root command name, rest = subcommand names).
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
