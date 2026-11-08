use anyhow::Context;
use std::path::{Path, PathBuf};

pub fn starter_library() -> anyhow::Result<PathBuf> {
    if let Some(root) = super::installation::distribution_root()? {
        let installed = root.join("resources/starter");
        anyhow::ensure!(
            installed.join("wasset.toml").is_file(),
            "Official starter library is missing from this editor installation"
        );
        return Ok(installed);
    }
    super::installation::development_resource("engine/resources/starter")
        .context("Official starter library is missing from this editor installation")
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
