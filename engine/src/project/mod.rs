mod branding;
mod build;
mod deployment;
mod files;
mod installation;
pub use files::resource_uri;
mod kit;
mod library;
mod library_archive;
#[cfg(test)]
mod library_tests;
pub use library::{AssetLibrary, LibraryImport, LibraryManifest};
mod manifest;
mod paths;
mod scaffold;
mod sdk;
mod starter;
pub use sdk::{bind_sdk, configure_types, installed_sdk};
pub use starter::{import_starter, starter_library};
#[cfg(test)]
mod tests;
#[cfg(test)]
mod workflow_tests;

pub use branding::default_icon_rgba;
pub use build::{build_game, prepare_player};
pub use deployment::{export, launcher_name};
pub use kit::create_runtime_kit;
pub use manifest::{BuildSettings, Project};
pub use paths::{display_path, portable_path};
pub use scaffold::{ModOptions, create_mod, create_project, default_core};
