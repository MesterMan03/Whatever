use super::parser::tokenize_pub;
use crate::console::types::CommandNode;

/// Given the current input string and the registered root commands, return
/// a list of completion candidates (full replacement strings for the input).
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

    // Walk the tree following complete tokens
    let mut current_subs = roots;
    let mut current_node: Option<&CommandNode> = None;
    let complete_tokens = if trailing_space {
        &tokens[..]
    } else {
        &tokens[..tokens.len() - 1]
    };

    for tok in complete_tokens {
        if let Some(node) = current_subs.iter().find(|n| n.name == tok.as_str()) {
            current_node = Some(node);
            current_subs = &node.subcommands;
        } else {
            return Vec::new();
        }
    }

    let prefix_already = tokens[..tokens
        .len()
        .saturating_sub(if trailing_space { 0 } else { 1 })]
        .join(" ");

    let suggestions: Vec<String> = if trailing_space {
        if current_subs.is_empty() {
            current_node
                .map(|n| n.args.iter().map(|a| format!("<{}>", a.name)).collect())
                .unwrap_or_default()
        } else {
            current_subs.iter().map(|n| n.name.clone()).collect()
        }
    } else {
        let partial = tokens.last().unwrap();
        if current_subs.is_empty() {
            current_node
                .map(|n| n.args.iter().map(|a| format!("<{}>", a.name)).collect())
                .unwrap_or_default()
        } else {
            current_subs
                .iter()
                .filter(|n| n.name.starts_with(partial.as_str()))
                .map(|n| n.name.clone())
                .collect()
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
