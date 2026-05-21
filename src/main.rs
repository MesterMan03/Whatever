mod console;
mod debug;
mod engine;
mod input;
mod mods;
mod renderer;
mod sandbox;
mod script;
mod vfs;

use anyhow::Context;
use debug::DebugConfig;
use engine::Engine;
use std::env;
use winit::event_loop::EventLoop;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stdout)
        .init();

    let args: Vec<String> = env::args().collect();
    let debug_config = DebugConfig::from_args(&args);
    let cwd = env::current_dir().context("get cwd")?;

    let mut engine = Engine::new(&debug_config, &cwd)?;

    let event_loop = EventLoop::new().context("create event loop")?;
    event_loop.run_app(&mut engine).context("event loop")?;

    Ok(())
}
