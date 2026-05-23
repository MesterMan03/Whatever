use crate::console::types::{
    ArgSpec, ArgType, ArgValue, CommandContext, CommandNode, CommandResult, ParsedArgs,
};
use std::sync::Arc;

const WIDTH: usize = 80;
const HEIGHT: usize = 32;
const DEFAULT_MAX_ITER: u32 = 128;
// Space = inside set; denser chars = escapes quickly (bright edge)
const CHARS: &[u8] = b" .:-=+*#%@";

pub fn node() -> CommandNode {
    CommandNode::engine(
        "markbench",
        "Mandelbrot benchmark: render ASCII fractal across N threads",
    )
    .with_args(vec![
        ArgSpec {
            name: "threads".into(),
            arg_type: ArgType::Int,
            required: false,
            description: "number of threads (default: all available)".into(),
            has_suggest: false,
            suggest: None,
        },
        ArgSpec {
            name: "max_iter".into(),
            arg_type: ArgType::Int,
            required: false,
            description: format!("max iterations per pixel (default: {DEFAULT_MAX_ITER})"),
            has_suggest: false,
            suggest: None,
        },
    ])
    .with_handler(Arc::new(run))
}

#[inline(always)]
fn mandelbrot(cx: f64, cy: f64, max_iter: u32) -> u32 {
    let (mut zx, mut zy) = (0.0f64, 0.0f64);
    for i in 0..max_iter {
        let zx2 = zx * zx;
        let zy2 = zy * zy;
        if zx2 + zy2 > 4.0 {
            return i;
        }
        zy = 2.0 * zx * zy + cy;
        zx = zx2 - zy2 + cx;
    }
    max_iter
}

fn run(args: ParsedArgs, _ctx: &CommandContext) -> CommandResult {
    let default_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let thread_count: usize = match args.positional.first() {
        Some(ArgValue::Int(n)) if *n >= 1 => *n as usize,
        Some(ArgValue::Int(n)) => return Err(format!("threads must be >= 1, got {n}")),
        Some(_) => return Err("threads must be an integer".into()),
        None => default_threads,
    };
    let max_iter: u32 = match args.positional.get(1) {
        Some(ArgValue::Int(n)) if *n >= 1 => *n as u32,
        Some(ArgValue::Int(n)) => return Err(format!("max_iter must be >= 1, got {n}")),
        Some(_) => return Err("max_iter must be an integer".into()),
        None => DEFAULT_MAX_ITER,
    };

    // Region: x [-2.5, 1.0], y adjusted so chars look square (chars are ~2× taller than wide)
    let x_min = -2.5f64;
    let x_max = 1.0f64;
    let y_min = -0.9f64;
    let y_max = 0.9f64;

    let rows_per_thread = HEIGHT.div_ceil(thread_count);
    let start = std::time::Instant::now();

    let handles: Vec<_> = (0..thread_count)
        .filter_map(|t| {
            let row_start = t * rows_per_thread;
            if row_start >= HEIGHT {
                return None;
            }
            let row_end = (row_start + rows_per_thread).min(HEIGHT);
            Some(std::thread::spawn(move || {
                let mut rows = Vec::with_capacity(row_end - row_start);
                for row in row_start..row_end {
                    let cy = y_min + (row as f64 / (HEIGHT - 1) as f64) * (y_max - y_min);
                    let mut line = String::with_capacity(WIDTH);
                    for col in 0..WIDTH {
                        let cx = x_min + (col as f64 / (WIDTH - 1) as f64) * (x_max - x_min);
                        let iters = mandelbrot(cx, cy, max_iter);
                        let ch = if iters == max_iter {
                            b' '
                        } else {
                            let idx = (iters as usize * (CHARS.len() - 1)) / max_iter as usize;
                            CHARS[idx + 1]
                        };
                        line.push(ch as char);
                    }
                    rows.push((row, line));
                }
                rows
            }))
        })
        .collect();

    let actual_threads = handles.len();
    let mut grid: Vec<Option<String>> = (0..HEIGHT).map(|_| None).collect();
    for handle in handles {
        let rows = handle
            .join()
            .map_err(|_| "worker thread panicked".to_owned())?;
        for (idx, line) in rows {
            grid[idx] = Some(line);
        }
    }

    let elapsed = start.elapsed();

    let mut output: Vec<String> = grid.into_iter().flatten().collect();
    output.push(String::new());
    output.push(format!(
        "{}×{}  max_iter={}  threads={}  time={:.3}s",
        WIDTH,
        HEIGHT,
        max_iter,
        actual_threads,
        elapsed.as_secs_f64()
    ));

    Ok(output)
}
