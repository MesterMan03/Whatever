use super::{SceneDocument, SceneEntity};
use crate::{
    ecs::ComponentDefinition,
    mods::ModManifest,
    schema::Schema,
    vfs::{Vfs, VfsPath},
};
use anyhow::{Context, ensure};
use serde_json::Value;
use std::collections::BTreeMap;

/// Scene dependencies are explicit and direct, independent of other loaded mods.
pub fn allows_namespace(owner: &ModManifest, target: &str) -> bool {
    target == owner.meta.id || owner.dependencies.contains_key(target)
}

fn native_assets(kind: &str) -> &'static [&'static str] {
    match kind {
        "core:mesh_renderer" => &["mesh", "shader", "texture"],
        "core:sprite_renderer" => &["texture", "shader"],
        "core:text_renderer" => &["font", "shader"],
        _ => &[],
    }
}

/// Update typed asset fields, preserving ordinary strings, names and tags.
pub fn rewrite_asset_references(
    scene: &mut SceneDocument,
    definitions: &BTreeMap<String, ComponentDefinition>,
    old: &str,
    new: &str,
) {
    fn replace(value: &mut Value, old: &str, new: &str) {
        if let Some(text) = value.as_str() {
            let replacement = if text == old {
                Some(new.to_owned())
            } else {
                text.strip_prefix(&format!("{old}/"))
                    .map(|tail| format!("{new}/{tail}"))
            };
            if let Some(text) = replacement {
                *value = Value::String(text);
            }
        }
    }
    fn typed(schema: &Schema, value: &mut Value, old: &str, new: &str) {
        match schema {
            Schema::Asset => replace(value, old, new),
            Schema::Optional { inner } | Schema::Nullable { inner } => {
                typed(inner, value, old, new)
            }
            Schema::Array { element, .. } => {
                if let Some(items) = value.as_array_mut() {
                    for item in items {
                        typed(element, item, old, new);
                    }
                }
            }
            Schema::Tuple { elements } => {
                if let Some(items) = value.as_array_mut() {
                    for (schema, item) in elements.iter().zip(items) {
                        typed(schema, item, old, new);
                    }
                }
            }
            Schema::Struct { fields } => {
                if let Some(values) = value.as_object_mut() {
                    for field in fields {
                        if let Some(value) = values.get_mut(&field.name) {
                            typed(&field.schema, value, old, new);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    fn walk(
        entities: &mut [SceneEntity],
        definitions: &BTreeMap<String, ComponentDefinition>,
        old: &str,
        new: &str,
    ) {
        for entity in entities {
            for (kind, data) in &mut entity.components {
                if let Some(object) = data.as_object_mut() {
                    for field in native_assets(kind) {
                        if let Some(value) = object.get_mut(*field) {
                            replace(value, old, new);
                        }
                    }
                    if let Some(definition) = definitions.get(kind) {
                        for field in &definition.fields {
                            if let Some(value) = object.get_mut(&field.name) {
                                typed(&field.schema, value, old, new);
                            }
                        }
                    }
                }
            }
            walk(&mut entity.children, definitions, old, new);
        }
    }
    walk(&mut scene.entities, definitions, old, new);
}

pub fn validate_asset(owner: &ModManifest, value: &str, vfs: &dyn Vfs) -> anyhow::Result<()> {
    if value.is_empty() {
        return Ok(());
    }
    let path = VfsPath::parse(value).context("expected an asset VFS path")?;
    ensure!(
        allows_namespace(owner, &path.mod_id),
        "mod '{}' must declare a dependency on '{}' to reference '{value}'",
        owner.meta.id,
        path.mod_id
    );
    ensure!(vfs.exists(&path), "missing asset '{value}'");
    Ok(())
}

fn schema_assets(
    schema: &Schema,
    value: &Value,
    owner: &ModManifest,
    vfs: &dyn Vfs,
) -> anyhow::Result<()> {
    match schema {
        Schema::Component { component_type } => {
            let (namespace, _) = component_type
                .split_once(':')
                .context("invalid component reference type")?;
            ensure!(
                allows_namespace(owner, namespace),
                "component reference requires a direct dependency on '{namespace}'"
            );
        }
        Schema::Asset => {
            if let Some(path) = value.as_str() {
                validate_asset(owner, path, vfs)?;
            }
        }
        Schema::Optional { inner } | Schema::Nullable { inner } if !value.is_null() => {
            schema_assets(inner, value, owner, vfs)?
        }
        Schema::Array { element, .. } => {
            if let Some(items) = value.as_array() {
                for item in items {
                    schema_assets(element, item, owner, vfs)?;
                }
            }
        }
        Schema::Tuple { elements } => {
            if let Some(items) = value.as_array() {
                for (schema, item) in elements.iter().zip(items) {
                    schema_assets(schema, item, owner, vfs)?;
                }
            }
        }
        Schema::Struct { fields } => {
            for field in fields {
                schema_assets(&field.schema, &value[&field.name], owner, vfs)
                    .with_context(|| field.name.clone())?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn validate_references(
    scene: &SceneDocument,
    owner: &ModManifest,
    vfs: &dyn Vfs,
    definitions: &BTreeMap<String, ComponentDefinition>,
) -> anyhow::Result<()> {
    fn walk(
        entities: &[SceneEntity],
        owner: &ModManifest,
        vfs: &dyn Vfs,
        definitions: &BTreeMap<String, ComponentDefinition>,
    ) -> anyhow::Result<()> {
        for entity in entities {
            for (kind, data) in &entity.components {
                let check = (|| -> anyhow::Result<()> {
                    if !crate::ecs::is_native_component(kind) {
                        let (namespace, _) = kind
                            .split_once(':')
                            .context("component needs a namespace")?;
                        ensure!(
                            allows_namespace(owner, namespace),
                            "mod '{}' must declare a dependency on '{namespace}' to use component '{kind}'",
                            owner.meta.id
                        );
                    }
                    if let Some(definition) = definitions.get(kind) {
                        for field in &definition.fields {
                            schema_assets(
                                &field.schema,
                                data.get(&field.name).unwrap_or(&field.default),
                                owner,
                                vfs,
                            )?;
                        }
                    }
                    let fields = native_assets(kind);
                    for field in fields {
                        if let Some(path) = data[*field].as_str() {
                            validate_asset(owner, path, vfs)
                                .with_context(|| format!("field '{field}'"))?;
                        }
                    }
                    Ok(())
                })();
                check.with_context(|| {
                    format!(
                        "entity '{}' component '{kind}'",
                        entity.script_id.as_deref().unwrap_or(&entity.name)
                    )
                })?;
            }
            walk(&entity.children, owner, vfs, definitions)?;
        }
        Ok(())
    }
    walk(&scene.entities, owner, vfs, definitions)
}
