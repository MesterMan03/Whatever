mod audio;
mod console;
mod debug;
mod ecs;
mod engine;
mod input;
mod logging;
mod mods;
mod renderer;
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

    // --mods <path>: load mods exclusively from the given directory.
    let mods_override: Option<std::path::PathBuf> = {
        let mut it = args.iter().peekable();
        let mut found = None;
        while let Some(arg) = it.next() {
            if arg == "--mods" {
                if let Some(val) = it.next() {
                    found = Some(std::path::PathBuf::from(val));
                }
            } else if let Some(val) = arg.strip_prefix("--mods=") {
                found = Some(std::path::PathBuf::from(val));
            }
        }
        found
    };

    let log_mirror = Arc::new(Mutex::new(Vec::new()));
    let log_writer = logging::init(&cwd, Arc::clone(&log_mirror))?;

    let mut engine = Engine::new(&debug_config, &cwd, mods_override.as_deref(), log_mirror, log_writer)?;

    let event_loop = EventLoop::new().context("create event loop")?;
    event_loop.run_app(&mut engine).context("event loop")?;

    Ok(())
}
