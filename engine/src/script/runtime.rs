use anyhow::{Context, bail};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

pub const HOST_PROTOCOL: u32 = 10;

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
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.env_clear();
    command.env("BUN_DEBUG_QUIET_LOGS", "1");
    // The pinned Windows JSC collector can deadlock when parallel markers stop
    // the mutator. Each mod already has its own process; keep marking serial.
    #[cfg(target_os = "windows")]
    command.env("BUN_JSC_numberOfGCMarkers", "1");
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
    let mut candidates = bundled_host_candidates(parent, &name);
    if cfg!(debug_assertions) {
        candidates.push(crate::development_root().join("runtime/dist").join(&name));
    }
    select_host(candidates, cfg!(debug_assertions), validate_host)
}

fn bundled_host_candidates(parent: &std::path::Path, name: &str) -> Vec<PathBuf> {
    vec![
        parent.join(format!("runtime{}", std::env::consts::EXE_SUFFIX)),
        parent
            .join("engine")
            .join(format!("runtime{}", std::env::consts::EXE_SUFFIX)),
        parent.join("lib").join(name),
        parent.join(name),
    ]
}

fn select_host(
    candidates: impl IntoIterator<Item = PathBuf>,
    allow_fallback: bool,
    mut validate: impl FnMut(&std::path::Path) -> anyhow::Result<()>,
) -> anyhow::Result<PathBuf> {
    let mut failures = Vec::new();
    for path in candidates.into_iter().filter(|path| path.is_file()) {
        let checked = path
            .canonicalize()
            .context("resolve bundled mod host")
            .and_then(|path| validate(&path).map(|()| path));
        match checked {
            Ok(path) => return Ok(path),
            Err(error) => {
                failures.push(format!("{}: {error:#}", path.display()));
                if !allow_fallback {
                    break;
                }
            }
        }
    }
    if failures.is_empty() {
        bail!("Bundled mod host is missing. Build the Bun fork, then run bun run build:host.");
    }
    bail!(
        "No compatible bundled mod host was found:\n{}",
        failures.join("\n")
    )
}

pub fn validate_host(path: &std::path::Path) -> anyhow::Result<()> {
    check_host(path, false)
}
pub fn validate_release_host(path: &std::path::Path) -> anyhow::Result<()> {
    check_host(path, true)
}
fn check_host(path: &std::path::Path, release: bool) -> anyhow::Result<()> {
    let output = host_output(
        host_command(path).arg("--runtime-info"),
        "check bundled mod host",
        Duration::from_secs(10),
    )?;
    if !output.status.success() {
        bail!(
            "Bundled mod host self check failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let info: HostInfo =
        serde_json::from_slice(&output.stdout).context("invalid bundled mod host metadata")?;
    validate_host_info(&info, release)
}

fn validate_host_info(info: &HostInfo, release: bool) -> anyhow::Result<()> {
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
            "Bundled mod host is incompatible with this engine build: expected protocol {HOST_PROTOCOL}, API {}, Bun 1.4.4 and native policy 1; found protocol {}, API {}, Bun {} and native policy {}. Rebuild it with bun run build:host.",
            env!("CARGO_PKG_VERSION"),
            info.protocol,
            info.api_version,
            info.bun_version,
            info.native_policy
        );
    }
    Ok(())
}

/// Engine-owned source inspection; the host never evaluates mod code in this mode.
pub fn inspect_components(
    path: &std::path::Path,
    root: &std::path::Path,
    owner: &str,
) -> anyhow::Result<Vec<crate::ecs::ComponentDefinition>> {
    let mut command = host_command(path);
    command.args(["--component-metadata"]).arg(root).arg(owner);
    let output = host_output(
        &mut command,
        &format!("inspect component sources for {owner}"),
        Duration::from_secs(30),
    )?;
    if !output.status.success() {
        anyhow::bail!(
            "Component inspection for {owner} failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let definitions: Vec<crate::ecs::ComponentDefinition> =
        serde_json::from_slice(&output.stdout).context("invalid component metadata")?;
    for definition in &definitions {
        definition.validate(owner)?;
    }
    Ok(definitions)
}

/// Drain both pipes while waiting, and reap a stalled host instead of waiting indefinitely.
fn host_output(
    command: &mut Command,
    operation: &str,
    timeout: Duration,
) -> anyhow::Result<Output> {
    use std::io::Read;

    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| operation.to_owned())?;
    let stdout = child.stdout.take().context("host stdout pipe")?;
    let stderr = child.stderr.take().context("host stderr pipe")?;
    std::thread::scope(|scope| {
        let read = |mut pipe: Box<dyn Read + Send>| -> std::io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            pipe.read_to_end(&mut bytes)?;
            Ok(bytes)
        };
        let stdout = scope.spawn(move || read(Box::new(stdout)));
        let stderr = scope.spawn(move || read(Box::new(stderr)));
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if started.elapsed() >= timeout => {
                    break Err(anyhow::anyhow!(
                        "{operation} timed out after {:.0} seconds",
                        timeout.as_secs_f64()
                    ));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(error) => break Err(anyhow::Error::new(error).context(operation.to_owned())),
            }
        };
        if status.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let stdout = stdout
            .join()
            .map_err(|_| anyhow::anyhow!("host stdout reader stopped"))??;
        let stderr = stderr
            .join()
            .map_err(|_| anyhow::anyhow!("host stderr reader stopped"))??;
        Ok(Output {
            status: status?,
            stdout,
            stderr,
        })
    })
}

pub fn load_component_types(
    world: &mut crate::ecs::World,
    registry: &crate::mods::ModRegistry,
) -> anyhow::Result<()> {
    let scripted: Vec<_> = registry
        .iter()
        .filter(|item| item.manifest.script.is_some())
        .collect();
    if scripted.is_empty() {
        return Ok(());
    }
    let host = find_host()?;
    for item in scripted {
        let owner = &item.manifest.meta.id;
        for definition in inspect_components(&host, &item.root, owner)? {
            world.register_component(owner, definition)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "subprocess fixture for host pipe and timeout tests"]
    fn host_output_fixture() {
        use std::io::Write;
        if std::env::var("WHATEVER_HOST_OUTPUT_FIXTURE").as_deref() == Ok("hang") {
            std::thread::sleep(Duration::from_secs(60));
        } else {
            std::io::stdout()
                .write_all(&vec![b'o'; 128 * 1024])
                .expect("stdout");
            std::io::stderr()
                .write_all(&vec![b'e'; 128 * 1024])
                .expect("stderr");
        }
    }

    fn fixture_command(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command.args([
            "--ignored",
            "--exact",
            "script::runtime::tests::host_output_fixture",
            "--nocapture",
        ]);
        command.env("WHATEVER_HOST_OUTPUT_FIXTURE", mode);
        command
    }

    #[test]
    fn host_output_drains_large_stdout_and_stderr_without_blocking() {
        let output = host_output(
            &mut fixture_command("pipes"),
            "fixture",
            Duration::from_secs(10),
        )
        .expect("capture both pipes");
        assert!(output.status.success());
        assert!(
            output
                .stdout
                .windows(128 * 1024)
                .any(|bytes| bytes.iter().all(|byte| *byte == b'o'))
        );
        assert!(
            output
                .stderr
                .windows(128 * 1024)
                .any(|bytes| bytes.iter().all(|byte| *byte == b'e'))
        );
    }

    #[test]
    fn host_output_terminates_and_reaps_a_stalled_process() {
        let started = Instant::now();
        let error = host_output(
            &mut fixture_command("hang"),
            "fixture",
            Duration::from_millis(200),
        )
        .expect_err("host must time out");
        assert!(error.to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    fn info(protocol: u32) -> HostInfo {
        HostInfo {
            runtime: "whatever-mod-host".into(),
            protocol,
            native_policy: 1,
            api_version: env!("CARGO_PKG_VERSION").into(),
            bun_version: "1.4.4".into(),
        }
    }

    #[test]
    fn development_discovery_skips_stale_hosts_but_release_keeps_its_bundled_host() {
        let directory = std::env::temp_dir().join(format!(
            "whatever-host-discovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).expect("fixture");
        let stale = directory.join("stale-host");
        let current = directory.join("current-host");
        std::fs::write(&stale, (HOST_PROTOCOL - 1).to_string()).expect("stale host");
        std::fs::write(&current, HOST_PROTOCOL.to_string()).expect("current host");
        let check = |path: &std::path::Path| -> anyhow::Result<()> {
            let protocol = std::fs::read_to_string(path)?.parse()?;
            validate_host_info(&info(protocol), false)
        };
        let candidates = || {
            [
                directory.join("missing-host"),
                stale.clone(),
                current.clone(),
            ]
        };
        let selected = select_host(candidates(), true, check).expect("compatible fallback");
        assert_eq!(selected, current.canonicalize().expect("canonical host"));

        let error =
            select_host(candidates(), false, check).expect_err("release rejects stale host");
        let message = error.to_string();
        assert!(message.contains(&stale.display().to_string()));
        assert!(message.contains(&format!("expected protocol {HOST_PROTOCOL}")));
        assert!(message.contains(&format!("found protocol {}", HOST_PROTOCOL - 1)));

        let error = select_host([stale.clone(), stale.clone()], true, check)
            .expect_err("no compatible candidate");
        assert!(error.to_string().contains("No compatible bundled mod host"));
        std::fs::remove_file(stale).expect("remove stale fixture");
        std::fs::remove_file(current).expect("remove current fixture");
        std::fs::remove_dir(directory).expect("remove fixture directory");
    }

    #[test]
    fn installed_tools_find_the_engine_runtime_and_reject_a_stale_adjacent_host() {
        let directory = std::env::temp_dir().join(format!(
            "whatever-installed-host-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let engine = directory.join("engine");
        std::fs::create_dir_all(&engine).expect("fixture");
        let runtime_name = format!("runtime{}", std::env::consts::EXE_SUFFIX);
        let bundled = engine.join(&runtime_name);
        std::fs::write(&bundled, HOST_PROTOCOL.to_string()).expect("bundled host");
        let name = format!("whatever-mod-host{}", std::env::consts::EXE_SUFFIX);
        let candidates = || bundled_host_candidates(&directory, &name);
        let check = |path: &std::path::Path| -> anyhow::Result<()> {
            validate_host_info(&info(std::fs::read_to_string(path)?.parse()?), true)
        };
        assert_eq!(
            select_host(candidates(), false, check).expect("installed runtime"),
            bundled.canonicalize().expect("canonical host")
        );
        let adjacent = directory.join(&runtime_name);
        std::fs::write(&adjacent, (HOST_PROTOCOL - 1).to_string()).expect("stale adjacent host");
        assert!(select_host(candidates(), false, check).is_err());
        assert_eq!(
            select_host(candidates(), true, check).expect("development fallback"),
            bundled.canonicalize().expect("canonical host")
        );
        std::fs::remove_file(adjacent).expect("remove stale host");
        std::fs::remove_file(bundled).expect("remove bundled host");
        std::fs::remove_dir(engine).expect("remove engine directory");
        std::fs::remove_dir(directory).expect("remove fixture");
    }

    #[test]
    fn host_validation_still_requires_every_native_compatibility_field() {
        validate_host_info(&info(HOST_PROTOCOL), true).expect("compatible host");
        let mut wrong_policy = info(HOST_PROTOCOL);
        wrong_policy.native_policy = 2;
        let mut wrong_api = info(HOST_PROTOCOL);
        wrong_api.api_version = "0.0.0".into();
        let mut wrong_runtime = info(HOST_PROTOCOL);
        wrong_runtime.runtime = "bun".into();
        let mut wrong_bun = info(HOST_PROTOCOL);
        wrong_bun.bun_version = "1.4.6".into();
        for rejected in [
            info(HOST_PROTOCOL - 1),
            wrong_policy,
            wrong_api,
            wrong_runtime,
            wrong_bun,
        ] {
            assert!(validate_host_info(&rejected, false).is_err());
        }
        let mut debug = info(HOST_PROTOCOL);
        debug.bun_version = "1.4.4-debug".into();
        assert!(validate_host_info(&debug, false).is_ok());
        assert!(validate_host_info(&debug, true).is_err());
    }
}
