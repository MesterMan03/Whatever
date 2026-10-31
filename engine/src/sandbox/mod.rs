mod paths;
pub use paths::{resolve_contained, valid_storage_id};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

mod policy;
pub use policy::{SandboxConfig, SandboxGuard, apply_post_spawn, apply_pre_spawn};
