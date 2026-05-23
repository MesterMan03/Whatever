use super::parser::tokenize_pub;
use crate::console::types::{CommandNode, CommandSource};

/// Context for firing an async (mod-side) arg suggestion IPC request.
pub struct SuggestContext {
    pub mod_id: String,
    pub command_path: Vec<String>,
    pub arg_index: usize,
    pub current: String,
    /// The portion of the input that precedes the argument being completed.
    /// Each suggestion returned by the mod must be prefixed with this to form
    /// a full replacement string for `input_buf`.
    pub prefix: String,
}

/// Given the current input and the registered root commands, return
/// synchronous completion candidates (full replacement strings for the input).
///
/// For engine commands with a `suggest` fn, it is called to produce candidates.
/// For mod commands with `has_suggest`, no candidates are returned here —
/// call [`arg_suggest_context`] instead to fire the IPC request.
pub fn complete(input: &str, roots: &[CommandNode]) -> Vec<String> {
    let tokens = tokenize_pub(input);
    let trailing_space = input.ends_with(' ') || input.ends_with('\t');

    if tokens.is_empty() {
        return roots.iter().map(|n| n.name.clone()).collect();
    }

    if tokens.len() == 1 && !trailing_space {
        let partial = &tokens[0];
        return roots
            .iter()
            .filter(|n| n.name.starts_with(partial.as_str()))
            .map(|n| n.name.clone())
            .collect();
    }

    let (current_node, cmd_path_len) = walk_tree(&tokens, trailing_space, roots);

    let complete_tokens = if trailing_space {
        &tokens[..]
    } else {
        &tokens[..tokens.len() - 1]
    };

    let prefix_already =
        tokens[..tokens.len().saturating_sub(if trailing_space { 0 } else { 1 })].join(" ");

    let all_consumed = cmd_path_len == complete_tokens.len();

    let suggestions: Vec<String> = if trailing_space {
        if all_consumed {
            if let Some(node) = current_node {
                if !node.subcommands.is_empty() {
                    node.subcommands.iter().map(|n| n.name.clone()).collect()
                } else {
                    let arg_index = complete_tokens.len() - cmd_path_len;
                    sync_arg_suggestions(node, arg_index, "")
                }
            } else {
                Vec::new()
            }
        } else if let Some(node) = current_node {
            let arg_index = complete_tokens.len() - cmd_path_len;
            sync_arg_suggestions(node, arg_index, "")
        } else {
            Vec::new()
        }
    } else {
        let partial = tokens.last().unwrap();
        if all_consumed {
            if let Some(node) = current_node {
                if !node.subcommands.is_empty() {
                    node.subcommands
                        .iter()
                        .filter(|n| n.name.starts_with(partial.as_str()))
                        .map(|n| n.name.clone())
                        .collect()
                } else {
                    let arg_index = complete_tokens.len() - cmd_path_len;
                    sync_arg_suggestions(node, arg_index, partial)
                }
            } else {
                Vec::new()
            }
        } else if let Some(node) = current_node {
            let arg_index = complete_tokens.len() - cmd_path_len;
            sync_arg_suggestions(node, arg_index, partial)
        } else {
            Vec::new()
        }
    };

    suggestions
        .into_iter()
        .map(|s| {
            if prefix_already.is_empty() {
                s
            } else {
                format!("{prefix_already} {s}")
            }
        })
        .collect()
}

/// Returns the context needed to fire an async arg suggestion IPC request,
/// or `None` if the current input is not at a mod-command argument position
/// with a registered `suggest` fn.
pub fn arg_suggest_context(input: &str, roots: &[CommandNode]) -> Option<SuggestContext> {
    let tokens = tokenize_pub(input);
    let trailing_space = input.ends_with(' ') || input.ends_with('\t');

    if tokens.is_empty() {
        return None;
    }

    let complete_tokens = if trailing_space {
        &tokens[..]
    } else {
        &tokens[..tokens.len().saturating_sub(1)]
    };

    let (current_node, cmd_path_len) = walk_tree(&tokens, trailing_space, roots);
    let current_node = current_node?;

    let all_consumed = cmd_path_len == complete_tokens.len();

    // If still in subcommand position, no arg suggest needed.
    if all_consumed && !current_node.subcommands.is_empty() {
        return None;
    }

    let arg_index = complete_tokens.len() - cmd_path_len;
    let current = if trailing_space {
        String::new()
    } else {
        tokens.last().cloned().unwrap_or_default()
    };

    let arg_spec = current_node.args.get(arg_index)?;
    if !arg_spec.has_suggest {
        return None;
    }

    let mod_id = match &current_node.source {
        CommandSource::Mod(id) => id.clone(),
        CommandSource::Engine => return None,
    };

    // Build command path (all tokens consumed as the command path).
    let command_path = tokens[..cmd_path_len].to_vec();

    // Prefix = everything up to (but not including) the partial token being completed.
    let prefix = tokens[..tokens.len().saturating_sub(if trailing_space { 0 } else { 1 })].join(" ");

    Some(SuggestContext {
        mod_id,
        command_path,
        arg_index,
        current,
        prefix,
    })
}

/// Walk the command tree consuming tokens as command/subcommand names.
/// Returns (deepest matched node, number of tokens consumed).
fn walk_tree<'a>(
    tokens: &[String],
    trailing_space: bool,
    roots: &'a [CommandNode],
) -> (Option<&'a CommandNode>, usize) {
    let complete_tokens = if trailing_space {
        &tokens[..]
    } else {
        &tokens[..tokens.len().saturating_sub(1)]
    };

    let mut current_subs = roots;
    let mut current_node: Option<&CommandNode> = None;
    let mut cmd_path_len = 0;

    for tok in complete_tokens {
        if let Some(node) = current_subs.iter().find(|n| n.name == tok.as_str()) {
            current_node = Some(node);
            current_subs = &node.subcommands;
            cmd_path_len += 1;
        } else {
            break;
        }
    }

    (current_node, cmd_path_len)
}

/// Return synchronous suggestions for the arg at `arg_index` given the partial `current`.
/// Returns a `<name>` placeholder for args without a suggest fn, nothing for mod has_suggest.
fn sync_arg_suggestions(node: &CommandNode, arg_index: usize, current: &str) -> Vec<String> {
    if let Some(arg_spec) = node.args.get(arg_index) {
        if let Some(ref suggest_fn) = arg_spec.suggest {
            let results = suggest_fn(current);
            if current.is_empty() {
                results
            } else {
                results
                    .into_iter()
                    .filter(|s| s.starts_with(current))
                    .collect()
            }
        } else if arg_spec.has_suggest {
            // Mod async suggest — widget fires IPC, nothing to show synchronously.
            Vec::new()
        } else {
            vec![format!("<{}>", arg_spec.name)]
        }
    } else {
        Vec::new()
    }
}
