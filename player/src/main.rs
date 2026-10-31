use anyhow::Context;
use std::env;
use std::sync::{Arc, Mutex};
use whatever_engine::{debug::DebugConfig, engine::Engine, logging, runtime};
use winit::event_loop::EventLoop;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let debug_config = DebugConfig::from_args(&args);
    let cwd = runtime::installation_root()?;

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
