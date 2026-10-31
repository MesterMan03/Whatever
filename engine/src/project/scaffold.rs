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
    let executable = std::env::current_exe()?;
    let installed = executable
        .parent()
        .context("executable directory")?
        .join("mods/core");
    if installed.join("mod.toml").is_file() {
        return Ok(installed);
    }
    if cfg!(debug_assertions) {
        return Ok(crate::development_root().join("engine/resources/core"));
    }
    anyhow::bail!("core template is missing from this editor installation")
}

pub fn create_mod(directory: &Path, options: &ModOptions) -> anyhow::Result<PathBuf> {
    ensure!(
        crate::sandbox::valid_storage_id(&options.id) && options.id != "core",
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
        "assets":{"root":"assets"},"dependencies":{"core":"*"}});
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
            format!(
                "import {{ Engine, Scene }} from \"@whatever-engine/api\";\n\nEngine.on(\"init\", async () => {{\n  const initial = Scene.initialScene();\n  const paths = [\"{id}://main\", \"{id}://main.wscene\", \"{id}://scenes/main.wscene\"];\n  const scene = initial && paths.includes(initial.path) ? initial : await Scene.loadScene(\"{id}://main\");\n  if (Engine.mode === \"client\") Engine.setMainCamera(scene.get(\"camera\").id);\n}});\n",
                id = options.id
            ),
        )?;
        let executable = std::env::current_exe()?;
        let mut candidates = vec![
            executable
                .parent()
                .context("executable directory")?
                .join("sdk/index.d.ts"),
        ];
        if cfg!(debug_assertions) {
            candidates.push(crate::development_root().join("runtime/dist/index.d.ts"));
        }
        let types = candidates.into_iter().find(|path| path.is_file());
        let mut config = serde_json::json!({"compilerOptions":{"target":"ESNext","module":"ESNext","moduleResolution":"bundler","strict":true,"noEmit":true,"skipLibCheck":true},"include":["scripts/**/*.ts"]});
        let mut package = serde_json::json!({"name":options.id,"private":true,"type":"module"});
        if let Some(types) = types {
            std::fs::create_dir_all(directory.join("types"))?;
            std::fs::copy(types, directory.join("types/whatever-engine.d.ts"))?;
            config["compilerOptions"]["paths"] =
                serde_json::json!({"@whatever-engine/api":["./types/whatever-engine.d.ts"]});
        } else {
            package["devDependencies"] =
                serde_json::json!({"@whatever-engine/api":env!("CARGO_PKG_VERSION")});
        }
        std::fs::write(
            directory.join("package.json"),
            serde_json::to_vec_pretty(&package)?,
        )?;
        std::fs::write(
            directory.join("tsconfig.json"),
            serde_json::to_vec_pretty(&config)?,
        )?;
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
    copy_tree(core, &directory.join("mods/core"))?;
    let metadata =
        serde_json::json!({"game":{"id":id,"name":name,"default_mode":"client","tick_rate":60}});
    std::fs::write(
        directory.join("mods/core/meta.toml"),
        toml::to_string_pretty(&metadata)?,
    )?;
    create_mod(&directory.join("mods").join(id), &ModOptions::new(id, name))?;
    let project = Project {
        version: 1,
        id: id.into(),
        name: name.into(),
        mods: vec!["mods/core".into(), format!("mods/{id}")],
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
