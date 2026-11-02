use super::Project;
use crate::mods::ModManifest;
use anyhow::{Context, ensure};
use std::path::{Path, PathBuf};

pub fn resource_uri(manifest: &ModManifest, relative: &str) -> Option<String> {
    let asset = if manifest.assets.root == "." {
        Some(relative)
    } else {
        relative.strip_prefix(&format!("{}/", manifest.assets.root.trim_end_matches('/')))
    };
    let mut result = asset.map(str::to_owned);
    for (prefix, directory) in manifest
        .mounts
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .chain(std::iter::once((
            "scenes",
            manifest
                .mounts
                .get("scenes")
                .map(String::as_str)
                .unwrap_or("scenes"),
        )))
    {
        if let Some(tail) = relative.strip_prefix(&format!("{}/", directory.trim_end_matches('/')))
        {
            result = Some(format!("{prefix}/{tail}"));
        }
    }
    result.map(|path| format!("{}://{path}", manifest.meta.id))
}

fn checked(root: &Path, relative: &str) -> anyhow::Result<PathBuf> {
    let path = crate::sandbox::resolve_contained(root, relative)?;
    let mut current = root.to_path_buf();
    for part in Path::new(relative).components() {
        current.push(part);
        if let Ok(metadata) = std::fs::symlink_metadata(&current) {
            ensure!(
                !metadata.file_type().is_symlink(),
                "file operations do not follow links"
            );
        }
    }
    ensure!(
        !relative
            .split('/')
            .any(|part| [".git", ".whatever", "node_modules"].contains(&part)),
        "this folder is managed by tooling"
    );
    Ok(path)
}

fn editable(manifest: &ModManifest, relative: &str) -> anyhow::Result<()> {
    ensure!(
        ![
            "mod.toml",
            "meta.toml",
            "tsconfig.json",
            "package.json",
            ".gitignore",
            "scripts",
            "scenes"
        ]
        .contains(&relative)
            && relative != manifest.assets.root
            && !manifest.mounts.values().any(|dir| dir == relative),
        "mod configuration and root folders are managed through Mod Settings"
    );
    Ok(())
}

fn replace_prefix(value: &str, old: &str, new: &str) -> String {
    if value == old {
        return new.into();
    }
    value
        .strip_prefix(&format!("{old}/"))
        .map(|tail| format!("{new}/{tail}"))
        .unwrap_or_else(|| value.into())
}

fn replace_file(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;
    let temporary = path.with_file_name(format!(
        ".whatever-file-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    struct Temporary(PathBuf);
    impl Drop for Temporary {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let guard = Temporary(temporary.clone());
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temporary, path)?;
    drop(guard);
    Ok(())
}

impl Project {
    pub fn create_folder(&self, owner: &str, relative: &str) -> anyhow::Result<PathBuf> {
        let (root, _) = self
            .manifests()?
            .into_iter()
            .find(|(_, m)| m.meta.id == owner)
            .context("mod")?;
        let path = checked(&root, relative)?;
        ensure!(!path.exists(), "a file or folder already exists here");
        std::fs::create_dir(&path).context("create folder")?;
        Ok(path)
    }

    /// Move within a mod, updating authored scene URIs and manifest entry points.
    /// JS/TS source text is deliberately left for the developer to update.
    pub fn move_resource(
        &self,
        owner: &str,
        source: &str,
        destination: &str,
        definitions: &std::collections::BTreeMap<String, crate::ecs::ComponentDefinition>,
    ) -> anyhow::Result<PathBuf> {
        let manifests = self.manifests()?;
        let (root, manifest) = manifests
            .iter()
            .find(|(_, m)| m.meta.id == owner)
            .context("mod")?;
        editable(manifest, source)?;
        editable(manifest, destination)?;
        let from = checked(root, source)?;
        let to = checked(root, destination)?;
        ensure!(from.exists(), "source is missing");
        let case_only = source != destination
            && from
                .canonicalize()
                .ok()
                .zip(to.canonicalize().ok())
                .is_some_and(|(a, b)| a == b);
        ensure!(!to.exists() || case_only, "destination already exists");
        ensure!(!to.starts_with(&from), "cannot move a folder inside itself");
        ensure!(
            to.parent().is_some_and(Path::is_dir),
            "destination folder does not exist"
        );
        let old_uri = resource_uri(manifest, source);
        let new_uri = resource_uri(manifest, destination);
        ensure!(
            old_uri.is_none() || new_uri.is_some(),
            "assets and scenes must stay inside a VFS asset or scene folder"
        );
        let mut edits = Vec::<(PathBuf, Vec<u8>, Vec<u8>)>::new();
        for (mod_root, item) in &manifests {
            let manifest_path = mod_root.join("mod.toml");
            let original = std::fs::read(&manifest_path)?;
            let mut value: toml::Value = toml::from_str(std::str::from_utf8(&original)?)?;
            let before = value.clone();
            if item.meta.id == owner
                && let Some(script) = value.get_mut("script").and_then(toml::Value::as_table_mut)
            {
                for key in ["entry", "client", "server"] {
                    if let Some(toml::Value::String(path)) = script.get_mut(key) {
                        *path = replace_prefix(path, source, destination);
                    }
                }
            }
            if let Some(toml::Value::String(path)) = value.get_mut("initial_scene")
                && let (Some(old), Some(new)) = (&old_uri, &new_uri)
            {
                let canonical = crate::scene::scene_path(path)?.as_string();
                let replaced = replace_prefix(&canonical, old, new);
                if replaced != canonical {
                    *path = replaced;
                }
            }
            if value != before {
                edits.push((
                    manifest_path,
                    original,
                    toml::to_string_pretty(&value)?.into_bytes(),
                ));
            }
            let scenes = item
                .mounts
                .get("scenes")
                .map(String::as_str)
                .unwrap_or("scenes");
            let scene_root = crate::sandbox::resolve_contained(mod_root, scenes)?;
            if scene_root.is_dir()
                && let (Some(old), Some(new)) = (&old_uri, &new_uri)
            {
                use crate::vfs::Vfs;
                for relative in crate::vfs::DiskLayer::new(&item.meta.id, &scene_root)
                    .list(&item.meta.id, "")?
                {
                    if !relative.ends_with(".wscene") {
                        continue;
                    }
                    let path = scene_root.join(relative);
                    let original = std::fs::read(&path)?;
                    let mut value: crate::scene::SceneDocument = serde_json::from_slice(&original)?;
                    let before = serde_json::to_vec(&value)?;
                    crate::scene::rewrite_asset_references(&mut value, definitions, old, new);
                    if serde_json::to_vec(&value)? != before {
                        edits.push((path, original, serde_json::to_vec_pretty(&value)?));
                    }
                }
            }
        }
        std::fs::rename(&from, &to).context("move resource")?;
        let mut written = Vec::new();
        let result = (|| -> anyhow::Result<()> {
            for (path, original, replacement) in &edits {
                let path = if let Ok(tail) = path.strip_prefix(&from) {
                    if tail.as_os_str().is_empty() {
                        to.clone()
                    } else {
                        to.join(tail)
                    }
                } else {
                    path.clone()
                };
                ensure!(
                    std::fs::read(&path)? == *original,
                    "a referenced file changed outside the editor"
                );
                replace_file(&path, replacement)?;
                written.push((path, original, replacement));
            }
            Ok(())
        })();
        if let Err(error) = result {
            for (path, original, replacement) in written.into_iter().rev() {
                if std::fs::read(&path).is_ok_and(|bytes| bytes == *replacement) {
                    let _ = replace_file(&path, original);
                }
            }
            std::fs::rename(&to, &from)
                .context("restore resource after reference update failed")?;
            return Err(error);
        }
        Ok(to)
    }

    /// Deletion is reversible: move the item into this mod's ignored local trash.
    pub fn trash_resource(&self, owner: &str, relative: &str) -> anyhow::Result<PathBuf> {
        let (root, manifest) = self
            .manifests()?
            .into_iter()
            .find(|(_, m)| m.meta.id == owner)
            .context("mod")?;
        editable(&manifest, relative)?;
        let from = checked(&root, relative)?;
        ensure!(from.exists(), "source is missing");
        let trash = root.join(".whatever/trash").join(format!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        // Check the local trash parent before creating or moving anything.
        crate::sandbox::resolve_contained(&root, ".whatever/trash")?;
        std::fs::create_dir_all(&trash)?;
        let ignore = checked(&root, ".gitignore")?;
        let mut ignored = std::fs::read_to_string(&ignore).unwrap_or_default();
        if !ignored.lines().any(|line| line.trim() == ".whatever/") {
            if !ignored.ends_with('\n') {
                ignored.push('\n');
            }
            ignored.push_str(".whatever/\n");
            std::fs::write(ignore, ignored)?;
        }
        let target = trash.join(from.file_name().context("file name")?);
        std::fs::rename(from, &target)?;
        Ok(target)
    }
}
