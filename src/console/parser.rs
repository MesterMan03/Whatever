/// Tokenize an input string, respecting double-quoted groups.
pub fn tokenize_pub(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for c in input.chars() {
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
    if !specs.is_empty() && raw.len() > specs.len() {
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
                _ => {
                    return Err(format!(
                        "argument <{}>: expected bool (true/false), got '{raw_val}'",
                        spec.name
                    ))
                }
            },
        };
        positional.push(val);
    }

    Ok(ParsedArgs { positional })
}
