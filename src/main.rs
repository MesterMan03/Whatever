mod debug;
mod engine;
mod input;
mod mods;
mod renderer;
mod script;
mod vfs;

use std::env;
use std::path::PathBuf;
use anyhow::Context;
use winit::event_loop::EventLoop;
use debug::DebugConfig;
use engine::Engine;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = env::args().collect();
    let debug_config = DebugConfig::from_args(&args);
    let cwd = PathBuf::from(env::current_dir().context("get cwd")?);

    let mut engine = Engine::new(&debug_config, &cwd)?;

    let event_loop = EventLoop::new().context("create event loop")?;
    event_loop.run_app(&mut engine).context("event loop")?;

    Ok(())
}
