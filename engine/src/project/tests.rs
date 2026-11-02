use super::*;
use crate::{
    runtime::RuntimeMode,
    vfs::{Vfs, VfsPath},
};

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "whatever-project-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir(&path).expect("fixture");
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn project_roundtrips_mod_roots_and_keeps_invalid_additions_off_disk() {
    let fixture = Fixture::new();
    let mut project = create_project(
        &fixture.0.join("game"),
        "sample_game",
        "A \"quoted\" game",
        &crate::development_root().join("engine/resources/core"),
    )
    .expect("project");
    let external = create_mod(
        &fixture.0.join("external"),
        &ModOptions::new("external", "External mod"),
    )
    .expect("mod");
    project.add_mod(&external).expect("add external");
    let reopened = Project::open(&project.path).expect("open");
    assert_eq!(reopened.name, "A \"quoted\" game");
    assert!(reopened.roots().expect("roots").contains(&external));
    let saved = std::fs::read_to_string(&project.path).expect("saved");
    assert!(project.add_mod(&external).is_err());
    assert_eq!(
        std::fs::read_to_string(&project.path).expect("unchanged"),
        saved
    );
    assert!(
        create_project(
            &fixture.0.join("game"),
            "other",
            "Other",
            &crate::development_root().join("engine/resources/core")
        )
        .is_err()
    );
}

#[test]
fn folder_mounts_preserve_legacy_assets_and_validate_scene_files() {
    let fixture = Fixture::new();
    let project = create_project(
        &fixture.0.join("game"),
        "sample",
        "Sample",
        &crate::development_root().join("engine/resources/core"),
    )
    .expect("project");
    let root = fixture.0.join("game/mods/core");
    std::fs::write(root.join("assets/original.txt"), "asset").expect("asset");
    std::fs::create_dir(root.join("data")).expect("data");
    std::fs::write(root.join("data/value.json"), "{}").expect("data");
    let mut manifest = std::fs::read_to_string(root.join("mod.toml")).expect("manifest");
    manifest.push_str("\n[mounts]\ndata = 'data'\n");
    std::fs::write(root.join("mod.toml"), manifest).expect("mount");
    let (_, vfs) = project.registry(RuntimeMode::Server).expect("registry");
    for path in [
        "core://original.txt",
        "core://scenes/main.wscene",
        "core://data/value.json",
    ] {
        assert!(vfs.exists(&VfsPath::parse(path).expect("path")), "{path}");
    }
    let listed = vfs.list("core", "").expect("list");
    assert!(listed.contains(&"scenes/main.wscene".to_owned()));
    assert!(listed.contains(&"data/value.json".to_owned()));
    assert!(!vfs.exists(&VfsPath::parse("core://scenes/../mod.toml").expect("path")));
    std::fs::write(
        root.join("scenes/bad.wscene"),
        "{\"version\":2,\"entities\":[]}",
    )
    .expect("bad scene");
    assert!(project.validate().is_err());
}

#[test]
fn build_exclusions_preserve_development_and_reject_broken_dependencies() {
    let fixture = Fixture::new();
    let mut project = create_project(
        &fixture.0.join("game"),
        "sample",
        "Sample",
        &crate::development_root().join("engine/resources/core"),
    )
    .expect("project");
    let tools = create_mod(
        &fixture.0.join("dev_tools"),
        &ModOptions::new("dev_tools", "Development tools"),
    )
    .expect("tools");
    project.add_mod(&tools).expect("add tools");
    project.build.exclude_mods = vec!["dev_tools".into()];
    let built = project.for_build().expect("filtered build");
    assert_eq!(project.mods.len(), 2);
    assert_eq!(built.mods.len(), 1);
    assert!(built.build.exclude_mods.is_empty());
    project.build.exclude_mods = vec!["core".into()];
    assert!(project.for_build().is_err());
    project.build.exclude_mods = vec!["unknown".into()];
    assert!(project.for_build().is_err());
    let path = fixture.0.join("game/mods/core/mod.toml");
    let mut manifest: toml::Value =
        toml::from_str(&std::fs::read_to_string(&path).expect("manifest")).expect("parse");
    manifest["dependencies"]
        .as_table_mut()
        .expect("dependencies")
        .insert("dev_tools".into(), toml::Value::String("*".into()));
    std::fs::write(path, toml::to_string(&manifest).expect("serialize")).expect("write");
    project.build.exclude_mods = vec!["dev_tools".into()];
    assert!(project.for_build().is_err());
}

#[test]
fn project_identity_mod_sides_and_startup_dependencies_are_explicit() {
    let fixture = Fixture::new();
    let mut project = create_project(
        &fixture.0.join("game"),
        "sample",
        "Sample",
        &crate::development_root().join("engine/resources/core"),
    )
    .expect("project");
    let extra = create_mod(
        &fixture.0.join("client_mod"),
        &ModOptions::new("client_mod", "Client mod"),
    )
    .expect("mod");
    project.add_mod(&extra).expect("add");
    let manifest = std::fs::read_to_string(extra.join("mod.toml")).expect("manifest");
    std::fs::write(
        extra.join("mod.toml"),
        manifest.replace("[mod]", "[mod]\nsides = ['client']"),
    )
    .expect("sides");
    project
        .validate()
        .expect("unused client mod is optional on servers");
    std::fs::write(
        fixture.0.join("game/mods/core/meta.toml"),
        "[game]\nid='stale_game'\nname='Stale name'\ntick_rate=60\n",
    )
    .expect("metadata");
    let options = crate::runtime::LaunchOptions {
        project: Some(project.path.clone()),
        mode: Some(RuntimeMode::Server),
        ..Default::default()
    };
    let writer = std::sync::Arc::new(std::sync::Mutex::new(std::io::BufWriter::new(
        std::fs::File::create(fixture.0.join("game.log")).expect("log"),
    )));
    let loaded = crate::runtime::LoadedGame::load(
        &options,
        &crate::debug::DebugConfig::from_args(&[]),
        &fixture.0,
        writer,
    )
    .expect("load");
    assert_eq!(loaded.meta.game.id, "sample");
    assert_eq!(loaded.meta.game.name, "Sample");
    assert!(loaded.registry.get("client_mod").is_none());
    assert_eq!(
        loaded.registry.startup_scene().expect("startup"),
        Some("core://main")
    );
    project.build.exclude_mods = vec!["client_mod".into()];
    project
        .for_build()
        .expect("independent client mod may be excluded");
    let path = fixture.0.join("game/mods/core/mod.toml");
    let mut manifest: toml::Value =
        toml::from_str(&std::fs::read_to_string(&path).expect("manifest")).expect("parse");
    manifest["initial_scene"] = toml::Value::String("client_mod://main".into());
    manifest["dependencies"]
        .as_table_mut()
        .expect("dependencies")
        .insert("client_mod".into(), toml::Value::String("*".into()));
    std::fs::write(path, toml::to_string(&manifest).expect("serialize")).expect("write");
    assert!(
        project.for_build().is_err(),
        "startup dependency cannot be excluded"
    );
    assert!(
        project.validate().is_err(),
        "a shared mod cannot require a client-only dependency on servers"
    );
}
