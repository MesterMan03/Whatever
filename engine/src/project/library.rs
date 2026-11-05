use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// A library is authoring content, never a loaded mod or an executable installer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryManifest {
    pub format_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    pub license: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub homepage: String,
    /// External mod requirements remain explicit in the receiving mod.
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}
impl LibraryManifest {
    pub fn validate(&self) -> anyhow::Result<()> {
        ensure!(self.format_version == 1, "unsupported library format");
        ensure!(
            crate::sandbox::valid_storage_id(&self.id) && self.id != "core",
            "invalid library ID"
        );
        ensure!(!self.name.trim().is_empty(), "library needs a name");
        ensure!(
            !self.authors.is_empty() && self.authors.iter().all(|s| !s.trim().is_empty()),
            "library needs creator attribution"
        );
        ensure!(!self.license.trim().is_empty(), "library needs a license");
        semver::Version::parse(&self.version).context("invalid library version")?;
        for (id, requirement) in &self.dependencies {
            ensure!(
                crate::sandbox::valid_storage_id(id),
                "invalid library dependency"
            );
            semver::VersionReq::parse(requirement)?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct AssetLibrary {
    pub manifest: LibraryManifest,
    pub files: BTreeMap<String, Vec<u8>>,
}

const IMPORT_BINDING: &str = ".wasset-import.json";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportBinding {
    version: u32,
    asset_prefix: String,
    component_prefix: String,
}

impl AssetLibrary {
    pub(crate) fn from_files(files: BTreeMap<String, Vec<u8>>) -> anyhow::Result<Self> {
        ensure!(
            !files.contains_key(IMPORT_BINDING),
            "archives cannot contain a local import binding"
        );
        super::library_archive::validate_files(&files)?;
        let metadata = files
            .get("wasset.toml")
            .context("library is missing root wasset.toml")?;
        let manifest: LibraryManifest = toml::from_str(std::str::from_utf8(metadata)?)?;
        manifest.validate()?;
        // Parse prefab structure before any files are imported. Custom component schemas
        // are checked against the receiving project after import.
        for (path, bytes) in &files {
            if path.ends_with(".wprefab") {
                crate::scene::PrefabDocument::parse(bytes)
                    .with_context(|| format!("prefab {path}"))?;
            }
        }
        Ok(Self { manifest, files })
    }

    pub fn from_directory(root: &Path) -> anyhow::Result<Self> {
        let mut files = super::library_archive::directory_files(root)?;
        // Convert edited imported bundles back to portable authoring namespaces.
        if let Some(bytes) = files.remove(IMPORT_BINDING) {
            let binding: ImportBinding = serde_json::from_slice(&bytes)?;
            let manifest: LibraryManifest = toml::from_str(std::str::from_utf8(
                files.get("wasset.toml").context("missing wasset.toml")?,
            )?)?;
            manifest.validate()?;
            let target = crate::vfs::VfsPath::parse(&binding.asset_prefix)
                .context("invalid import binding")?;
            ensure!(
                binding.version == 1
                    && binding.asset_prefix.ends_with('/')
                    && binding
                        .component_prefix
                        .starts_with(&format!("{}:", target.mod_id)),
                "invalid library import binding"
            );
            for (path, bytes) in &mut files {
                let extension = Path::new(path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                if path != "wasset.toml"
                    && [
                        "json", "wprefab", "wscene", "ts", "js", "mts", "mjs", "cts", "cjs", "tsx",
                        "jsx", "wgsl", "toml", "md", "txt",
                    ]
                    .contains(&extension)
                    && let Ok(text) = std::str::from_utf8(bytes)
                {
                    *bytes = text
                        .split(&binding.asset_prefix)
                        .map(|part| {
                            part.replace(&binding.component_prefix, &format!("{}:", manifest.id))
                        })
                        .collect::<Vec<_>>()
                        .join(&format!("{}://", manifest.id))
                        .into_bytes();
                }
            }
        }
        Self::from_files(files)
    }

    pub fn open(path: &Path) -> anyhow::Result<Self> {
        Self::from_files(super::library_archive::read_archive(path)?)
    }

    pub fn write(&self, destination: &Path) -> anyhow::Result<()> {
        self.validate()?;
        super::library_archive::write_archive(&self.files, destination)
    }

    /// Import the complete bundle atomically, remapping its own namespace. Identical
    /// imports are reused. An edited/conflicting bundle gets a new directory instead.
    pub fn import(&self, mod_root: &Path) -> anyhow::Result<LibraryImport> {
        self.validate()?;
        let normalized_root = mod_root.canonicalize()?;
        let mod_root = normalized_root.as_path();
        let target: crate::mods::ModManifest =
            toml::from_str(&std::fs::read_to_string(mod_root.join("mod.toml"))?)?;
        for dependency in self.manifest.dependencies.keys() {
            ensure!(
                dependency == &target.meta.id || target.dependencies.contains_key(dependency),
                "declare library dependency '{dependency}' in Mod Settings before importing"
            );
        }
        let directory = target
            .mounts
            .get("libraries")
            .map(String::as_str)
            .unwrap_or("libraries");
        let parent = crate::sandbox::resolve_contained(mod_root, directory)?;
        reject_links(mod_root, &parent)?;
        std::fs::create_dir_all(&parent)?;
        for index in 0..10000 {
            let name = if index == 0 {
                self.manifest.id.clone()
            } else {
                format!("{}_{index}", self.manifest.id)
            };
            let destination = parent.join(&name);
            reject_links(mod_root, &destination)?;
            let prefix = format!("{}://libraries/{name}/", target.meta.id);
            let files = self.remapped(&target.meta.id, &name, &prefix);
            if destination.exists() {
                if destination.is_dir()
                    && files
                        .iter()
                        .all(|(p, b)| std::fs::read(destination.join(p)).is_ok_and(|old| old == *b))
                {
                    return Ok(LibraryImport {
                        directory: destination,
                        prefix,
                        reused: true,
                    });
                }
                continue;
            }
            let staging_parent = crate::sandbox::resolve_contained(mod_root, ".whatever")?;
            reject_links(mod_root, &staging_parent)?;
            std::fs::create_dir_all(&staging_parent)?;
            let staging = staging_parent.join(format!(
                "library-import-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos()
            ));
            std::fs::create_dir(&staging)?;
            struct Cleanup(PathBuf);
            impl Drop for Cleanup {
                fn drop(&mut self) {
                    let _ = std::fs::remove_dir_all(&self.0);
                }
            }
            let _cleanup = Cleanup(staging.clone());
            for (path, bytes) in &files {
                let output = staging.join(path);
                std::fs::create_dir_all(output.parent().context("library file directory")?)?;
                std::fs::write(output, bytes)?;
            }
            ensure!(
                !destination.exists(),
                "library destination appeared during import"
            );
            let binding = ImportBinding {
                version: 1,
                asset_prefix: prefix.clone(),
                component_prefix: format!("{}:{name}_", target.meta.id),
            };
            std::fs::write(
                staging.join(IMPORT_BINDING),
                serde_json::to_vec_pretty(&binding)?,
            )?;
            std::fs::rename(staging, &destination).context("publish imported library")?;
            return Ok(LibraryImport {
                directory: destination,
                prefix,
                reused: false,
            });
        }
        anyhow::bail!("too many library directory conflicts")
    }

    fn remapped(&self, owner: &str, instance: &str, prefix: &str) -> BTreeMap<String, Vec<u8>> {
        let asset_prefix = format!("{}://", self.manifest.id);
        let component_prefix = format!("{}:", self.manifest.id);
        self.files
            .iter()
            .map(|(path, bytes)| {
                let extension = Path::new(path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                let bytes = if path != "wasset.toml"
                    && [
                        "json", "wprefab", "wscene", "ts", "js", "mts", "mjs", "cts", "cjs", "tsx",
                        "jsx", "wgsl", "toml", "md", "txt",
                    ]
                    .contains(&extension)
                {
                    std::str::from_utf8(bytes)
                        .map(|text| {
                            text.split(&asset_prefix)
                                .map(|part| {
                                    part.replace(&component_prefix, &format!("{owner}:{instance}_"))
                                })
                                .collect::<Vec<_>>()
                                .join(prefix)
                                .into_bytes()
                        })
                        .unwrap_or_else(|_| bytes.clone())
                } else {
                    bytes.clone()
                };
                (path.clone(), bytes)
            })
            .collect()
    }

    fn validate(&self) -> anyhow::Result<()> {
        ensure!(
            !self.files.contains_key(IMPORT_BINDING),
            "local import bindings cannot be distributed"
        );
        self.manifest.validate()?;
        super::library_archive::validate_files(&self.files)?;
        let metadata: LibraryManifest = toml::from_str(std::str::from_utf8(
            self.files
                .get("wasset.toml")
                .context("missing wasset.toml")?,
        )?)?;
        ensure!(
            metadata == self.manifest,
            "library metadata and file manifest differ"
        );
        for (path, bytes) in &self.files {
            if path.ends_with(".wprefab") {
                crate::scene::PrefabDocument::parse(bytes)?;
            }
        }
        Ok(())
    }
}

pub struct LibraryImport {
    pub directory: PathBuf,
    pub prefix: String,
    pub reused: bool,
}

impl super::Project {
    pub fn import_library(
        &self,
        owner: &str,
        library: &AssetLibrary,
    ) -> anyhow::Result<LibraryImport> {
        let manifests = self.manifests()?;
        let (root, _) = manifests
            .iter()
            .find(|(_, m)| m.meta.id == owner)
            .context("receiving mod")?;
        for (id, requirement) in &library.manifest.dependencies {
            let (_, manifest) = manifests
                .iter()
                .find(|(_, m)| &m.meta.id == id)
                .with_context(|| format!("library requires mod '{id}'"))?;
            ensure!(
                semver::VersionReq::parse(requirement)?
                    .matches(&semver::Version::parse(&manifest.meta.version)?),
                "library requires {id}@{requirement}"
            );
        }
        library.import(root)
    }
}

fn reject_links(root: &Path, path: &Path) -> anyhow::Result<()> {
    let mut current = root.to_path_buf();
    for part in path.strip_prefix(root)?.components() {
        current.push(part);
        if let Ok(metadata) = std::fs::symlink_metadata(&current) {
            ensure!(
                !metadata.file_type().is_symlink(),
                "library imports do not follow links"
            );
        }
    }
    Ok(())
}
