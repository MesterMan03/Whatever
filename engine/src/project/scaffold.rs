use super::{Project, deployment::copy_tree};
use anyhow::{Context, ensure};
use std::path::{Path, PathBuf};

pub struct ModOptions {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    pub license: String,
    pub scripts: bool,
}

impl ModOptions {
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: "0.1.0".into(),
            description: String::new(),
            authors: Vec::new(),
            license: "MIT".into(),
            scripts: true,
        }
    }
}

pub fn default_core() -> anyhow::Result<PathBuf> {
    if let Some(root) = super::installation::distribution_root()? {
        let installed = root.join("mods/core");
        if installed.join("mod.toml").is_file() {
            return Ok(installed);
        }
    }
    if cfg!(debug_assertions) {
        return Ok(crate::development_root().join("engine/resources/core"));
    }
    anyhow::bail!("core template is missing from this editor installation")
}

pub fn create_mod(directory: &Path, options: &ModOptions) -> anyhow::Result<PathBuf> {
    create_mod_inner(directory, options, false)
}

fn create_mod_inner(directory: &Path, options: &ModOptions, game: bool) -> anyhow::Result<PathBuf> {
    ensure!(
        crate::sandbox::valid_storage_id(&options.id) && (options.id != "core" || game),
        "invalid mod ID or reserved core ID"
    );
    semver::Version::parse(&options.version).context("invalid mod version")?;
    ensure!(
        !directory.exists() || std::fs::read_dir(directory)?.next().is_none(),
        "mod directory must be new or empty"
    );
    std::fs::create_dir_all(directory.join("assets"))?;
    std::fs::create_dir_all(directory.join("scenes"))?;
    let mut manifest = serde_json::json!({"mod":{"id":options.id,"name":options.name,"version":options.version,
        "description":options.description,"authors":options.authors,"license":options.license},
        "assets":{"root":"assets"}});
    manifest["dependencies"] = serde_json::json!({});
    if game {
        manifest["initial_scene"] = serde_json::json!("core://main");
    }
    if options.scripts {
        manifest["script"] = serde_json::json!({"entry":"scripts/index.ts"});
    }
    std::fs::write(
        directory.join("mod.toml"),
        toml::to_string_pretty(&manifest)?,
    )?;
    std::fs::write(directory.join("assets/.gitkeep"), "")?;
    std::fs::write(directory.join(".gitignore"), "node_modules/\nlogs/\n")?;
    let scene = serde_json::json!({"version":1,"entities":[{"name":"Camera","script_id":"camera","components":{
        "core:transform":{"position":[0,2,8],"rotation":[0,0,0,1],"scale":[1,1,1]},"core:camera":{}}},
        {"name":"Ambient light","components":{"core:ambient_light":{"intensity":0.3}}}]});
    std::fs::write(
        directory.join("scenes/main.wscene"),
        serde_json::to_vec_pretty(&scene)?,
    )?;
    if options.scripts {
        std::fs::create_dir_all(directory.join("scripts"))?;
        std::fs::write(
            directory.join("scripts/index.ts"),
            if game {
                "import { Engine, Scene } from \"@whatever-engine/api\";\n\nEngine.on(\"init\", () => {\n  const scene = Scene.initialScene();\n  if (scene && Engine.mode === \"client\") Engine.setMainCamera(scene.get(\"camera\"));\n});\n"
            } else {
                "import { Engine } from \"@whatever-engine/api\";\n\nEngine.on(\"init\", () => {\n  // Add your mod behavior here.\n});\n"
            },
        )?;
        std::fs::write(
            directory.join("package.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"name":options.id,"private":true,"type":"module"}),
            )?,
        )?;
        if !game {
            super::configure_types(directory)?;
        }
    }
    Ok(directory.canonicalize()?)
}

pub fn create_project(
    directory: &Path,
    id: &str,
    name: &str,
    core: &Path,
) -> anyhow::Result<Project> {
    ensure!(
        crate::sandbox::valid_storage_id(id) && id != "core",
        "invalid project ID"
    );
    ensure!(!directory.exists(), "project directory already exists");
    let manifest: crate::mods::ModManifest =
        toml::from_str(&std::fs::read_to_string(core.join("mod.toml"))?)?;
    ensure!(
        manifest.meta.id == "core",
        "template must be the ordinary core mod"
    );
    let core_root = directory.join("mods/core");
    create_mod_inner(&core_root, &ModOptions::new("core", name), true)?;
    // An explicit template may contribute assets and metadata; core remains the game mod.
    let generated = std::fs::read_to_string(core_root.join("mod.toml"))?;
    copy_tree(core, &core_root)?;
    let mut template: toml::Value =
        toml::from_str(&std::fs::read_to_string(core_root.join("mod.toml"))?)?;
    let generated: toml::Value = toml::from_str(&generated)?;
    template["mod"]["name"] = toml::Value::String(name.into());
    let table = template.as_table_mut().context("core template manifest")?;
    if !table.contains_key("script") {
        table.insert("script".into(), generated["script"].clone());
    }
    if !table.contains_key("initial_scene") {
        table.insert("initial_scene".into(), generated["initial_scene"].clone());
    }
    table
        .entry("dependencies")
        .or_insert_with(|| toml::Value::Table(Default::default()));
    std::fs::write(
        core_root.join("mod.toml"),
        toml::to_string_pretty(&template)?,
    )?;
    let metadata =
        serde_json::json!({"game":{"id":id,"name":name,"default_mode":"client","tick_rate":60}});
    std::fs::write(
        directory.join("mods/core/meta.toml"),
        toml::to_string_pretty(&metadata)?,
    )?;
    super::configure_types(directory)?;
    let project = Project {
        version: 1,
        id: id.into(),
        name: name.into(),
        mods: vec!["mods/core".into()],
        build: super::BuildSettings::default(),
        path: directory.canonicalize()?.join("whatever.wproject"),
    };
    project.validate()?;
    project.save()?;
    Ok(project)
}

impl Project {
    pub fn add_mod(&mut self, directory: &Path) -> anyhow::Result<()> {
        let directory = directory.canonicalize()?;
        ensure!(directory.join("mod.toml").is_file(), "not a mod directory");
        let parent = self
            .path
            .parent()
            .context("project directory")?
            .canonicalize()?;
        let directory = super::portable_path(&directory);
        let parent = super::portable_path(&parent);
        let path = directory
            .strip_prefix(&parent)
            .unwrap_or(&directory)
            .to_string_lossy()
            .replace('\\', "/");
        let mut candidate = self.clone();
        candidate.mods.push(path);
        candidate.validate()?;
        candidate.save()?;
        *self = candidate;
        Ok(())
    }
}
