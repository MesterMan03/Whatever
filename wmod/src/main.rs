mod create;

use anyhow::{Context, bail, ensure};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use whatever_engine::project::{self, ModOptions, Project};

const HELP: &str = "Whatever development companion\n\n\
Usage:\n\
  wmod --version                                  Print the shared release version\n\
  wmod create <directory>                         Interactive mod creation\n\
  wmod mod create <directory> --id <id> [--name <name>] [--project <project>] [--no-scripts]\n\
  wmod project create <directory> --id <id> [--name <name>] [--core <core-mod>]\n\
  wmod project check <project-or-directory>\n\
  wmod project bind-sdk <project-or-directory>\n\
  wmod project add <project-or-directory> <mod-directory>\n\
  wmod package <project-or-directory> --output <build-directory>\n\
  wmod library pack <folder> --output <file.wasset>\n\
  wmod library inspect <file.wasset>\n\
  wmod library import <file.wasset> --project <project> --mod <id>\n\
  wmod runtime-kit --player <binary> --host <binary> --launcher <binary> --source <checkout> --output <new-directory>\n";

struct Arguments {
    positional: Vec<String>,
    options: BTreeMap<String, String>,
}
impl Arguments {
    fn parse(args: &[String], allowed: &[&str]) -> anyhow::Result<Self> {
        let mut result = Self {
            positional: Vec::new(),
            options: BTreeMap::new(),
        };
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            if let Some(key) = arg.strip_prefix("--") {
                ensure!(allowed.contains(&key), "unknown option --{key}");
                let value = if key == "no-scripts" {
                    String::new()
                } else {
                    let value = args
                        .next()
                        .with_context(|| format!("--{key} needs a value"))?;
                    ensure!(
                        !value.starts_with("--") && !value.is_empty(),
                        "--{key} needs a value"
                    );
                    value.clone()
                };
                ensure!(
                    result.options.insert(key.into(), value).is_none(),
                    "duplicate option --{key}"
                );
            } else {
                result.positional.push(arg.clone());
            }
        }
        Ok(result)
    }
    fn at(&self, index: usize, label: &str) -> anyhow::Result<&str> {
        self.positional
            .get(index)
            .map(String::as_str)
            .with_context(|| format!("missing {label}"))
    }
    fn required(&self, key: &str) -> anyhow::Result<&str> {
        self.options
            .get(key)
            .map(String::as_str)
            .with_context(|| format!("missing --{key}"))
    }
    fn count(&self, count: usize) -> anyhow::Result<()> {
        ensure!(
            self.positional.len() == count,
            "expected {count} positional arguments; run wmod --help"
        );
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .without_time()
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && matches!(args[0].as_str(), "--version" | "-V") {
        use std::io::Write;
        std::io::stdout().write_all(format!("wmod {}\n", env!("CARGO_PKG_VERSION")).as_bytes())?;
        return Ok(());
    }
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        use std::io::Write;
        std::io::stdout().write_all(HELP.as_bytes())?;
        return Ok(());
    }
    if args[0] == "create" {
        ensure!(
            args.len() <= 2,
            "use wmod mod create for noninteractive options"
        );
        return create::run(args.get(1).map(String::as_str).unwrap_or("."));
    }
    let action = args[0].as_str();
    let (subcommand, tail) = if matches!(action, "project" | "mod" | "library") {
        (
            args.get(1)
                .context("missing subcommand; run wmod --help")?
                .as_str(),
            &args[2..],
        )
    } else {
        ("", &args[1..])
    };
    match (action, subcommand) {
        ("library", "pack") => {
            let args = Arguments::parse(tail, &["output"])?;
            args.count(1)?;
            let library =
                project::AssetLibrary::from_directory(Path::new(args.at(0, "library folder")?))?;
            library.write(Path::new(args.required("output")?))?;
            tracing::info!(name=%library.manifest.name, files=library.files.len(), "packed asset library");
        }
        ("library", "inspect") => {
            let args = Arguments::parse(tail, &[])?;
            args.count(1)?;
            let library = project::AssetLibrary::open(Path::new(args.at(0, "library archive")?))?;
            use std::io::Write;
            std::io::stdout().write_all(format!("{:#?}\n", library.manifest).as_bytes())?;
            tracing::info!(
                files = library.files.len(),
                "archive validated without executing content"
            );
        }
        ("library", "import") => {
            let args = Arguments::parse(tail, &["project", "mod"])?;
            args.count(1)?;
            let library = project::AssetLibrary::open(Path::new(args.at(0, "library archive")?))?;
            let project = Project::open(Path::new(args.required("project")?))?;
            let imported = project.import_library(args.required("mod")?, &library)?;
            tracing::info!(path=%imported.directory.display(), reused=imported.reused, "imported editable library");
        }
        ("project", "create") => {
            let args = Arguments::parse(tail, &["id", "name", "core"])?;
            args.count(1)?;
            let id = args.required("id")?;
            let name = args.options.get("name").map(String::as_str).unwrap_or(id);
            let core = args
                .options
                .get("core")
                .map(PathBuf::from)
                .map(Ok)
                .unwrap_or_else(project::default_core)?;
            let project =
                project::create_project(Path::new(args.at(0, "directory")?), id, name, &core)?;
            tracing::info!(path=%whatever_engine::project::display_path(&project.path), "created project");
        }
        ("project", "bind-sdk") => {
            let args = Arguments::parse(tail, &[])?;
            args.count(1)?;
            let project = Project::open(Path::new(args.at(0, "project")?))?;
            project::bind_sdk(project.path.parent().context("project directory")?)?;
            for root in project.roots()? {
                if root.join("tsconfig.json").exists() {
                    project::bind_sdk(&root)?;
                }
            }
            tracing::info!("SDK binding updated");
        }
        ("project", "check") => {
            let args = Arguments::parse(tail, &[])?;
            args.count(1)?;
            Project::open(Path::new(args.at(0, "project")?))?.validate()?;
            tracing::info!("project, dependencies, scripts and scenes are valid");
        }
        ("project", "add") => {
            let args = Arguments::parse(tail, &[])?;
            args.count(2)?;
            Project::open(Path::new(args.at(0, "project")?))?
                .add_mod(Path::new(args.at(1, "mod directory")?))?;
            tracing::info!("added mod to project");
        }
        ("mod", "create") => {
            let args = Arguments::parse(tail, &["id", "name", "project", "no-scripts"])?;
            args.count(1)?;
            let id = args.required("id")?;
            let mut options = ModOptions::new(
                id,
                args.options.get("name").map(String::as_str).unwrap_or(id),
            );
            options.scripts = !args.options.contains_key("no-scripts");
            let mut project = args
                .options
                .get("project")
                .map(|path| Project::open(Path::new(path)))
                .transpose()?;
            if let Some(project) = &project {
                project.validate()?;
                ensure!(
                    !project
                        .manifests()?
                        .iter()
                        .any(|(_, mod_)| mod_.meta.id == id),
                    "mod ID already exists in project"
                );
            }
            let root = project::create_mod(Path::new(args.at(0, "directory")?), &options)?;
            if let Some(project) = &mut project {
                project.add_mod(&root)?;
            }
            tracing::info!(path=%whatever_engine::project::display_path(&root), "created mod");
        }
        ("package", "") => {
            let args = Arguments::parse(tail, &["runtime", "output"])?;
            args.count(1)?;
            let collection = Project::open(Path::new(args.at(0, "project")?))?;
            let destination = Path::new(args.required("output")?);
            let output = if let Some(runtime) = args.options.get("runtime") {
                project::export(&collection, Path::new(runtime), destination)?
            } else {
                project::build_game(&collection, destination, |line| tracing::info!("{line}"))?
            };
            tracing::info!(path=%whatever_engine::project::display_path(&output), "packaged game");
        }
        ("runtime-kit", "") => {
            let args = Arguments::parse(tail, &["player", "host", "launcher", "source", "output"])?;
            args.count(0)?;
            project::create_runtime_kit(
                Path::new(args.required("player")?),
                Path::new(args.required("host")?),
                Path::new(args.required("launcher")?),
                Path::new(args.required("source")?),
                Path::new(args.required("output")?),
            )?;
            tracing::info!("created runtime kit");
        }
        _ => bail!("unknown command; run wmod --help"),
    }
    Ok(())
}
