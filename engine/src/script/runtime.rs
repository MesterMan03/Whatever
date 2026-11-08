use anyhow::{Context, ensure};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ScriptPackage {
    pub modules: BTreeMap<String, String>,
    pub definitions: Vec<crate::ecs::ComponentDefinition>,
}

pub fn source_files(root: &Path) -> anyhow::Result<BTreeMap<String, String>> {
    let root = root.canonicalize().context("resolve mod root")?;
    let mut files = BTreeMap::new();
    let mut bytes = 0;
    let mut visited = 0;
    fn walk(
        root: &Path,
        path: &Path,
        files: &mut BTreeMap<String, String>,
        bytes: &mut usize,
        visited: &mut usize,
        depth: usize,
    ) -> anyhow::Result<()> {
        ensure!(depth <= 64, "mod source directories exceed depth limit");
        ensure!(
            path.canonicalize()?.starts_with(root),
            "source directory escapes mod root"
        );
        let mut entries = std::fs::read_dir(path)?
            .take(50001)
            .collect::<Result<Vec<_>, _>>()?;
        ensure!(
            entries.len() + *visited <= 50000,
            "mod source directory exceeds entry limit"
        );
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            *visited += 1;
            ensure!(
                *visited <= 50000,
                "mod source directory exceeds entry limit"
            );
            let name = entry.file_name();
            if ["node_modules", ".git", ".whatever", "assets", "logs"]
                .iter()
                .any(|skip| name == *skip)
            {
                continue;
            }
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            if kind.is_dir() {
                walk(root, &path, files, bytes, visited, depth + 1)?;
                continue;
            }
            let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
                continue;
            };
            if !["js", "jsx", "ts", "tsx", "mts", "cts", "mjs", "cjs", "json"].contains(&extension)
                || path.to_string_lossy().ends_with(".d.ts")
            {
                continue;
            }
            let metadata = entry.metadata()?;
            ensure!(
                metadata.len() <= 4 * 1024 * 1024,
                "source file exceeds 4 MiB: {}",
                path.display()
            );
            let canonical = path.canonicalize()?;
            ensure!(canonical.starts_with(root), "source escapes mod root");
            use std::io::Read;
            let mut source = String::new();
            std::fs::File::open(canonical)?
                .take(4 * 1024 * 1024 + 1)
                .read_to_string(&mut source)?;
            ensure!(source.len() <= 4 * 1024 * 1024, "source file exceeds 4 MiB");
            *bytes += source.len();
            ensure!(
                *bytes <= 32 * 1024 * 1024 && files.len() < 10000,
                "mod sources exceed file/byte limits"
            );
            files.insert(
                path.strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/"),
                source,
            );
        }
        Ok(())
    }
    walk(&root, &root, &mut files, &mut bytes, &mut visited, 0)?;
    Ok(files)
}

pub fn compile_sources(root: &Path, owner: &str) -> anyhow::Result<ScriptPackage> {
    let files = source_files(root)?;
    use sha2::{Digest, Sha256};
    static COMPILER_ID: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let compiler = COMPILER_ID
        .get_or_init(|| Sha256::digest(super::EmbeddedRuntime::bundled_bytes()).to_vec());
    let mut digest = Sha256::new();
    digest.update(compiler);
    digest.update(owner.as_bytes());
    for (name, text) in &files {
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update((text.len() as u64).to_le_bytes());
        digest.update(text.as_bytes());
    }
    let key = format!("{:x}", digest.finalize());
    let cache = dirs::cache_dir().map(|directory| {
        directory
            .join("Whatever/script-cache")
            .join(format!("{key}.json"))
    });
    if let Some(path) = &cache
        && std::fs::metadata(path).is_ok_and(|metadata| metadata.len() <= 64 * 1024 * 1024)
        && let Ok(bytes) = std::fs::read(path)
        && let Ok(package) = serde_json::from_slice::<ScriptPackage>(&bytes)
        && package
            .definitions
            .iter()
            .all(|definition| definition.validate(owner).is_ok())
    {
        return Ok(package);
    }
    thread_local! {static COMPILER:std::cell::RefCell<Option<super::EmbeddedRuntime>>=const{std::cell::RefCell::new(None)};}
    let result = COMPILER
        .with_borrow_mut(|slot| -> anyhow::Result<serde_json::Value> {
            if slot.is_none() {
                *slot = Some(super::EmbeddedRuntime::bundled()?);
            }
            slot.as_mut()
                .context("source compiler")?
                .compile(owner, &files)
        })
        .with_context(|| format!("compile scripts for mod '{owner}'"))?;
    let package: ScriptPackage = serde_json::from_value(result)?;
    for definition in &package.definitions {
        definition.validate(owner)?;
    }
    if let Some(path) = cache
        && let (Some(parent), Ok(bytes)) = (path.parent(), serde_json::to_vec(&package))
        && std::fs::create_dir_all(parent).is_ok()
    {
        let _ = std::fs::write(path, bytes);
    }
    Ok(package)
}

pub fn inspect_components(
    root: &Path,
    owner: &str,
) -> anyhow::Result<Vec<crate::ecs::ComponentDefinition>> {
    Ok(compile_sources(root, owner)?.definitions)
}
pub fn load_component_types(
    world: &mut crate::ecs::World,
    registry: &crate::mods::ModRegistry,
) -> anyhow::Result<()> {
    for item in registry
        .iter()
        .filter(|item| item.manifest.script.is_some())
    {
        let owner = &item.manifest.meta.id;
        for definition in inspect_components(&item.root, owner)? {
            world.register_component(owner, definition)?;
        }
    }
    Ok(())
}
