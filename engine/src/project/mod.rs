mod branding;
mod build;
mod deployment;
mod kit;
mod manifest;
mod paths;
mod scaffold;
#[cfg(test)]
mod tests;

pub use branding::default_icon_rgba;
pub use build::{build_game, prepare_player};
pub use deployment::{export, launcher_name};
pub use kit::create_runtime_kit;
pub use manifest::{BuildSettings, Project};
pub use paths::{display_path, portable_path};
pub use scaffold::{ModOptions, create_mod, create_project, default_core};
