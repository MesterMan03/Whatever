use crate::console::types::CommandNode;

/// Given the current input string and the registered root commands, return
/// a list of completion candidates (full replacement strings for the input).
pub fn complete(input: &str, roots: &[CommandNode]) -> Vec<String> {
    let tokens = super::parser::tokenize_pub(input);
    let trailing_space = input.ends_with(' ') || input.ends_with('\t');

    if tokens.is_empty() {
        // suggest all root commands
        return roots.iter().map(|n| n.name.clone()).collect();
    }

    if tokens.len() == 1 && !trailing_space {
        // completing the root command name
        let partial = &tokens[0];
        return roots
            .iter()
            .filter(|n| n.name.starts_with(partial.as_str()))
            .map(|n| n.name.clone())
            .collect();
    }

    // Walk the tree following complete tokens
    let mut current_node: Option<&CommandNode> = None;
    let mut current_subs = roots;
    let complete_tokens = if trailing_space { &tokens[..] } else { &tokens[..tokens.len() - 1] };

    for tok in complete_tokens {
        if let Some(node) = current_subs.iter().find(|n| n.name == tok.as_str()) {
            current_node = Some(node);
            current_subs = &node.subcommands;
        } else {
            // token doesn't match — no completions
            return Vec::new();
        }
    }

    let prefix_already = tokens[..tokens.len().saturating_sub(if trailing_space { 0 } else { 1 })]
        .join(" ");

    if trailing_space {
        // suggest subcommands or arg names of current_node
        let suggestions: Vec<String> = if current_subs.is_empty() {
            // no subcommands; suggest arg placeholders
            current_node
                .map(|n| n.args.iter().map(|a| format!("<{}>", a.name)).collect())
                .unwrap_or_default()
        } else {
            current_subs.iter().map(|n| n.name.clone()).collect()
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
    } else {
        // completing the last (partial) token
        let partial = tokens.last().unwrap();
        let candidates: Vec<String> = if current_subs.is_empty() {
            current_node
                .map(|n| n.args.iter().map(|a| format!("<{}>", a.name)).collect())
                .unwrap_or_default()
        } else {
            current_subs
                .iter()
                .filter(|n| n.name.starts_with(partial.as_str()))
                .map(|n| n.name.clone())
                .collect()
        };
        candidates
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
}

/// Shared tokenizer export so completer can use it without re-implementing.
pub fn tokenize_pub(input: &str) -> Vec<String> {
    super::parser::tokenize_pub(input)
}
