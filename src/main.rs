mod audio;
mod console;
mod debug;
mod ecs;
mod engine;
mod input;
mod logging;
mod mods;
mod network;
mod renderer;
mod runtime;
mod sandbox;
mod script;
mod vfs;
mod watchdog;

use anyhow::Context;
use debug::DebugConfig;
use engine::Engine;
use std::env;
use std::sync::{Arc, Mutex};
use winit::event_loop::EventLoop;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let debug_config = DebugConfig::from_args(&args);
    let cwd = env::current_dir().context("get cwd")?;

    let options = runtime::LaunchOptions::parse(&args)?;

    let log_mirror = Arc::new(Mutex::new(Vec::new()));
    let log_writer = logging::init(&cwd, Arc::clone(&log_mirror))?;

    let game = runtime::LoadedGame::load(&options, &debug_config, &cwd, log_writer)?;
    if game.mode == runtime::RuntimeMode::Server {
        return runtime::run_server(game);
    }
    let mut engine = Engine::new(game, log_mirror)?;

    let event_loop = EventLoop::new().context("create event loop")?;
    event_loop.run_app(&mut engine).context("event loop")?;

    Ok(())
}
