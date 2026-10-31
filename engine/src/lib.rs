pub mod audio;
pub mod console;
pub mod cursor;
pub mod debug;
pub mod ecs;
pub mod engine;
pub mod input;
pub mod logging;
pub mod mods;
pub mod network;
pub mod physics;
mod presentation;
pub mod project;
pub mod renderer;
pub mod runtime;
pub mod sandbox;
pub mod scene;
pub mod script;
pub mod vfs;
pub mod watchdog;

/// Source checkout root, used only for development defaults and integration fixtures.
pub fn development_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}
