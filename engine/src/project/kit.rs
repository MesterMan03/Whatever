use super::deployment::copy_tree;
use anyhow::{Context, ensure};
use std::path::Path;

/// Package the prebuilt player and pinned host. No system Bun or source checkout is needed to run it.
pub fn create_runtime_kit(
    player: &Path,
    host: &Path,
    launcher: &Path,
    source: &Path,
    output: &Path,
) -> anyhow::Result<()> {
    ensure!(!output.exists(), "runtime kit destination already exists");
    ensure!(player.is_file(), "missing player executable");
    ensure!(launcher.is_file(), "missing launcher executable");
    crate::script::validate_release_host(host)?;
    for path in [
        source.join("LICENSE"),
        source.join("vendor/bun/LICENSE.md"),
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
    let host = host.canonicalize()?;
    let source = source.canonicalize()?;
    let absolute = std::path::absolute(output)?;
    ensure!(
        !source.join("engine/resources/core").starts_with(&absolute)
            && !absolute.starts_with(source.join("engine/resources/core")),
        "runtime kit cannot overlap core sources"
    );
    std::fs::create_dir_all(output.join("engine/lib"))?;
    std::fs::create_dir_all(output.join("sdk"))?;
    copy_tree(
        &player,
        &output
            .join("engine")
            .join(format!("Whatever{}", std::env::consts::EXE_SUFFIX)),
    )?;
    copy_tree(
        &host,
        &output
            .join("engine")
            .join(format!("runtime{}", std::env::consts::EXE_SUFFIX)),
    )?;
    copy_tree(
        launcher,
        &output.join(format!("whatever-launcher{}", std::env::consts::EXE_SUFFIX)),
    )?;
    for entry in std::fs::read_dir(player.parent().context("player directory")?)? {
        let entry = entry?;
        if entry
            .path()
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
        {
            copy_tree(
                &entry.path(),
                &output.join("engine").join(entry.file_name()),
            )?;
        }
    }
    copy_tree(
        &source.join("engine/resources/core"),
        &output.join("mods/core"),
    )?;
    copy_tree(
        &source.join("engine/resources/starter"),
        &output.join("resources/starter"),
    )?;
    copy_tree(&source.join("LICENSE"), &output.join("LICENSE"))?;
    copy_tree(
        &source.join("vendor/bun/LICENSE.md"),
        &output.join("BUN-LICENSE.md"),
    )?;
    copy_tree(
        &source.join("runtime/dist/index.d.ts"),
        &output.join("sdk/index.d.ts"),
    )?;
    Ok(())
}
