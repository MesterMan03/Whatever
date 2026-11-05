use super::*;
use crate::{ecs::World, runtime::RuntimeMode, scene::SceneDocument};
use serde_json::json;
use std::path::PathBuf;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "whatever-workflow-{}-{}",
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
            "sample",
            "Sample",
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

#[test]
fn a_new_game_owns_one_scripted_core_and_references_the_installed_sdk() {
    let fixture = Fixture::new();
    let project = fixture.project();
    let root = project.path.parent().expect("root");
    assert_eq!(project.mods, vec!["mods/core"]);
    assert!(root.join("mods/core/scripts/index.ts").is_file());
    assert!(!root.join("mods/sample").exists());
    assert!(!root.join("mods/core/assets/meshes").exists());
    assert!(!root.join("mods/core/types").exists());
    assert!(!root.join("types").exists());
    let config = std::fs::read_to_string(root.join("tsconfig.json")).expect("config");
    assert!(config.contains("./.whatever/tsconfig.sdk.json"));
    let binding = root.join(".whatever/tsconfig.sdk.json");
    std::fs::write(&binding, "stale path").expect("stale");
    bind_sdk(root).expect("rebind");
    let binding: serde_json::Value =
        serde_json::from_slice(&std::fs::read(binding).expect("binding")).expect("JSON");
    assert!(
        std::path::Path::new(
            binding["compilerOptions"]["paths"]["@whatever-engine/api"][0]
                .as_str()
                .expect("path")
        )
        .is_file()
    );
}

#[test]
fn starter_imports_preserve_user_edits_and_restore_deleted_resources_on_demand() {
    let fixture = Fixture::new();
    let project = fixture.project();
    let root = project.roots().expect("roots")[0].clone();
    let first = import_starter(&root, "meshes/cube.obj").expect("import");
    assert_eq!(first, "core://libraries/whatever_starter/meshes/cube.obj");
    assert_eq!(
        import_starter(&root, "meshes/cube.obj").expect("reuse"),
        first
    );
    let path = root.join("libraries/whatever_starter/meshes/cube.obj");
    std::fs::write(&path, "user mesh").expect("edit");
    let second = import_starter(&root, "meshes/cube.obj").expect("distinct import");
    assert_ne!(second, first);
    assert_eq!(
        std::fs::read_to_string(&path).expect("preserved"),
        "user mesh"
    );
    project
        .trash_resource("core", "libraries/whatever_starter/meshes/cube.obj")
        .expect("trash");
    assert_ne!(
        import_starter(&root, "meshes/cube.obj").expect("restore"),
        first
    );
    assert!(import_starter(&root, "../mod.toml").is_err());
}

#[test]
fn scene_asset_dependencies_are_direct_and_build_exclusions_cannot_break_them() {
    let fixture = Fixture::new();
    let mut project = fixture.project();
    let mut options = ModOptions::new("library", "Library");
    options.scripts = false;
    let library = create_mod(&fixture.0.join("library"), &options).expect("library");
    project.add_mod(&library).expect("add");
    std::fs::write(library.join("assets/cube.obj"), "mesh").expect("asset");
    let scene=SceneDocument::parse(&serde_json::to_vec(&json!({"version":1,"entities":[{"script_id":"cube","components":{"core:mesh_renderer":{"mesh":"library://cube.obj","shader":"library://cube.obj"}}}]})).expect("json")).expect("scene");
    let core = project.roots().expect("roots")[0].clone();
    let path = core.join("scenes/shared.wscene");
    std::fs::write(&path, serde_json::to_vec_pretty(&scene).expect("JSON")).expect("scene");
    let (_, vfs) = project.registry(RuntimeMode::Client).expect("registry");
    let error = project
        .validate_scene(&path, &scene, &vfs, &Default::default())
        .expect_err("undeclared dependency");
    assert!(format!("{error:#}").contains("declare a dependency on 'library'"));
    let manifest_path = core.join("mod.toml");
    let mut manifest: toml::Value =
        toml::from_str(&std::fs::read_to_string(&manifest_path).expect("manifest")).expect("parse");
    manifest["dependencies"]
        .as_table_mut()
        .expect("dependencies")
        .insert("library".into(), toml::Value::String("*".into()));
    std::fs::write(
        manifest_path,
        toml::to_string_pretty(&manifest).expect("toml"),
    )
    .expect("manifest");
    project.validate().expect("declared direct dependency");
    project.build.exclude_mods = vec!["library".into()];
    let error = project
        .validate_build_selection()
        .expect_err("dependency exclusion");
    assert!(error.to_string().contains("core"));
    std::fs::remove_file(library.join("assets/cube.obj")).expect("delete asset");
    assert!(project.validate().is_err());
}

#[test]
fn asset_moves_rewrite_scenes_and_manifest_paths_and_delete_uses_local_trash() {
    let fixture = Fixture::new();
    let project = fixture.project();
    let root = project.roots().expect("roots")[0].clone();
    std::fs::write(root.join("assets/texture.png"), "pixels").expect("asset");
    let scene = json!({"version":1,"entities":[{"components":{"core:sprite_renderer":{"texture":"core://texture.png","shader":"core://texture.png"}}}]});
    let scene_path = root.join("scenes/asset.wscene");
    std::fs::write(&scene_path, serde_json::to_vec(&scene).expect("JSON")).expect("scene");
    project
        .create_folder("core", "assets/images")
        .expect("folder");
    project
        .move_resource(
            "core",
            "assets/texture.png",
            "assets/Texture.png",
            &Default::default(),
        )
        .expect("case-only rename");
    project
        .move_resource(
            "core",
            "assets/Texture.png",
            "assets/images/texture.png",
            &Default::default(),
        )
        .expect("move");
    let saved = std::fs::read_to_string(&scene_path).expect("scene");
    assert!(saved.contains("core://images/texture.png"));
    assert!(!saved.contains("core://texture.png"));
    project
        .move_resource(
            "core",
            "scripts/index.ts",
            "scripts/game.ts",
            &Default::default(),
        )
        .expect("rename script");
    project
        .move_resource(
            "core",
            "scenes/main.wscene",
            "scenes/start.wscene",
            &Default::default(),
        )
        .expect("rename initial scene");
    assert!(
        std::fs::read_to_string(root.join("mod.toml"))
            .expect("manifest")
            .contains("core://scenes/start.wscene")
    );
    assert!(
        std::fs::read_to_string(root.join("mod.toml"))
            .expect("manifest")
            .contains("scripts/game.ts")
    );
    let trash = project
        .trash_resource("core", "assets/images/texture.png")
        .expect("trash");
    assert_eq!(
        std::fs::read_to_string(trash).expect("recoverable"),
        "pixels"
    );
    assert!(project.trash_resource("core", "mod.toml").is_err());
    assert!(
        project
            .move_resource(
                "core",
                "scripts/game.ts",
                "../escape.ts",
                &Default::default()
            )
            .is_err()
    );
    assert!(
        project
            .move_resource(
                "core",
                "assets/images",
                "assets/images/child",
                &Default::default()
            )
            .is_err()
    );
    assert!(project.create_folder("core", "../outside").is_err());
}

#[test]
fn core_custom_components_work_without_claiming_native_ids() {
    let mut world = World::new();
    let definition = crate::ecs::ComponentDefinition {
        id: "core:Player".into(),
        source: "scripts/player.ts".into(),
        export: "Player".into(),
        fields: vec![],
    };
    world
        .register_component("core", definition.clone())
        .expect("core component");
    let entity = world.create_entity();
    assert!(world.set_component(&entity, "core:Player", json!({})));
    assert!(world.get_component(&entity, "core:Player").is_some());
    let mut native = definition;
    native.id = "core:transform".into();
    assert!(world.register_component("core", native).is_err());
    assert!(!world.set_component(&entity, "core:Unregistered", json!({})));
}

#[test]
fn nested_typed_assets_require_direct_dependencies_and_moves_preserve_plain_strings() {
    use crate::{
        ecs::{ComponentDefinition, ComponentField},
        schema::{Field, Schema},
        vfs::{DiskLayer, LayeredVfs},
    };
    let fixture = Fixture::new();
    std::fs::write(fixture.0.join("texture.png"), "pixels").expect("asset");
    let mut vfs = LayeredVfs::new();
    vfs.push_layer(Box::new(DiskLayer::new("library", &fixture.0)));
    let mut owner: crate::mods::ModManifest = toml::from_str(
        "[mod]\nid='core'\nname='Game'\nversion='0.1.0'\n[dependencies]\nmiddle='*'\n",
    )
    .expect("manifest");
    let schema = Schema::Struct {
        fields: vec![Field {
            name: "images".into(),
            schema: Schema::Array {
                element: Box::new(Schema::Tuple {
                    elements: vec![Schema::Asset, Schema::String { max_length: None }],
                }),
                max_length: None,
            },
        }],
    };
    let definition = ComponentDefinition {
        id: "core:Resource".into(),
        source: "scripts/resource.ts".into(),
        export: "Resource".into(),
        fields: vec![ComponentField {
            name: "data".into(),
            schema,
            default: json!({"images":[]}),
            editor: Default::default(),
        }],
    };
    let definitions = std::collections::BTreeMap::from([(definition.id.clone(), definition)]);
    let mut scene=SceneDocument::parse(&serde_json::to_vec(&json!({"version":1,"entities":[{"name":"library://texture.png","components":{"core:Resource":{"data":{"images":[["library://texture.png","unlisted://ordinary-text"]]}}}}]})).expect("JSON")).expect("scene");
    assert!(
        crate::scene::validate_references(&scene, &owner, &vfs, &definitions).is_err(),
        "a transitive dependency is insufficient"
    );
    owner.dependencies.insert("library".into(), "*".into());
    crate::scene::validate_references(&scene, &owner, &vfs, &definitions)
        .expect("direct dependency; ordinary text is not an asset");
    crate::scene::rewrite_asset_references(
        &mut scene,
        &definitions,
        "library://texture.png",
        "library://renamed.png",
    );
    assert_eq!(scene.entities[0].name, "library://texture.png");
    assert_eq!(
        scene.entities[0].components["core:Resource"]["data"]["images"][0],
        json!(["library://renamed.png", "unlisted://ordinary-text"])
    );
    assert!(
        crate::scene::validate_references(&scene, &owner, &vfs, &definitions).is_err(),
        "missing assets are rejected"
    );
}
