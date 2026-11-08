use anyhow::{Context, ensure};
use std::path::{Path, PathBuf};

pub(super) fn distribution_root() -> anyhow::Result<Option<PathBuf>> {
    let executable = std::env::current_exe()?;
    let directory = executable.parent().context("executable directory")?;
    Ok([Some(directory), directory.parent()]
        .into_iter()
        .flatten()
        .find(|root| root.join("distribution.json").is_file())
        .map(Path::to_path_buf))
}

pub(super) fn binary_directory(root: &Path) -> PathBuf {
    if cfg!(target_os = "macos") && root.file_name().is_some_and(|name| name == "MacOS") {
        root.to_path_buf()
    } else {
        root.join("bin")
    }
}

/// The package collector records the player's complete private dependency closure.
pub(super) fn player_libraries(directory: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let manifest = directory.join("native-libraries.json");
    let names: Vec<String> = if manifest.is_file() {
        let info: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest)?)?;
        ensure!(info["format"] == 1, "unsupported native library manifest");
        serde_json::from_value(info["player"].clone())?
    } else {
        let mut files: Vec<String> = [
            "whatever_engine",
            "whatever_script_vm",
            "whatever_render_deps",
        ]
        .into_iter()
        .map(|name| {
            format!(
                "{}{name}{}",
                std::env::consts::DLL_PREFIX,
                std::env::consts::DLL_SUFFIX
            )
        })
        .filter(|name| directory.join(name).is_file())
        .collect();
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if (name.starts_with("std-") || name.starts_with("libstd-"))
                && name.ends_with(std::env::consts::DLL_SUFFIX)
            {
                files.push(name);
            }
        }
        files
    };
    names
        .into_iter()
        .map(|name| {
            ensure!(
                !name.is_empty()
                    && Path::new(&name)
                        .file_name()
                        .is_some_and(|file| file == name.as_str())
                    && !name.contains(['/', '\\', ':']),
                "invalid native library filename"
            );
            let file = directory.join(name);
            ensure!(file.is_file(), "missing native library {}", file.display());
            Ok(file)
        })
        .collect()
}

pub(super) fn write_library_manifest(directory: &Path, files: &[PathBuf]) -> anyhow::Result<()> {
    let names = files
        .iter()
        .map(|file| {
            file.file_name()
                .context("native library filename")?
                .to_str()
                .context("native library name is not UTF-8")
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    std::fs::write(
        directory.join("native-libraries.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"format":1,"player":names}))?,
    )?;
    Ok(())
}
