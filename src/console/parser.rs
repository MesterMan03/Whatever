/// Parse a raw input string into a command path and raw argument tokens.
/// Handles basic quoting: "hello world" is one token.
pub fn parse(input: &str) -> (Vec<String>, Vec<String>) {
    let tokens = tokenize(input);
    if tokens.is_empty() {
        return (Vec::new(), Vec::new());
    }

    // First token is always the root command; subsequent tokens that don't
    // look like positional args could be subcommands — the registry resolves that.
    // We return all tokens and let the registry walk them.
    let mut iter = tokens.into_iter();
    let mut path = vec![iter.next().unwrap()];
    let rest: Vec<String> = iter.collect();
    (path.extend(rest.iter().cloned()), (path, rest)).1
}

pub fn tokenize_pub(input: &str) -> Vec<String> {
    tokenize(input)
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' => in_quotes = !in_quotes,
            ' ' | '\t' if !in_quotes => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Walk the token list against the command tree to split path from args.
/// Returns (subcommand_path_tokens, arg_tokens).
pub fn split_path_and_args(
    tokens: &[String],
    root_subcommands: &[crate::console::types::CommandNode],
) -> (Vec<String>, Vec<String>) {
    if tokens.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut path = vec![tokens[0].clone()];
    let mut current_subs = root_subcommands;
    let mut i = 1;

    while i < tokens.len() {
        if let Some(sub) = current_subs.iter().find(|s| s.name == tokens[i]) {
            path.push(tokens[i].clone());
            current_subs = &sub.subcommands;
            i += 1;
        } else {
            break;
        }
    }

    (path, tokens[i..].to_vec())
}

/// Parse raw arg strings into typed ArgValues according to specs.
pub fn parse_args(
    raw: &[String],
    specs: &[crate::console::types::ArgSpec],
) -> Result<crate::console::types::ParsedArgs, String> {
    use crate::console::types::{ArgType, ArgValue, ParsedArgs};

    let required_count = specs.iter().filter(|s| s.required).count();
    if raw.len() < required_count {
        let missing = specs
            .iter()
            .filter(|s| s.required)
            .nth(raw.len())
            .map(|s| s.name.as_str())
            .unwrap_or("?");
        return Err(format!("missing required argument: <{missing}>"));
    }
    if raw.len() > specs.len() && !specs.is_empty() {
        return Err(format!(
            "too many arguments: expected at most {}, got {}",
            specs.len(),
            raw.len()
        ));
    }

    let mut positional = Vec::new();
    for (i, raw_val) in raw.iter().enumerate() {
        let spec = match specs.get(i) {
            Some(s) => s,
            None => break,
        };
        let val = match spec.arg_type {
            ArgType::String => ArgValue::String(raw_val.clone()),
            ArgType::Int => raw_val
                .parse::<i64>()
                .map(ArgValue::Int)
                .map_err(|_| format!("argument <{}>: expected integer, got '{raw_val}'", spec.name))?,
            ArgType::Float => raw_val
                .parse::<f64>()
                .map(ArgValue::Float)
                .map_err(|_| format!("argument <{}>: expected number, got '{raw_val}'", spec.name))?,
            ArgType::Bool => match raw_val.as_str() {
                "true" | "1" | "yes" => ArgValue::Bool(true),
                "false" | "0" | "no" => ArgValue::Bool(false),
                _ => return Err(format!("argument <{}>: expected bool (true/false), got '{raw_val}'", spec.name)),
            },
        };
        positional.push(val);
    }

    Ok(ParsedArgs { positional })
}
