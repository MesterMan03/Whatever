use super::RuntimeMode;
use crate::{
    debug::{DebugConfig, DebugLogger},
    logging::SharedLogWriter,
    mods::{GameMeta, ModRegistry, discover_and_load},
    vfs::{LayeredVfs, VfsHandle},
};
use anyhow::{Context, bail};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
/// Installed games resolve data beside the executable, independently of launch cwd.
pub fn installation_root() -> anyhow::Result<PathBuf> {
    let executable = std::env::current_exe()?;
    let parent = executable.parent().context("executable has no parent")?;
    for directory in [Some(parent), parent.parent()].into_iter().flatten() {
        if directory.join("whatever.wproject").is_file()
            || directory.join("mods/core/mod.toml").is_file()
        {
            return Ok(directory.to_path_buf());
        }
    }
    if cfg!(debug_assertions) {
        return Ok(crate::development_root());
    }
    Ok(parent.to_path_buf())
}

#[derive(Default, Debug)]
pub struct LaunchOptions {
    pub mode: Option<RuntimeMode>,
    pub mods: Option<PathBuf>,
    pub project: Option<PathBuf>,
    pub network: crate::network::NetworkSettings,
}
impl LaunchOptions {
    pub fn parse(args: &[String]) -> anyhow::Result<Self> {
        let mut result = Self::default();
        let mut args = args.iter().skip(1);
        while let Some(arg) = args.next() {
            if arg == "--mode" || arg.starts_with("--mode=") {
                let value = if let Some(value) = arg.strip_prefix("--mode=") {
                    value
                } else {
                    args.next().context("--mode requires client or server")?
                };
                if result.mode.replace(value.parse()?).is_some() {
                    bail!("--mode may only be specified once");
                }
            } else if ["--tls-cert", "--tls-key", "--tls-ca", "--network-data"]
                .contains(&arg.as_str())
            {
                let path = PathBuf::from(
                    args.next()
                        .with_context(|| format!("{arg} requires a path"))?,
                );
                match arg.as_str() {
                    "--tls-cert" => result.network.certificate = Some(path),
                    "--tls-key" => result.network.private_key = Some(path),
                    "--tls-ca" => result.network.authority = Some(path),
                    _ => result.network.directory = path,
                }
            } else if arg == "--project" || arg.starts_with("--project=") {
                let value = arg.strip_prefix("--project=").map(Ok).unwrap_or_else(|| {
                    args.next()
                        .map(String::as_str)
                        .context("--project requires a path")
                })?;
                anyhow::ensure!(
                    !value.is_empty() && result.project.is_none(),
                    "--project requires one path"
                );
                result.project = Some(PathBuf::from(value));
            } else if arg == "--mods" || arg.starts_with("--mods=") {
                let value = if let Some(value) = arg.strip_prefix("--mods=") {
                    value
                } else {
                    args.next().context("--mods requires a directory")?
                };
                if value.is_empty() {
                    bail!("--mods requires a directory");
                }
                result.mods = Some(PathBuf::from(value));
            }
        }
        anyhow::ensure!(
            result.project.is_none() || result.mods.is_none(),
            "--project and --mods cannot be combined"
        );
        anyhow::ensure!(
            result.network.certificate.is_some() == result.network.private_key.is_some(),
            "--tls-cert and --tls-key must be supplied together"
        );
        Ok(result)
    }
}
pub struct LoadedGame {
    pub mode: RuntimeMode,
    pub network: crate::network::NetworkSettings,
    pub meta: GameMeta,
    pub registry: Arc<ModRegistry>,
    pub vfs: VfsHandle,
    pub debug: DebugLogger,
}
impl LoadedGame {
    pub fn load(
        options: &LaunchOptions,
        debug_config: &DebugConfig,
        cwd: &Path,
        writer: SharedLogWriter,
    ) -> anyhow::Result<Self> {
        let mut debug = DebugLogger::new(debug_config, writer);
        let project = if let Some(path) = &options.project {
            Some(crate::project::Project::open(path)?)
        } else if options.mods.is_none() && cwd.join("whatever.wproject").is_file() {
            Some(crate::project::Project::open(cwd)?)
        } else {
            None
        };
        let (directories, meta_path) = if let Some(project) = project {
            let directories = project.roots()?;
            let core = project
                .manifests()?
                .into_iter()
                .find(|(_, manifest)| manifest.meta.id == "core")
                .context("project is missing core")?
                .0;
            (directories, core.join("meta.toml"))
        } else if let Some(root) = &options.mods {
            (vec![root.clone()], root.join("core/meta.toml"))
        } else {
            (
                vec![cwd.join("mods"), cwd.join("mods_user")],
                cwd.join("mods/core/meta.toml"),
            )
        };
        let meta = if meta_path.exists() {
            toml::from_str::<GameMeta>(
                &std::fs::read_to_string(&meta_path)
                    .with_context(|| format!("reading {}", meta_path.display()))?,
            )
            .context("parse core/meta.toml")?
        } else {
            GameMeta::default()
        };
        let mode = options.mode.unwrap_or(meta.game.default_mode);
        super::scheduler::TickClock::new(meta.game.tick_rate)?;
        tracing::info!(%mode, game=%meta.game.id, "loading game");
        let mut vfs = LayeredVfs::new();
        vfs.set_log(
            debug.log_writer(),
            debug.console_mirror(),
            debug.shared_switches(),
        );
        let mut registry = ModRegistry::new();
        let roots: Vec<&Path> = directories.iter().map(PathBuf::as_path).collect();
        discover_and_load(&roots, mode, &mut vfs, &mut registry, &mut debug)?;
        let mut network = options.network.clone();
        if network.directory.as_os_str().is_empty() {
            network.directory = crate::network::NetworkSettings::for_game(&meta.game.id)?.directory;
        }
        Ok(Self {
            mode,
            network,
            meta,
            registry: Arc::new(registry),
            vfs: Arc::new(vfs),
            debug,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        std::iter::once("Whatever")
            .chain(values.iter().copied())
            .map(str::to_owned)
            .collect()
    }
    #[test]
    fn mode_flags_are_explicit_and_validated() {
        assert_eq!(
            LaunchOptions::parse(&args(&["--mode=server"]))
                .expect("options")
                .mode,
            Some(RuntimeMode::Server)
        );
        assert_eq!(
            LaunchOptions::parse(&args(&["--mode", "client"]))
                .expect("options")
                .mode,
            Some(RuntimeMode::Client)
        );
        for flags in [
            &["--mode"][..],
            &["--mode=bad"][..],
            &["--mode=client", "--mode=server"][..],
        ] {
            assert!(LaunchOptions::parse(&args(flags)).is_err());
        }
    }
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "whatever-config-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos()
            ));
            std::fs::create_dir(&path).expect("fixture");
            Self(path)
        }
        fn put(&self, name: &str, text: &str) {
            let path = self.0.join(name);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("directories");
            std::fs::write(path, text).expect("file");
        }
        fn load(&self, mode: Option<RuntimeMode>) -> anyhow::Result<LoadedGame> {
            let log = std::fs::File::create(self.0.join("test.log"))?;
            LoadedGame::load(
                &LaunchOptions {
                    mode,
                    mods: Some(self.0.clone()),
                    ..Default::default()
                },
                &DebugConfig::from_args(&[]),
                &self.0,
                Arc::new(std::sync::Mutex::new(std::io::BufWriter::new(log))),
            )
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn metadata_default_and_flag_select_mods_before_dependency_checks() {
        let fixture = Fixture::new();
        fixture.put(
            "core/meta.toml",
            "[game]\nid='fixture'\nname='Fixture'\ndefault_mode='server'\n",
        );
        fixture.put(
            "core/mod.toml",
            "[mod]\nid='core'\nname='Core'\nversion='1.0.0'\n",
        );
        fixture.put("client/mod.toml","[mod]\nid='client'\nname='Client'\nversion='1.0.0'\nsides=['client']\n[dependencies]\nmissing='*'\n");
        fixture.put(
            "server/mod.toml",
            "[mod]\nid='server'\nname='Server'\nversion='1.0.0'\nsides=['server']\n",
        );
        let game = fixture
            .load(None)
            .expect("server skips client dependencies");
        assert_eq!(game.mode, RuntimeMode::Server);
        assert!(game.registry.get("core").is_some());
        assert!(game.registry.get("server").is_some());
        assert!(game.registry.get("client").is_none());
        assert!(fixture.load(Some(RuntimeMode::Client)).is_err());
    }
    #[test]
    fn dependencies_on_an_excluded_side_are_reported() {
        let fixture = Fixture::new();
        fixture.put(
            "client/mod.toml",
            "[mod]\nid='client'\nname='Client'\nversion='1.0.0'\nsides=['client']\n",
        );
        fixture.put("server/mod.toml","[mod]\nid='server'\nname='Server'\nversion='1.0.0'\nsides=['server']\n[dependencies]\nclient='*'\n");
        let error = fixture
            .load(Some(RuntimeMode::Server))
            .err()
            .expect("invalid dependency");
        assert!(error.to_string().contains("requires 'client'"));
    }
    #[test]
    fn side_entries_override_the_shared_fallback() {
        let manifest:crate::mods::ModManifest=toml::from_str("[mod]\nid='core'\nname='Core'\nversion='1.0.0'\n[script]\nentry='shared.ts'\nclient='client.ts'\n").expect("script config");
        let config = manifest.script.expect("script section");
        assert_eq!(config.entry_for(RuntimeMode::Client), Some("client.ts"));
        assert_eq!(config.entry_for(RuntimeMode::Server), Some("shared.ts"));
    }
}
