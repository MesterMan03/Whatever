use std::collections::{HashMap, VecDeque};
use std::path::Path;
use anyhow::{bail, Context};
use semver::{Version, VersionReq};
use crate::debug::DebugLogger;
use crate::vfs::{DiskLayer, LayeredVfs, VfsPath};
use super::manifest::ModManifest;
use super::registry::{LoadedMod, ModRegistry};

pub fn discover_and_load(
    search_dirs: &[&Path],
    vfs: &mut LayeredVfs,
    registry: &mut ModRegistry,
    debug: &mut DebugLogger,
) -> anyhow::Result<()> {
    let mut manifests: Vec<(ModManifest, std::path::PathBuf)> = Vec::new();

    for dir in search_dirs {
        if !dir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(dir)
            .with_context(|| format!("reading mod directory {}", dir.display()))?
        {
            let entry = entry?;
            let mod_root = entry.path();
            if !mod_root.is_dir() {
                continue;
            }
            let toml_path = mod_root.join("mod.toml");
            if !toml_path.exists() {
                continue;
            }
            let src = std::fs::read_to_string(&toml_path)
                .with_context(|| format!("reading {}", toml_path.display()))?;
            let manifest: ModManifest = toml::from_str(&src)
                .with_context(|| format!("parsing {}", toml_path.display()))?;
            debug.modloader(&format!("discovered mod '{}' at {}", manifest.meta.id, mod_root.display()));
            manifests.push((manifest, mod_root));
        }
    }

    // validate dependency existence
    let id_to_version: HashMap<String, Version> = manifests
        .iter()
        .map(|(m, _)| {
            let v = Version::parse(&m.meta.version)
                .unwrap_or_else(|_| Version::new(0, 0, 0));
            (m.meta.id.clone(), v)
        })
        .collect();

    for (manifest, _) in &manifests {
        for (dep_id, req_str) in &manifest.dependencies {
            let req = VersionReq::parse(req_str)
                .with_context(|| format!("mod '{}' has invalid semver req for dep '{}'", manifest.meta.id, dep_id))?;
            match id_to_version.get(dep_id) {
                None => bail!("mod '{}' requires '{}' which is not loaded", manifest.meta.id, dep_id),
                Some(v) if !req.matches(v) => bail!(
                    "mod '{}' requires {}@{}, found {}",
                    manifest.meta.id, dep_id, req_str, v
                ),
                _ => {}
            }
        }
    }

    // Kahn's toposort
    let sorted = toposort(manifests, &id_to_version, debug)?;

    for (manifest, mod_root) in sorted {
        let assets_root = mod_root.join(&manifest.assets.root);
        let mod_id = manifest.meta.id.clone();
        debug.modloader(&format!("loading mod '{}' from {}", mod_id, mod_root.display()));

        vfs.push_layer(Box::new(DiskLayer::new(mod_id.clone(), assets_root)));

        // register explicit path overrides
        for (from_str, to_str) in &manifest.overrides.paths {
            if let (Some(from), Some(to)) = (VfsPath::parse(from_str), VfsPath::parse(to_str)) {
                vfs.add_override(from, to);
            }
        }

        registry.register(LoadedMod { manifest, root: mod_root });
    }

    Ok(())
}

fn toposort(
    mut manifests: Vec<(ModManifest, std::path::PathBuf)>,
    id_to_version: &HashMap<String, Version>,
    debug: &mut DebugLogger,
) -> anyhow::Result<Vec<(ModManifest, std::path::PathBuf)>> {
    // ensure core is first if present
    if let Some(core_pos) = manifests.iter().position(|(m, _)| m.meta.id == "core") {
        let core = manifests.remove(core_pos);
        manifests.insert(0, core);
    }

    let ids: Vec<String> = manifests.iter().map(|(m, _)| m.meta.id.clone()).collect();
    let idx: HashMap<&str, usize> = ids.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
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

    let mut queue: VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
    let mut order: Vec<usize> = Vec::with_capacity(n);

    while let Some(i) = queue.pop_front() {
        order.push(i);
        for &j in &adj[i] {
            in_degree[j] -= 1;
            if in_degree[j] == 0 {
                queue.push_back(j);
            }
        }
    }

    if order.len() != n {
        bail!("mod dependency cycle detected");
    }

    let _ = id_to_version; // used for validation above

    let mut indexed: Vec<Option<(ModManifest, std::path::PathBuf)>> =
        manifests.into_iter().map(Some).collect();

    let sorted: Vec<_> = order
        .into_iter()
        .filter_map(|i| indexed[i].take())
        .collect();

    debug.modloader(&format!(
        "load order: {}",
        sorted.iter().map(|(m, _)| m.meta.id.as_str()).collect::<Vec<_>>().join(" → ")
    ));

    Ok(sorted)
}