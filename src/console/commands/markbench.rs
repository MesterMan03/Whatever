use crate::console::types::{ArgSpec, ArgType, ArgValue, CommandContext, CommandNode, CommandResult, ParsedArgs};
use std::sync::Arc;

pub fn node() -> CommandNode {
    CommandNode::engine("markbench", "Benchmark: sum 1..=1,000,000,000,000 across N threads")
        .with_args(vec![ArgSpec {
            name: "thread_count".into(),
            arg_type: ArgType::Int,
            required: false,
            description: "number of threads (default: all available)".into(),
        }])
        .with_handler(Arc::new(run))
}

fn run(args: ParsedArgs, _ctx: &CommandContext) -> CommandResult {
    let default_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let thread_count: usize = match args.positional.first() {
        Some(ArgValue::Int(n)) if *n >= 1 => *n as usize,
        Some(ArgValue::Int(n)) => return Err(format!("thread_count must be >= 1, got {n}")),
        Some(_) => return Err("thread_count must be an integer".into()),
        None => default_threads,
    };

    let n = 1_000_000_000_000u128;
    let chunk = n / thread_count as u128;
    let start = std::time::Instant::now();

    let handles: Vec<_> = (0..thread_count)
        .map(|i| {
            let lo = i as u128 * chunk + 1;
            let hi = if i + 1 == thread_count { n } else { lo + chunk - 1 };
            std::thread::spawn(move || (lo..=hi).fold(0u128, u128::wrapping_add))
        })
        .collect();

    let total: u128 = handles
        .into_iter()
        .filter_map(|h| h.join().ok())
        .fold(0u128, u128::wrapping_add);

    let elapsed = start.elapsed();
    Ok(vec![
        format!("sum(1..={n}) = {total}"),
        format!(
            "time: {:.3}s  threads: {thread_count}",
            elapsed.as_secs_f64()
        ),
    ])
}
