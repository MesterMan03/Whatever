use super::deployment::copy_tree;
use anyhow::{Context, ensure};
use std::path::Path;

/// Package the player with its embedded compiler and Wasm runtime. Games contain ordinary script sources.
pub fn create_runtime_kit(
    player: &Path,
    launcher: &Path,
    source: &Path,
    output: &Path,
) -> anyhow::Result<()> {
    ensure!(!output.exists(), "runtime kit destination already exists");
    ensure!(player.is_file(), "missing player executable");
    ensure!(launcher.is_file(), "missing launcher executable");
    for path in [
        source.join("LICENSE"),
        source.join("engine/resources/core/mod.toml"),
        source.join("runtime/dist/index.d.ts"),
    ] {
        ensure!(
            path.is_file(),
            "missing runtime kit source {}",
            path.display()
        );
    }
    // Check all inputs before creating output; leave existing inputs untouched.
    let player = player.canonicalize()?;
    let source = source.canonicalize()?;
    let absolute = std::path::absolute(output)?;
    ensure!(
        !source.join("engine/resources/core").starts_with(&absolute)
            && !absolute.starts_with(source.join("engine/resources/core")),
        "runtime kit cannot overlap core sources"
    );
    let binaries = super::installation::binary_directory(output);
    std::fs::create_dir_all(&binaries)?;
    std::fs::create_dir_all(output.join("sdk"))?;
    copy_tree(
        &player,
        &binaries.join(format!("Whatever{}", std::env::consts::EXE_SUFFIX)),
    )?;
    copy_tree(
        launcher,
        &binaries.join(format!("whatever-launcher{}", std::env::consts::EXE_SUFFIX)),
    )?;
    let libraries =
        super::installation::player_libraries(player.parent().context("player directory")?)?;
    for library in &libraries {
        copy_tree(
            library,
            &binaries.join(library.file_name().context("library filename")?),
        )?;
    }
    super::installation::write_library_manifest(&binaries, &libraries)?;
    std::fs::write(
        output.join("distribution.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"format":2,"version":env!("CARGO_PKG_VERSION")}),
        )?,
    )?;
    copy_tree(
        &source.join("engine/resources/core"),
        &output.join("mods/core"),
    )?;
    copy_tree(
        &source.join("engine/resources/starter"),
        &output.join("resources/starter"),
    )?;
    std::fs::create_dir_all(output.join("resources/libraries"))?;
    super::AssetLibrary::from_directory(&source.join("engine/resources/starter"))?
        .write(&output.join("resources/libraries/WhateverStarter.wasset"))?;
    copy_tree(&source.join("LICENSE"), &output.join("LICENSE"))?;
    std::fs::write(
        output.join("RUNTIME-LICENSES.md"),
        include_str!("../../resources/RUNTIME-LICENSES.md"),
    )?;
    copy_tree(
        &source.join("runtime/dist/index.d.ts"),
        &output.join("sdk/index.d.ts"),
    )?;
    Ok(())
}
