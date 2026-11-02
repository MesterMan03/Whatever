use anyhow::{Context, ensure};
use std::path::{Path, PathBuf};

pub fn starter_library() -> anyhow::Result<PathBuf> {
    let executable = std::env::current_exe()?;
    let installed = executable
        .parent()
        .context("editor directory")?
        .join("resources/starter");
    if installed.is_dir() {
        return Ok(installed);
    }
    if cfg!(debug_assertions) {
        return Ok(crate::development_root().join("engine/resources/starter"));
    }
    anyhow::bail!("Starter asset library is missing from this editor installation")
}

/// Materialize ordinary editable assets. Matching imports are reused; user files are preserved.
pub fn import_starter(root: &Path, relative: &str) -> anyhow::Result<String> {
    let manifest: crate::mods::ModManifest =
        toml::from_str(&std::fs::read_to_string(root.join("mod.toml"))?)?;
    let assets = if manifest.assets.root == "." {
        root.to_path_buf()
    } else {
        crate::sandbox::resolve_contained(root, &manifest.assets.root)?
    };
    std::fs::create_dir_all(&assets)?;
    let source = crate::sandbox::resolve_contained(&starter_library()?, relative)?;
    ensure!(source.is_file(), "starter asset '{relative}' is missing");
    let bytes = std::fs::read(source)?;
    let mut imported = format!("starter/{relative}");
    for index in 1..10000 {
        let destination = crate::sandbox::resolve_contained(&assets, &imported)?;
        if destination.is_file() && std::fs::read(&destination)? == bytes {
            return Ok(format!("{}://{imported}", manifest.meta.id));
        }
        if !destination.exists() {
            std::fs::create_dir_all(destination.parent().context("asset directory")?)?;
            // create_new also protects against a concurrent external edit.
            use std::io::Write;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)?
                .write_all(&bytes)?;
            return Ok(format!("{}://{imported}", manifest.meta.id));
        }
        let path = Path::new(relative);
        let stem = path
            .file_stem()
            .context("starter filename")?
            .to_string_lossy();
        let extension = path
            .extension()
            .map(|v| format!(".{}", v.to_string_lossy()))
            .unwrap_or_default();
        imported = format!(
            "starter/{}/{}_{index}{extension}",
            path.parent()
                .unwrap_or(Path::new(""))
                .to_string_lossy()
                .replace('\\', "/"),
            stem
        );
    }
    anyhow::bail!("too many starter asset filename conflicts")
}
