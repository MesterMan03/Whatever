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
        &crate::development_root().join("mods/core"),
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
            &crate::development_root().join("mods/core")
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
        &crate::development_root().join("mods/core"),
    )
    .expect("project");
    let root = fixture.0.join("game/mods/sample");
    std::fs::write(root.join("assets/original.txt"), "asset").expect("asset");
    std::fs::create_dir(root.join("data")).expect("data");
    std::fs::write(root.join("data/value.json"), "{}").expect("data");
    let mut manifest = std::fs::read_to_string(root.join("mod.toml")).expect("manifest");
    manifest.push_str("\n[mounts]\ndata = 'data'\n");
    std::fs::write(root.join("mod.toml"), manifest).expect("mount");
    let (_, vfs) = project.registry(RuntimeMode::Server).expect("registry");
    for path in [
        "sample://original.txt",
        "sample://scenes/main.wscene",
        "sample://data/value.json",
    ] {
        assert!(vfs.exists(&VfsPath::parse(path).expect("path")), "{path}");
    }
    let listed = vfs.list("sample", "").expect("list");
    assert!(listed.contains(&"scenes/main.wscene".to_owned()));
    assert!(listed.contains(&"data/value.json".to_owned()));
    assert!(!vfs.exists(&VfsPath::parse("sample://scenes/../mod.toml").expect("path")));
    std::fs::write(
        root.join("scenes/bad.wscene"),
        "{\"version\":2,\"entities\":[]}",
    )
    .expect("bad scene");
    assert!(project.validate().is_err());
}
