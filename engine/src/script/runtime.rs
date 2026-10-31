use anyhow::{Context, bail};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

pub const HOST_PROTOCOL: u32 = 8;

#[derive(Deserialize)]
struct HostInfo {
    runtime: String,
    protocol: u32,
    native_policy: u32,
    api_version: String,
    bun_version: String,
}

pub fn host_command(path: &std::path::Path) -> Command {
    let mut command = Command::new(path);
    command.env_clear();
    command.env("BUN_DEBUG_QUIET_LOGS", "1");
    for name in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
}

pub fn find_host() -> anyhow::Result<PathBuf> {
    let executable = std::env::current_exe().context("locate engine executable")?;
    let name = format!("whatever-mod-host{}", std::env::consts::EXE_SUFFIX);
    let parent = executable
        .parent()
        .context("engine executable has no parent")?;
    let mut candidates = vec![
        parent.join(format!("runtime{}", std::env::consts::EXE_SUFFIX)),
        parent.join("lib").join(&name),
        parent.join(&name),
    ];
    if cfg!(debug_assertions) {
        candidates.push(crate::development_root().join("runtime/dist").join(&name));
    }
    let Some(path) = candidates.into_iter().find(|path| path.is_file()) else {
        bail!("Bundled mod host is missing. Build the Bun fork, then run bun run build:host.");
    };
    let path = path.canonicalize().context("resolve bundled mod host")?;
    validate_host(&path)?;
    Ok(path)
}

pub fn validate_host(path: &std::path::Path) -> anyhow::Result<()> {
    check_host(path, false)
}
pub fn validate_release_host(path: &std::path::Path) -> anyhow::Result<()> {
    check_host(path, true)
}
fn check_host(path: &std::path::Path, release: bool) -> anyhow::Result<()> {
    let output = host_command(path)
        .arg("--runtime-info")
        .output()
        .context("check bundled mod host")?;
    if !output.status.success() {
        bail!(
            "Bundled mod host self check failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let info: HostInfo =
        serde_json::from_slice(&output.stdout).context("invalid bundled mod host metadata")?;
    let bun = semver::Version::parse(&info.bun_version).context("invalid bundled Bun version")?;
    if release
        && bun
            .pre
            .as_str()
            .split(['.', '-'])
            .any(|part| part == "debug")
    {
        bail!(
            "Deployment requires the release Bun fork; run bun run build:bun then bun run build:host. Debug runtimes are for development only."
        );
    }
    if (bun.major, bun.minor, bun.patch) != (1, 4, 4)
        || info.runtime != "whatever-mod-host"
        || info.protocol != HOST_PROTOCOL
        || info.native_policy != 1
        || info.api_version != env!("CARGO_PKG_VERSION")
    {
        bail!(
            "Bundled mod host is incompatible with this engine build; rebuild it with bun run build:host."
        );
    }
    Ok(())
}
