use crate::schema::{Field, MAX_NODES, Schema};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldEditor {
    pub label: Option<String>,
    pub tooltip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentField {
    pub name: String,
    pub schema: Schema,
    pub default: Value,
    #[serde(default)]
    pub editor: FieldEditor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentDefinition {
    pub id: String,
    pub source: String,
    pub export: String,
    pub fields: Vec<ComponentField>,
}

/// Exact native IDs are reserved; the game's core mod may define other components.
pub fn is_native_component(id: &str) -> bool {
    matches!(
        id,
        "core:entity"
            | "core:transform"
            | "core:world_transform"
            | "core:camera"
            | "core:sprite_renderer"
            | "core:mesh_renderer"
            | "core:text_renderer"
            | "core:ambient_light"
            | "core:directional_light"
            | "core:point_light"
            | "core:physics_world"
            | "core:rigid_body"
            | "core:rigid_body_state"
            | "core:collider"
            | "core:joint"
            | "core:physics_status"
    )
}

impl ComponentDefinition {
    pub fn validate(&self, owner: &str) -> Result<()> {
        let (namespace, name) = self
            .id
            .split_once(':')
            .context("component ID must be mod_id:name")?;
        ensure!(
            namespace == owner && !name.is_empty() && name.len() <= 128,
            "component must use its owning mod's namespace"
        );
        ensure!(
            !is_native_component(&self.id),
            "component '{}' is reserved for the native engine",
            self.id
        );
        ensure!(
            !self.source.is_empty() && !self.export.is_empty(),
            "component needs an exported source class"
        );
        let schema = Schema::Struct {
            fields: self
                .fields
                .iter()
                .map(|f| Field {
                    name: f.name.clone(),
                    schema: f.schema.clone(),
                })
                .collect(),
        };
        let mut budget = MAX_NODES;
        schema.validate_definition(0, &mut budget)?;
        for field in &self.fields {
            ensure!(
                !matches!(
                    field.name.as_str(),
                    "id" | "entity" | "start" | "tick" | "destroy" | "onEnable" | "onDisable"
                ),
                "reserved component field '{}'",
                field.name
            );
            field
                .schema
                .normalize(&field.default)
                .with_context(|| format!("default for {}.{}", self.id, field.name))?;
        }
        Ok(())
    }

    pub fn defaults(&self) -> Value {
        Value::Object(
            self.fields
                .iter()
                .map(|f| (f.name.clone(), f.default.clone()))
                .collect(),
        )
    }

    pub fn normalize(&self, value: &Value) -> Result<Value> {
        let object = value
            .as_object()
            .context("component data must be an object")?;
        ensure!(
            object
                .keys()
                .all(|key| self.fields.iter().any(|f| f.name == *key)),
            "component contains undeclared fields"
        );
        let mut result = Map::new();
        for field in &self.fields {
            result.insert(
                field.name.clone(),
                field
                    .schema
                    .normalize(object.get(&field.name).unwrap_or(&field.default))
                    .with_context(|| format!("{}.{}", self.id, field.name))?,
            );
        }
        Ok(Value::Object(result))
    }
}

impl super::World {
    pub fn allow_missing_components_for_editor(&mut self) {
        self.allow_missing_components = true;
    }

    pub fn register_component(
        &mut self,
        owner: &str,
        definition: ComponentDefinition,
    ) -> Result<()> {
        definition.validate(owner)?;
        ensure!(
            !self.component_types.contains_key(&definition.id),
            "duplicate component '{}'",
            definition.id
        );
        self.component_types
            .insert(definition.id.clone(), definition);
        Ok(())
    }
}

#[cfg(test)]
pub(crate) use tests::link as register_test_component;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use serde_json::json;

    pub(crate) fn link(world: &mut World, id: &str, fields: &[(&str, Schema, Value)]) {
        let owner = id.split_once(':').expect("namespace").0;
        world
            .register_component(
                owner,
                ComponentDefinition {
                    id: id.into(),
                    source: "scripts/components.ts".into(),
                    export: "Link".into(),
                    fields: fields
                        .iter()
                        .map(|(name, schema, default)| ComponentField {
                            name: (*name).into(),
                            schema: schema.clone(),
                            default: default.clone(),
                            editor: FieldEditor::default(),
                        })
                        .collect(),
                },
            )
            .expect("register component");
    }

    #[test]
    fn custom_components_reject_unknown_types_and_validate_every_write() {
        let mut world = World::new();
        let entity = world.create_entity();
        assert!(!world.set_component(&entity, "test:missing", json!({})));
        link(
            &mut world,
            "test:health",
            &[(
                "health",
                Schema::I32 {
                    min: Some(0.0),
                    max: Some(100.0),
                },
                json!(100),
            )],
        );
        assert!(world.set_component(&entity, "test:health", json!({})));
        let first_instance = world.component_instances["test:health"][&entity.index];
        assert!(world.set_component(&entity, "test:health", json!({"health":42})));
        assert_eq!(
            world.component_instances["test:health"][&entity.index],
            first_instance
        );
        assert!(world.remove_component(&entity, "test:health"));
        assert!(world.set_component(&entity, "test:health", json!({})));
        assert_ne!(
            world.component_instances["test:health"][&entity.index],
            first_instance
        );
        assert_eq!(
            world.get_component(&entity, "test:health"),
            Some(json!({"health":100}))
        );
        assert!(!world.set_component(&entity, "test:health", json!({"health":"wrong"})));
        assert!(!world.set_component(&entity, "test:health", json!({"health":101})));
        assert!(!world.set_component(&entity, "test:health", json!({"extra":true})));
        assert_eq!(
            world.get_component(&entity, "test:health"),
            Some(json!({"health":100}))
        );
    }
}
