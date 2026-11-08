use anyhow::{Context, ensure};
use std::path::{Path, PathBuf};

pub fn installed_sdk() -> anyhow::Result<PathBuf> {
    let distribution = super::installation::distribution_root()?;
    let mut candidates = distribution
        .as_ref()
        .map(|root| root.join("sdk/index.d.ts"))
        .into_iter()
        .collect::<Vec<_>>();
    if distribution.is_none()
        && let Some(source) = super::installation::development_resource("runtime/dist/index.d.ts")
    {
        candidates.push(source);
    }
    let sdk = candidates
        .into_iter()
        .find(|path| path.is_file())
        .context("SDK missing: use a complete editor distribution or rebuild the runtime SDK")?;
    if let Some(root) = sdk.parent().and_then(Path::parent) {
        let distribution = root.join("distribution.json");
        if distribution.is_file() {
            let info: serde_json::Value = serde_json::from_slice(&std::fs::read(distribution)?)?;
            ensure!(
                info["version"].as_str() == Some(env!("CARGO_PKG_VERSION")),
                "The installed SDK version does not match this editor; use the complete matching distribution"
            );
        }
    }
    Ok(sdk)
}

/// Only the small machine-local binding is generated; declarations stay in the editor.
pub fn bind_sdk(directory: &Path) -> anyhow::Result<()> {
    let sdk = installed_sdk()?.canonicalize()?;
    ensure!(sdk.is_file(), "SDK declarations are missing");
    let local = crate::sandbox::resolve_contained(directory, ".whatever")?;
    std::fs::create_dir_all(&local)?;
    let binding = serde_json::json!({"compilerOptions":{"paths":{"@whatever-engine/api":[super::portable_path(&sdk).to_string_lossy()]}}});
    std::fs::write(
        crate::sandbox::resolve_contained(directory, ".whatever/tsconfig.sdk.json")?,
        serde_json::to_vec_pretty(&binding)?,
    )?;
    let ignore = crate::sandbox::resolve_contained(directory, ".gitignore")?;
    let mut text = std::fs::read_to_string(&ignore).unwrap_or_default();
    if !text.lines().any(|line| line.trim() == ".whatever/") {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(".whatever/\n");
        std::fs::write(ignore, text)?;
    }
    Ok(())
}

pub fn configure_types(directory: &Path) -> anyhow::Result<()> {
    bind_sdk(directory)?;
    let config = serde_json::json!({"extends":"./.whatever/tsconfig.sdk.json", "compilerOptions":{
        "target":"ESNext","module":"ESNext","moduleResolution":"bundler","strict":true,"noEmit":true,"skipLibCheck":true,
        "allowImportingTsExtensions":true,"allowJs":true,"resolveJsonModule":true},"include":["scripts/**/*","libraries/**/*","mods/**/*"],"exclude":["**/node_modules","**/assets"]});
    std::fs::write(
        crate::sandbox::resolve_contained(directory, "tsconfig.json")?,
        serde_json::to_vec_pretty(&config)?,
    )?;
    Ok(())
}
