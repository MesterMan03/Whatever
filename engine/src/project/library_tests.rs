use super::*;
use crate::{
    ecs::World,
    scene::PrefabDocument,
    vfs::{Vfs, VfsPath},
};
use serde_json::json;
use std::{collections::BTreeMap, io::Write, path::PathBuf};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "whatever-library-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir(&path).expect("fixture");
        Self(path)
    }
    fn project(&self) -> Project {
        create_project(
            &self.0.join("game"),
            "library_qa",
            "Library QA",
            &crate::development_root().join("engine/resources/core"),
        )
        .expect("project")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn library() -> AssetLibrary {
    let manifest = LibraryManifest {
        format_version: 1,
        id: "sample".into(),
        name: "Sample library".into(),
        version: "1.0.0".into(),
        authors: vec!["A creator".into()],
        license: "MIT".into(),
        description: String::new(),
        homepage: String::new(),
        dependencies: BTreeMap::new(),
    };
    AssetLibrary::from_files(BTreeMap::from([
        ("wasset.toml".into(), toml::to_string(&manifest).expect("metadata").into_bytes()),
        ("textures/sample.bin".into(), vec![0,255,13,10,42]),
        ("scripts/Spin.ts".into(), b"export const type = 'sample:Spin'; const texture = 'sample://textures/sample.bin';".to_vec()),
        ("prefabs/item.wprefab".into(), serde_json::to_vec(&json!({"version":1,"name":"Item","entity":{"script_id":"item","children":[{"script_id":"child"}],"components":{"sample:Spin":{"texture":"sample://textures/sample.bin","target":{"$entity":"child"}}}}})).expect("prefab")),
    ])).expect("library")
}

#[test]
fn archives_roundtrip_and_imports_remap_namespaces_preserve_binary_files_and_edits() {
    let fixture = Fixture::new();
    let project = fixture.project();
    let library = library();
    let archive = fixture.0.join("sample.wasset");
    library.write(&archive).expect("pack");
    assert!(
        library.write(&archive).is_err(),
        "archives never overwrite existing files"
    );
    let reopened = AssetLibrary::open(&archive).expect("reopen");
    assert_eq!(reopened.manifest, library.manifest);
    assert_eq!(reopened.files, library.files);
    let first = project.import_library("core", &reopened).expect("import");
    assert!(!first.reused);
    assert_eq!(first.prefix, "core://libraries/sample/");
    assert_eq!(
        std::fs::read(first.directory.join("textures/sample.bin")).expect("binary"),
        vec![0, 255, 13, 10, 42]
    );
    let text = std::fs::read_to_string(first.directory.join("scripts/Spin.ts")).expect("source");
    assert!(text.contains("core:sample_Spin"));
    assert!(text.contains("core://libraries/sample/textures/sample.bin"));
    assert!(
        project
            .import_library("core", &reopened)
            .expect("reuse")
            .reused
    );
    std::fs::write(first.directory.join("textures/sample.bin"), b"user edit").expect("edit");
    let second = project
        .import_library("core", &reopened)
        .expect("conflict-safe import");
    assert_ne!(first.directory, second.directory);
    assert_eq!(
        std::fs::read(first.directory.join("textures/sample.bin")).expect("preserved"),
        b"user edit"
    );
    let text = std::fs::read_to_string(second.directory.join("scripts/Spin.ts")).expect("source");
    assert!(text.contains("core:sample_1_Spin"));
    assert!(text.contains("core://libraries/sample_1/textures/sample.bin"));
    let exported =
        AssetLibrary::from_directory(&second.directory).expect("export edited imported bundle");
    assert!(!exported.files.contains_key(".wasset-import.json"));
    assert!(
        std::str::from_utf8(&exported.files["scripts/Spin.ts"])
            .expect("source")
            .contains("sample:Spin")
    );
    let receiver = fixture.0.join("receiver");
    create_mod(&receiver, &ModOptions::new("other", "Other")).expect("receiver");
    let transferred = exported.import(&receiver).expect("portable reimport");
    let source =
        std::fs::read_to_string(transferred.directory.join("scripts/Spin.ts")).expect("source");
    assert!(source.contains("other:sample_Spin"));
    assert!(source.contains("other://libraries/sample/textures/sample.bin"));
}

#[test]
fn same_library_and_mod_id_does_not_corrupt_asset_uris() {
    let fixture = Fixture::new();
    let root = fixture.0.join("mod");
    create_mod(&root, &ModOptions::new("sample", "Sample")).expect("mod");
    let result = library().import(&root).expect("import");
    let text = std::fs::read_to_string(result.directory.join("scripts/Spin.ts")).expect("source");
    assert!(text.contains("sample://libraries/sample/textures/sample.bin"));
    assert!(text.contains("sample:sample_Spin"));
}

#[test]
fn invalid_paths_metadata_and_zip_entries_fail_before_importing() {
    let fixture = Fixture::new();
    let project = fixture.project();
    for path in [
        "../outside",
        "/absolute",
        "C:/outside",
        "folder\\escape",
        "NUL.txt",
        "trailing. /file",
        ".whatever/token",
    ] {
        let mut bad = library();
        bad.files.insert(path.into(), vec![1]);
        assert!(
            project.import_library("core", &bad).is_err(),
            "accepted {path}"
        );
    }
    let mut bad = library();
    bad.files.insert("SCRIPTS/spin.TS".into(), vec![]);
    assert!(bad.write(&fixture.0.join("bad.wasset")).is_err());
    let mut bad = library();
    bad.manifest.format_version = 2;
    assert!(project.import_library("core", &bad).is_err());
    for (n, path) in ["../outside", "C:/outside", "nul.txt", "folder\\outside"]
        .iter()
        .enumerate()
    {
        let archive = fixture.0.join(format!("bad-{n}.wasset"));
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).expect("archive"));
        zip.start_file(*path, zip::write::SimpleFileOptions::default())
            .expect("entry");
        zip.write_all(b"bad").expect("write");
        zip.finish().expect("finish");
        assert!(AssetLibrary::open(&archive).is_err(), "accepted zip {path}");
    }
    assert!(
        !project.roots().expect("roots")[0]
            .join("libraries/sample")
            .exists()
    );
    assert!(!fixture.0.join("outside").exists());
}

#[test]
fn prefab_instances_are_independent_and_internal_references_are_fresh() {
    let prefab=PrefabDocument::parse(&serde_json::to_vec(&json!({"version":1,"name":"Hierarchy","entity":{"script_id":"root","components":{"demo:link":{"target":{"$entity":"child"}}},"children":[{"script_id":"child"}]}})).expect("json")).expect("prefab");
    let mut world = World::new();
    world.allow_missing_components = true;
    let a = prefab.scene().instantiate(&mut world).expect("first");
    let b = prefab.scene().instantiate(&mut world).expect("second");
    assert_ne!(a.entity_ids, b.entity_ids);
    let root = crate::ecs::EntityId::parse(&a.script_ids["root"]).expect("root");
    assert_eq!(
        world.get_component(&root, "demo:link").expect("component")["target"]["$entity"],
        a.script_ids["child"]
    );
    assert!(PrefabDocument::parse(br#"{"version":2,"name":"Bad","entity":{}}"#).is_err());
    assert!(PrefabDocument::parse(br#"{"version":1,"name":"Bad","entity":{"components":{"demo:link":{"target":{"$entity":"outside"}}}}}"#).is_err());
}

#[test]
fn official_library_assets_and_every_prefab_validate_after_import() {
    let fixture = Fixture::new();
    let project = fixture.project();
    let library =
        AssetLibrary::from_directory(&crate::development_root().join("engine/resources/starter"))
            .expect("official library");
    assert!(library.files.len() > 250);
    let imported = project.import_library("core", &library).expect("import");
    let root = project.roots().expect("roots")[0].clone();
    let mut world = World::new();
    for definition in crate::script::inspect_components(&root, "core").expect("components") {
        world
            .register_component("core", definition)
            .expect("register");
    }
    assert_eq!(world.component_types.len(), 6);
    let (registry, vfs) = project
        .registry(crate::runtime::RuntimeMode::Client)
        .expect("registry");
    let mut prefabs = 0;
    for path in library.files.keys() {
        let uri = format!("{}{path}", imported.prefix);
        if path.ends_with(".wprefab") {
            PrefabDocument::load_checked(&vfs, &uri, &mut world.clone(), &registry)
                .unwrap_or_else(|error| panic!("{path}: {error:#}"));
            prefabs += 1;
        } else if path.ends_with(".obj") {
            let mesh = crate::renderer::load_mesh_from_vfs(&vfs, &uri)
                .unwrap_or_else(|error| panic!("{path}: {error:#}"));
            assert!(
                !mesh.vertices.is_empty() && !mesh.indices.is_empty(),
                "{path}"
            );
            if path == "meshes/roof.obj"
                || path == "meshes/ramp.obj"
                || path.starts_with("meshes/rock_")
            {
                let volume: f32 = mesh
                    .indices
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|face| {
                        let a = glam::Vec3::from(mesh.vertices[face[0] as usize].position);
                        let b = glam::Vec3::from(mesh.vertices[face[1] as usize].position);
                        let c = glam::Vec3::from(mesh.vertices[face[2] as usize].position);
                        a.dot(b.cross(c)) / 6.0
                    })
                    .sum();
                assert!(volume > 0.01, "{path} must have outward triangle winding");
            }
        } else if path.ends_with(".png") {
            image::load_from_memory(&vfs.read(&VfsPath::parse(&uri).expect("URI")).expect("read"))
                .expect("valid PNG");
        } else if path.ends_with(".wgsl") {
            let module = wgpu::naga::front::wgsl::parse_str(
                std::str::from_utf8(&library.files[path]).expect("WGSL"),
            )
            .unwrap_or_else(|error| panic!("{path}: {error:?}"));
            wgpu::naga::valid::Validator::new(
                wgpu::naga::valid::ValidationFlags::all(),
                wgpu::naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap_or_else(|error| panic!("{path}: {error:?}"));
        }
    }
    assert_eq!(prefabs, 131);
    project
        .validate()
        .expect("project validates on both runtime sides");
    project
        .move_resource(
            "core",
            "libraries/whatever_starter/meshes/cube.obj",
            "libraries/whatever_starter/meshes/box.obj",
            &world.component_types,
        )
        .expect("move");
    let cube = std::fs::read_to_string(imported.directory.join("prefabs/primitives/cube.wprefab"))
        .expect("prefab");
    assert!(cube.contains("meshes/box.obj"));
    project
        .validate()
        .expect("moved prefab references validate");
    project
        .move_resource(
            "core",
            "libraries/whatever_starter",
            "libraries/renamed",
            &world.component_types,
        )
        .expect("move complete library");
    project.validate().expect("renamed library validates");
    let portable = AssetLibrary::from_directory(&root.join("libraries/renamed"))
        .expect("export renamed library");
    let cube =
        std::str::from_utf8(&portable.files["prefabs/primitives/cube.wprefab"]).expect("JSON");
    assert!(cube.contains("whatever_starter://meshes/box.obj"));
}
