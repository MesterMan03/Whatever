use super::manifest::ModManifest;
use super::registry::{LoadedMod, ModRegistry};
use crate::debug::DebugLogger;
use crate::vfs::{DiskLayer, LayeredVfs, VfsPath};
use anyhow::{Context, bail};
use semver::{Version, VersionReq};
use std::collections::{BTreeSet, HashMap, HashSet};

type ModOrder = HashMap<String, (Vec<String>, Vec<String>, Vec<String>)>;
use std::path::Path;

pub fn discover_and_load(
    search_dirs: &[&Path],
    mode: crate::runtime::RuntimeMode,
    vfs: &mut LayeredVfs,
    registry: &mut ModRegistry,
    debug: &mut DebugLogger,
) -> anyhow::Result<()> {
    let mut manifests: Vec<(ModManifest, std::path::PathBuf)> = Vec::new();

    for dir in search_dirs {
        if !dir.exists() {
            continue;
        }
        let mod_roots = if dir.join("mod.toml").is_file() {
            vec![dir.to_path_buf()]
        } else {
            std::fs::read_dir(dir)
                .with_context(|| format!("reading mod directory {}", dir.display()))?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<Result<Vec<_>, _>>()?
        };
        for mod_root in mod_roots {
            if !mod_root.is_dir() {
                continue;
            }
            let toml_path = mod_root.join("mod.toml");
            if !toml_path.exists() {
                continue;
            }
            let src = std::fs::read_to_string(&toml_path)
                .with_context(|| format!("reading {}", toml_path.display()))?;
            let manifest: ModManifest =
                toml::from_str(&src).with_context(|| format!("parsing {}", toml_path.display()))?;
            debug.modloader(&format!(
                "discovered mod '{}' at {}",
                manifest.meta.id,
                mod_root.display()
            ));
            manifests.push((manifest, mod_root));
        }
    }

    let mut ids = HashSet::new();
    for (manifest, _) in &manifests {
        if !crate::sandbox::valid_storage_id(&manifest.meta.id) {
            bail!("invalid mod ID: {}", manifest.meta.id);
        }
        if !ids.insert(&manifest.meta.id) {
            bail!("duplicate mod ID: {}", manifest.meta.id);
        }
    }

    manifests.retain(|(manifest, _)| {
        let enabled = manifest.meta.sides.contains(&mode);
        if !enabled {
            registry.excluded.insert(manifest.meta.id.clone());
            tracing::info!(mod_id=%manifest.meta.id,%mode,"skipping mod for another runtime side");
        }
        enabled
    });

    // validate dependency existence
    let id_to_version: HashMap<String, Version> = manifests
        .iter()
        .map(|(m, _)| {
            let v = Version::parse(&m.meta.version).unwrap_or_else(|_| Version::new(0, 0, 0));
            (m.meta.id.clone(), v)
        })
        .collect();

    for (manifest, _) in &manifests {
        for (dep_id, req_str) in &manifest.dependencies {
            let req = VersionReq::parse(req_str).with_context(|| {
                format!(
                    "mod '{}' has invalid semver req for dep '{}'",
                    manifest.meta.id, dep_id
                )
            })?;
            match id_to_version.get(dep_id) {
                None => bail!(
                    "mod '{}' requires '{}' which is not loaded on this runtime side",
                    manifest.meta.id,
                    dep_id
                ),
                Some(v) if !req.matches(v) => bail!(
                    "mod '{}' requires {}@{}, found {}",
                    manifest.meta.id,
                    dep_id,
                    req_str,
                    v
                ),
                _ => {}
            }
        }
    }

    // Kahn's toposort
    let sorted = toposort(manifests, debug)?;

    // asset root per mod — used for dangling override validation
    let mod_assets: HashMap<String, std::path::PathBuf> = sorted
        .iter()
        .map(|(m, root)| {
            let assets = if m.assets.root == "." {
                root.canonicalize().context("resolve mod root")?
            } else {
                crate::sandbox::resolve_contained(root, &m.assets.root)
                    .with_context(|| format!("invalid asset root for mod '{}'", m.meta.id))?
            };
            Ok((m.meta.id.clone(), assets))
        })
        .collect::<anyhow::Result<_>>()?;

    // (after, before, dep_ids) per mod — used to check if conflicting override order is deterministic
    let mod_order: ModOrder = sorted
        .iter()
        .map(|(m, _)| {
            let deps: Vec<String> = m.dependencies.keys().cloned().collect();
            (
                m.meta.id.clone(),
                (
                    m.load_order.after.clone(),
                    m.load_order.before.clone(),
                    deps,
                ),
            )
        })
        .collect();

    // source path string -> current winning overrider mod_id
    let mut first_overriders: HashMap<String, String> = HashMap::new();

    for (manifest, mod_root) in sorted {
        let assets_root = mod_assets
            .get(&manifest.meta.id)
            .context("validated asset root is missing")?
            .clone();
        let mod_id = manifest.meta.id.clone();
        debug.modloader(&format!(
            "loading mod '{}' from {}",
            mod_id,
            mod_root.display()
        ));

        vfs.push_layer(Box::new(DiskLayer::new(mod_id.clone(), assets_root)));
        let mut mounts = manifest.mounts.clone();
        mounts
            .entry("scenes".into())
            .or_insert_with(|| "scenes".into());
        for (prefix, directory) in mounts {
            anyhow::ensure!(
                crate::sandbox::valid_storage_id(&prefix),
                "invalid VFS mount '{prefix}'"
            );
            let root = crate::sandbox::resolve_contained(&mod_root, &directory)
                .with_context(|| format!("invalid mount '{prefix}' in mod '{mod_id}'"))?;
            vfs.push_layer(Box::new(DiskLayer::mounted(mod_id.clone(), root, &prefix)));
        }

        // register explicit path overrides
        for (from_str, to_str) in &manifest.overrides.paths {
            let to_resolved = if to_str.contains("://") {
                to_str.clone()
            } else {
                format!("{}://{}", mod_id, to_str)
            };
            if let (Some(from), Some(to)) = (VfsPath::parse(from_str), VfsPath::parse(&to_resolved))
            {
                let from_key = from.as_string();
                if let Some(prev_mod) = first_overriders.get(&from_key)
                    && !has_explicit_order(prev_mod, &mod_id, &mod_order)
                {
                    tracing::warn!(
                        "override conflict: '{}' and '{}' both override '{}' with no explicit load order between them; '{}' wins — add [load_order] to make this deterministic",
                        prev_mod,
                        mod_id,
                        from_key,
                        mod_id
                    );
                }
                first_overriders.insert(from_key, mod_id.clone());
                vfs.add_override(from, to);
            }
        }

        registry.register(LoadedMod {
            manifest,
            root: mod_root,
        });
    }

    // Warn about dangling overrides: source asset doesn't physically exist
    for (source_str, declaring_mod) in &first_overriders {
        let Some(from) = VfsPath::parse(source_str) else {
            continue;
        };
        match mod_assets.get(&from.mod_id) {
            None => tracing::warn!(
                "dangling override by '{}': source mod '{}' is not loaded",
                declaring_mod,
                from.mod_id
            ),
            Some(assets_root) => {
                let mounted_exists = from.path.split_once('/').is_some_and(|(prefix, path)| {
                    registry
                        .iter()
                        .find(|loaded| loaded.manifest.meta.id == from.mod_id)
                        .is_some_and(|loaded| {
                            loaded
                                .manifest
                                .mounts
                                .get(prefix)
                                .map(String::as_str)
                                .or((prefix == "scenes").then_some("scenes"))
                                .and_then(|directory| {
                                    crate::sandbox::resolve_contained(&loaded.root, directory).ok()
                                })
                                .is_some_and(|root| root.join(path).is_file())
                        })
                });
                if !assets_root.join(&from.path).exists() && !mounted_exists {
                    tracing::warn!(
                        "dangling override by '{}': '{}' does not exist in mod '{}'",
                        declaring_mod,
                        from.path,
                        from.mod_id
                    );
                }
            }
        }
    }

    Ok(())
}

fn toposort(
    mut manifests: Vec<(ModManifest, std::path::PathBuf)>,
    debug: &mut DebugLogger,
) -> anyhow::Result<Vec<(ModManifest, std::path::PathBuf)>> {
    // Filesystem discovery order must not influence tick/override order.
    manifests.sort_by(|(a, _), (b, _)| a.meta.id.cmp(&b.meta.id));

    let ids: Vec<String> = manifests.iter().map(|(m, _)| m.meta.id.clone()).collect();
    let idx: HashMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let n = manifests.len();
    let mut in_degree = vec![0usize; n];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];

    for (i, (manifest, _)) in manifests.iter().enumerate() {
        // hard dep edges
        for dep_id in manifest.dependencies.keys() {
            if let Some(&j) = idx.get(dep_id.as_str()) {
                adj[j].push(i);
                in_degree[i] += 1;
            }
        }
        // soft load_order.after edges (ignored if absent)
        for after_id in &manifest.load_order.after {
            if let Some(&j) = idx.get(after_id.as_str()) {
                adj[j].push(i);
                in_degree[i] += 1;
            }
        }
        // soft load_order.before edges
        for before_id in &manifest.load_order.before {
            if let Some(&j) = idx.get(before_id.as_str()) {
                adj[i].push(j);
                in_degree[j] += 1;
            }
        }
    }

    let key = |i: usize| (ids[i] != "core", ids[i].clone(), i);
    let mut queue: BTreeSet<_> = (0..n).filter(|&i| in_degree[i] == 0).map(key).collect();
    let mut order: Vec<usize> = Vec::with_capacity(n);

    while let Some((_, _, i)) = queue.pop_first() {
        order.push(i);
        for &j in &adj[i] {
            in_degree[j] -= 1;
            if in_degree[j] == 0 {
                queue.insert(key(j));
            }
        }
    }

    if order.len() != n {
        bail!("mod dependency cycle detected");
    }

    let mut indexed: Vec<Option<(ModManifest, std::path::PathBuf)>> =
        manifests.into_iter().map(Some).collect();

    let sorted: Vec<_> = order
        .into_iter()
        .filter_map(|i| indexed[i].take())
        .collect();

    debug.modloader(&format!(
        "load order: {}",
        sorted
            .iter()
            .map(|(m, _)| m.meta.id.as_str())
            .collect::<Vec<_>>()
            .join(" → ")
    ));

    Ok(sorted)
}

fn has_explicit_order(mod_a: &str, mod_b: &str, mod_order: &ModOrder) -> bool {
    let references = |m: &str, other: &str| -> bool {
        if let Some((after, before, deps)) = mod_order.get(m) {
            after.iter().any(|id| id == other)
                || before.iter().any(|id| id == other)
                || deps.iter().any(|id| id == other)
        } else {
            false
        }
    };
    references(mod_a, mod_b) || references(mod_b, mod_a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependency_order_is_stable_across_discovery_and_hash_iteration() {
        let path = std::env::temp_dir().join(format!("whatever-sort-{}.log", std::process::id()));
        let log = std::fs::File::create(&path).expect("test log");
        let mut debug = DebugLogger::new(
            &crate::debug::DebugConfig::from_args(&[]),
            std::sync::Arc::new(std::sync::Mutex::new(std::io::BufWriter::new(log))),
        );
        for ids in [
            ["zeta", "core", "alpha", "beta"],
            ["beta", "alpha", "core", "zeta"],
        ] {
            let manifests = ids
                .into_iter()
                .map(|id| {
                    let dependencies = if id == "alpha" {
                        "\n[dependencies]\nzeta='*'\ncore='*'\n"
                    } else {
                        ""
                    };
                    let manifest = toml::from_str(&format!(
                        "[mod]\nid='{id}'\nname='{id}'\nversion='1.0.0'\n{dependencies}"
                    ))
                    .expect("manifest");
                    (manifest, std::path::PathBuf::new())
                })
                .collect();
            let order: Vec<_> = toposort(manifests, &mut debug)
                .expect("sort")
                .into_iter()
                .map(|(manifest, _)| manifest.meta.id)
                .collect();
            assert_eq!(order, ["core", "beta", "zeta", "alpha"]);
        }
        drop(debug);
        std::fs::remove_file(path).expect("cleanup log");
    }

    #[test]
    fn shipped_core_is_asset_only() {
        let root = crate::development_root();
        let core: ModManifest = toml::from_str(
            &std::fs::read_to_string(root.join("engine/resources/core/mod.toml"))
                .expect("core manifest"),
        )
        .expect("parse core");
        assert!(core.script.is_none());
    }
}
