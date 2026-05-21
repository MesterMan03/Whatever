use std::path::PathBuf;
use std::process::{Child, Command};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

pub struct SandboxConfig {
    pub mod_id: String,
    /// Mod's directory tree — read-only inside the sandbox.
    pub mod_root: PathBuf,
    /// Mod's persistent data dir — the only path with write access.
    pub mod_data_dir: PathBuf,
    /// Engine working directory (contains node_modules/@whatever/api).
    pub engine_root: PathBuf,
}

/// Opaque handle that must live as long as the ScriptProcess it protects.
/// On Windows this wraps the Job Object HANDLE; elsewhere it is a zero-size guard.
#[cfg(target_os = "windows")]
pub use self::windows::SandboxGuard;

#[cfg(not(target_os = "windows"))]
pub struct SandboxGuard;

/// Install pre-exec sandbox restrictions on the command.
/// Must be called before `Command::spawn`. Runs entirely in the parent process
/// but installs hooks that execute in the forked child before exec.
pub fn apply_pre_spawn(cmd: &mut Command, cfg: &SandboxConfig) -> anyhow::Result<()> {
    do_apply_pre_spawn(cmd, cfg)
}

/// Apply post-spawn restrictions (e.g. Job Objects on Windows).
/// Returns a guard that must be kept alive for the duration of the child process.
pub fn apply_post_spawn(child: &Child, cfg: &SandboxConfig) -> anyhow::Result<SandboxGuard> {
    do_apply_post_spawn(child, cfg)
}

// --- platform dispatch ---

#[cfg(target_os = "linux")]
fn do_apply_pre_spawn(cmd: &mut Command, cfg: &SandboxConfig) -> anyhow::Result<()> {
    linux::apply_pre_spawn(cmd, cfg)
}

#[cfg(target_os = "macos")]
fn do_apply_pre_spawn(cmd: &mut Command, cfg: &SandboxConfig) -> anyhow::Result<()> {
    macos::apply_pre_spawn(cmd, cfg)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn do_apply_pre_spawn(cmd: &mut Command, cfg: &SandboxConfig) -> anyhow::Result<()> {
    let _ = (cmd, cfg);
    tracing::debug!(mod_id = %cfg.mod_id, "sandbox: no pre-spawn isolation on this platform");
    Ok(())
}

#[cfg(target_os = "windows")]
fn do_apply_post_spawn(child: &Child, cfg: &SandboxConfig) -> anyhow::Result<SandboxGuard> {
    windows::apply_post_spawn(child, cfg)
}

#[cfg(not(target_os = "windows"))]
fn do_apply_post_spawn(child: &Child, cfg: &SandboxConfig) -> anyhow::Result<SandboxGuard> {
    let _ = (child, cfg);
    Ok(SandboxGuard)
}
