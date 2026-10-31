use super::{
    Project,
    deployment::{RuntimeFiles, export_files},
};
use anyhow::{Context, ensure};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn run(
    command: &mut Command,
    description: &str,
    progress: &mut impl FnMut(String),
) -> anyhow::Result<()> {
    progress(description.into());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| description.to_owned())?;
    if let Some(stderr) = child.stderr.take() {
        use std::io::BufRead;
        for line in std::io::BufReader::new(stderr).lines() {
            progress(line?);
        }
    }
    ensure!(
        child.wait()?.success(),
        "{description} failed; see the build log"
    );
    Ok(())
}

fn installed() -> anyhow::Result<Option<RuntimeFiles>> {
    let executable = std::env::current_exe()?;
    let parent = executable
        .parent()
        .context("editor installation directory")?;
    let files = RuntimeFiles::from_bundle(parent);
    Ok(files.player.is_file().then_some(files))
}

fn prepare(release: bool, progress: &mut impl FnMut(String)) -> anyhow::Result<RuntimeFiles> {
    if let Some(files) = installed()? {
        files.validate()?;
        if release {
            crate::script::validate_release_host(&files.host)?;
        }
        progress("Using the engine and mod runtime included with the editor".into());
        return Ok(files);
    }
    let source = crate::development_root();
    ensure!(
        source.join("player/Cargo.toml").is_file()
            && source.join("runtime/host/index.ts").is_file(),
        "This editor installation is missing its engine support files. Reinstall the complete Whatever editor distribution."
    );
    let mut cargo = Command::new("cargo");
    cargo.current_dir(&source).args([
        "build",
        "--locked",
        "-p",
        "Whatever",
        "-p",
        "whatever-launcher",
    ]);
    if release {
        cargo.arg("--release");
    }
    run(
        &mut cargo,
        if release {
            "Building the release player and launcher…"
        } else {
            "Preparing the player…"
        },
        progress,
    )?;
    let suffix = std::env::consts::EXE_SUFFIX;
    let profile = if release { "release" } else { "debug" };
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                source.join(path)
            }
        })
        .unwrap_or_else(|| source.join("target"));
    let host = source
        .join("runtime/dist")
        .join(format!("whatever-mod-host{suffix}"));
    let valid = if release {
        crate::script::validate_release_host(&host)
    } else {
        crate::script::validate_host(&host)
    };
    if valid.is_err() {
        if !source
            .join("vendor/bun/build/release")
            .join(format!("bun{suffix}"))
            .is_file()
        {
            run(
                Command::new("bun")
                    .current_dir(&source)
                    .args(["run", "build:bun"]),
                "Building the pinned release Bun runtime…",
                progress,
            )?;
        }
        run(
            Command::new("bun")
                .current_dir(&source)
                .args(["run", "build:runtime"]),
            "Building the scripting API…",
            progress,
        )?;
        run(
            Command::new("bun")
                .current_dir(&source)
                .args(["run", "build:host"]),
            "Building the pinned mod runtime…",
            progress,
        )?;
    }
    let files = RuntimeFiles {
        player: target.join(profile).join(format!("Whatever{suffix}")),
        launcher: target
            .join(profile)
            .join(format!("whatever-launcher{suffix}")),
        host,
        libraries: Some(target.join(profile)),
        license: source.join("LICENSE"),
        bun_license: source.join("vendor/bun/LICENSE.md"),
    };
    files.validate()?;
    Ok(files)
}

/// A normal game developer only chooses a destination; the editor owns its support payload.
pub fn build_game(
    project: &Project,
    destination: &Path,
    mut progress: impl FnMut(String),
) -> anyhow::Result<PathBuf> {
    progress("Validating project and scenes…".into());
    let project = project.for_build()?;
    let files = prepare(true, &mut progress)?;
    progress("Packaging mods and branding the launcher…".into());
    let output = export_files(&project, &files, destination)?;
    progress(format!("Build complete: {}", super::display_path(&output)));
    Ok(output)
}

pub fn prepare_player(mut progress: impl FnMut(String)) -> anyhow::Result<PathBuf> {
    Ok(prepare(false, &mut progress)?.player)
}
