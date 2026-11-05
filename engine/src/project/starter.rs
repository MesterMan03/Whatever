use anyhow::Context;
use std::path::{Path, PathBuf};

pub fn starter_library() -> anyhow::Result<PathBuf> {
    let executable = std::env::current_exe()?;
    let installed = executable
        .parent()
        .context("editor directory")?
        .join("resources/starter");
    if installed.join("wasset.toml").is_file() {
        return Ok(installed);
    }
    if cfg!(debug_assertions) {
        return Ok(crate::development_root().join("engine/resources/starter"));
    }
    anyhow::bail!("Official starter library is missing from this editor installation")
}

/// Native component defaults use the same ordinary library import as the catalog.
pub fn import_starter(root: &Path, relative: &str) -> anyhow::Result<String> {
    let library = super::AssetLibrary::from_directory(&starter_library()?)?;
    anyhow::ensure!(
        library.files.contains_key(relative),
        "missing starter resource '{relative}'"
    );
    Ok(format!("{}{relative}", library.import(root)?.prefix))
}
