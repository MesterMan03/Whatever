use super::Project;
use anyhow::{Context, ensure};
use std::path::{Path, PathBuf};

pub(super) struct RuntimeFiles {
    pub player: PathBuf,
    pub launcher: PathBuf,
    pub libraries: Option<PathBuf>,
    pub license: PathBuf,
}
impl RuntimeFiles {
    pub fn from_bundle(root: &Path) -> Self {
        let suffix = std::env::consts::EXE_SUFFIX;
        let binaries = super::installation::binary_directory(root);
        Self {
            player: binaries.join(format!("Whatever{suffix}")),
            launcher: binaries.join(format!("whatever-launcher{suffix}")),
            libraries: Some(binaries),
            license: root.join("LICENSE"),
        }
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        for path in [&self.player, &self.launcher, &self.license] {
            ensure!(
                path.is_file(),
                "editor support file is missing: {}",
                super::display_path(path)
            );
        }
        Ok(())
    }
}

pub(super) fn copy_tree(source: &Path, destination: &Path) -> anyhow::Result<()> {
    let metadata = std::fs::symlink_metadata(source)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "export does not follow links: {}",
        source.display()
    );
    if metadata.is_file() {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(source, destination)?;
    } else if metadata.is_dir() {
        std::fs::create_dir_all(destination)?;
        let mut entries = std::fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if [".git", ".whatever", "node_modules", "logs", ".DS_Store"]
                .iter()
                .any(|skip| entry.file_name() == *skip)
            {
                continue;
            }
            copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
        }
    }
    Ok(())
}

pub fn launcher_name(name: &str) -> anyhow::Result<String> {
    let safe: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let safe = safe.trim_end_matches([' ', '.']);
    ensure!(!safe.is_empty(), "project name cannot be empty");
    let base = safe.split('.').next().unwrap_or(safe).to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&base.as_str())
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && matches!(base.as_bytes()[3], b'1'..=b'9'));
    Ok(format!(
        "{}{safe}{}",
        if reserved { "_" } else { "" },
        std::env::consts::EXE_SUFFIX
    ))
}

/// Advanced tooling entry point for an explicitly supplied engine distribution.
pub fn export(project: &Project, runtime: &Path, destination: &Path) -> anyhow::Result<PathBuf> {
    export_files(project, &RuntimeFiles::from_bundle(runtime), destination)
}

pub(super) fn export_files(
    project: &Project,
    files: &RuntimeFiles,
    destination: &Path,
) -> anyhow::Result<PathBuf> {
    let project = project.for_build()?;
    files.validate()?;
    let game_executable = launcher_name(&project.name)?;
    let destination = std::path::absolute(destination)?;
    let parent = destination
        .parent()
        .context("build destination needs a parent directory")?
        .canonicalize()
        .context("build parent directory must already exist")?;
    let destination = parent.join(destination.file_name().context("build folder name")?);
    let manifests = project.manifests()?;
    for source in manifests
        .iter()
        .map(|(root, _)| root)
        .chain([&files.player, &files.launcher])
    {
        let source = source.canonicalize()?;
        ensure!(
            !destination.starts_with(&source) && !source.starts_with(&destination),
            "build destination cannot overlap the project mods or editor installation"
        );
    }
    if destination.exists() {
        ensure!(
            !std::fs::symlink_metadata(&destination)?
                .file_type()
                .is_symlink()
                && destination.is_dir(),
            "build destination must be an ordinary folder"
        );
        let empty = std::fs::read_dir(&destination)?.next().is_none();
        let owned = destination.join("build.json").is_file()
            && destination.join("whatever.wproject").is_file()
            && (destination.join("bin").is_dir() || destination.join("engine").is_dir());
        ensure!(
            empty || owned,
            "choose an empty folder or a previous Whatever build; this folder contains other files"
        );
    }
    let stage = parent.join(format!(
        ".whatever-export-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    std::fs::create_dir(&stage)?;
    // This guard only removes the exact staging folder created above, under the checked parent.
    struct Stage(PathBuf);
    impl Drop for Stage {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _stage_guard = Stage(stage.clone());
    copy_tree(&files.launcher, &stage.join(&game_executable))?;
    let suffix = std::env::consts::EXE_SUFFIX;
    copy_tree(
        &files.player,
        &stage.join("bin").join(format!("Whatever{suffix}")),
    )?;
    if let Some(libraries) = &files.libraries {
        let dependencies = super::installation::player_libraries(libraries)?;
        for library in &dependencies {
            copy_tree(
                library,
                &stage
                    .join("bin")
                    .join(library.file_name().context("library filename")?),
            )?;
        }
        super::installation::write_library_manifest(&stage.join("bin"), &dependencies)?;
    }
    copy_tree(&files.license, &stage.join("LICENSE"))?;
    std::fs::write(
        stage.join("RUNTIME-LICENSES.md"),
        include_str!("../../resources/RUNTIME-LICENSES.md"),
    )?;
    super::branding::brand(&project, &stage.join(&game_executable), &stage)?;
    let mut deployed = Project {
        version: 1,
        id: project.game_id(),
        name: project.name.clone(),
        mods: Vec::new(),
        build: project.build.clone(),
        path: stage.join("whatever.wproject"),
    };
    deployed.build.icon = Some("branding/icon.png".into());
    let mut versions = std::collections::BTreeMap::new();
    for (root, manifest) in manifests {
        let relative = format!("mods/{}", manifest.meta.id);
        copy_tree(&root, &stage.join(&relative))?;
        deployed.mods.push(relative);
        versions.insert(manifest.meta.id, manifest.meta.version);
    }
    let meta_path = stage.join("mods/core/meta.toml");
    let mut meta: toml::Value = if meta_path.is_file() {
        toml::from_str(&std::fs::read_to_string(&meta_path)?)?
    } else {
        toml::Value::Table(Default::default())
    };
    if meta.get("game").is_none() {
        meta["game"] = toml::Value::Table(Default::default());
    }
    meta["game"]["id"] = toml::Value::String(project.game_id());
    meta["game"]["name"] = toml::Value::String(project.name.clone());
    std::fs::write(meta_path, toml::to_string_pretty(&meta)?)?;
    deployed.mods.sort();
    deployed.save()?;
    deployed.validate()?;
    std::fs::write(
        stage.join("build.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "game": {"id":project.game_id(),"name":project.name}, "version": 1, "engine_version": env!("CARGO_PKG_VERSION"),
            "os": std::env::consts::OS, "arch": std::env::consts::ARCH, "mods": versions
        }))?,
    )?;
    let backup = stage.with_extension("previous");
    let replacing = destination.exists();
    if replacing {
        std::fs::rename(&destination, &backup).context("move previous build aside")?;
    }
    if let Err(error) = std::fs::rename(&stage, &destination) {
        if replacing {
            std::fs::rename(&backup, &destination).context("restore previous build")?;
        }
        return Err(error).context("publish completed build folder");
    }
    if replacing {
        std::fs::remove_dir_all(&backup).context("remove previous build backup")?;
    }
    Ok(destination)
}
