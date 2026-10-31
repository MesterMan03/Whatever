use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashSet;

pub const MAX_BYTES: usize = 1_048_576;
pub const MAX_ITEMS: usize = 65_536;
pub const MAX_NODES: usize = 100_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Schema {
    I32 {
        min: Option<f64>,
        max: Option<f64>,
    },
    F64 {
        min: Option<f64>,
        max: Option<f64>,
    },
    Varint {
        min: Option<f64>,
        max: Option<f64>,
    },
    Bool,
    Nullable {
        inner: Box<Schema>,
    },
    Tuple {
        elements: Vec<Schema>,
    },
    Enum {
        values: Vec<String>,
    },
    Entity,
    Asset,
    String {
        max_length: Option<usize>,
    },
    Array {
        element: Box<Schema>,
        max_length: Option<usize>,
    },
    Optional {
        inner: Box<Schema>,
    },
    Struct {
        fields: Vec<Field>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub name: String,
    pub schema: Schema,
}
fn name(value: &str) -> bool {
    if value.len() > 128 {
        return false;
    }
    let mut chars = value.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !matches!(value, "__proto__" | "constructor" | "prototype")
}
fn charge(budget: &mut usize) -> Result<()> {
    ensure!(*budget > 0, "schema/value exceeds the node limit");
    *budget -= 1;
    Ok(())
}
impl Schema {
    pub fn validate_definition(&self, depth: usize, budget: &mut usize) -> Result<()> {
        ensure!(depth <= 32, "schema nesting exceeds 32 levels");
        charge(budget)?;
        match self {
            Self::I32 { min, max } | Self::F64 { min, max } | Self::Varint { min, max } => {
                for value in [min, max].into_iter().flatten() {
                    ensure!(value.is_finite(), "numeric bounds must be finite");
                    if !matches!(self, Self::F64 { .. }) {
                        ensure!(
                            value.fract() == 0.0
                                && *value >= i32::MIN as f64
                                && *value <= i32::MAX as f64,
                            "integer bounds must fit i32"
                        );
                    }
                }
                ensure!(
                    !matches!((min,max),(Some(a),Some(b)) if a>b),
                    "min exceeds max"
                );
            }
            Self::Array {
                element,
                max_length,
            } => {
                ensure!(
                    max_length.is_none_or(|n| n <= MAX_ITEMS),
                    "array limit exceeds {MAX_ITEMS}"
                );
                element.validate_definition(depth + 1, budget)?;
            }
            Self::String { max_length } => ensure!(
                max_length.is_none_or(|n| n <= MAX_BYTES),
                "string limit exceeds {MAX_BYTES} bytes"
            ),
            Self::Optional { inner } => {
                ensure!(
                    !matches!(
                        inner.as_ref(),
                        Self::Optional { .. } | Self::Nullable { .. }
                    ),
                    "nested optional types are ambiguous"
                );
                inner.validate_definition(depth + 1, budget)?;
            }
            Self::Struct { fields } => {
                let mut names = HashSet::new();
                for field in fields {
                    ensure!(
                        name(&field.name) && names.insert(&field.name),
                        "invalid or duplicate field '{}'",
                        field.name
                    );
                    field.schema.validate_definition(depth + 1, budget)?;
                }
            }
            Self::Nullable { inner } => {
                ensure!(
                    !matches!(
                        inner.as_ref(),
                        Self::Optional { .. } | Self::Nullable { .. }
                    ),
                    "ambiguous nullable type"
                );
                inner.validate_definition(depth + 1, budget)?;
            }
            Self::Tuple { elements } => {
                ensure!(
                    !elements.is_empty() && elements.len() <= 32,
                    "tuple needs 1–32 elements"
                );
                for element in elements {
                    element.validate_definition(depth + 1, budget)?;
                }
            }
            Self::Enum { values } => {
                let unique: HashSet<_> = values.iter().collect();
                ensure!(
                    !values.is_empty() && values.len() <= 1024 && unique.len() == values.len(),
                    "invalid enum choices"
                );
            }
            Self::Bool | Self::Entity | Self::Asset => {}
        }
        Ok(())
    }
    pub fn normalize(&self, value: &Value) -> Result<Value> {
        let mut budget = MAX_NODES;
        self.normalize_inner(value, &mut budget)
    }
    fn normalize_inner(&self, value: &Value, budget: &mut usize) -> Result<Value> {
        charge(budget)?;
        match self {
            Self::Nullable { inner } => {
                if value.is_null() {
                    Ok(Value::Null)
                } else {
                    inner.normalize_inner(value, budget)
                }
            }
            Self::Tuple { elements } => {
                let values = value
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("expected tuple"))?;
                ensure!(values.len() == elements.len(), "tuple length mismatch");
                Ok(Value::Array(
                    elements
                        .iter()
                        .zip(values)
                        .map(|(schema, value)| schema.normalize_inner(value, budget))
                        .collect::<Result<_>>()?,
                ))
            }
            Self::Enum { values } => {
                ensure!(
                    value
                        .as_str()
                        .is_some_and(|v| values.iter().any(|s| s == v)),
                    "invalid enum choice"
                );
                Ok(value.clone())
            }
            Self::Entity => {
                ensure!(
                    value.as_object().is_some_and(|o| o.len() == 1)
                        && value
                            .get("$entity")
                            .and_then(Value::as_str)
                            .is_some_and(|s| !s.is_empty() && s.len() <= 256),
                    "expected entity reference"
                );
                Ok(value.clone())
            }
            Self::Asset => {
                let asset = value
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("expected asset path"))?;
                ensure!(
                    asset.is_empty() || crate::vfs::VfsPath::parse(asset).is_some(),
                    "expected VFS asset path"
                );
                Ok(value.clone())
            }
            Self::Optional { inner } => {
                if value.is_null() {
                    Ok(Value::Null)
                } else {
                    inner.normalize_inner(value, budget)
                }
            }
            Self::I32 { min, max } | Self::F64 { min, max } | Self::Varint { min, max } => {
                let n = value
                    .as_f64()
                    .ok_or_else(|| anyhow::anyhow!("expected number"))?;
                ensure!(n.is_finite(), "number must be finite");
                if !matches!(self, Self::F64 { .. }) {
                    ensure!(
                        n.fract() == 0.0 && n >= i32::MIN as f64 && n <= i32::MAX as f64,
                        "expected i32 integer"
                    );
                }
                ensure!(
                    min.is_none_or(|m| n >= m) && max.is_none_or(|m| n <= m),
                    "number {n} is outside declared bounds"
                );
                Ok(value.clone())
            }
            Self::Bool => {
                ensure!(value.is_boolean(), "expected bool");
                Ok(value.clone())
            }
            Self::String { max_length } => {
                let text = value
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("expected string"))?;
                ensure!(
                    text.len() <= max_length.unwrap_or(MAX_BYTES),
                    "UTF-8 string exceeds length limit"
                );
                Ok(value.clone())
            }
            Self::Array {
                element,
                max_length,
            } => {
                let values = value
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("expected array"))?;
                ensure!(
                    values.len() <= max_length.unwrap_or(MAX_ITEMS),
                    "array exceeds length limit"
                );
                Ok(Value::Array(
                    values
                        .iter()
                        .map(|v| element.normalize_inner(v, budget))
                        .collect::<Result<_>>()?,
                ))
            }
            Self::Struct { fields } => {
                let object = value
                    .as_object()
                    .ok_or_else(|| anyhow::anyhow!("expected object"))?;
                ensure!(
                    object
                        .keys()
                        .all(|key| fields.iter().any(|f| f.name == *key)),
                    "object contains undeclared fields"
                );
                let mut out = Map::new();
                for field in fields {
                    let val = object.get(&field.name).unwrap_or(&Value::Null);
                    let normalized = field
                        .schema
                        .normalize_inner(val, budget)
                        .map_err(|e| anyhow::anyhow!("{}: {e}", field.name))?;
                    out.insert(field.name.clone(), normalized);
                }
                Ok(Value::Object(out))
            }
        }
    }
}

impl Schema {
    pub fn validate_network(&self) -> Result<()> {
        match self {
            Self::Array { element, .. } => element.validate_network(),
            Self::Optional { inner } => inner.validate_network(),
            Self::Struct { fields } => {
                for field in fields {
                    field.schema.validate_network()?;
                }
                Ok(())
            }
            Self::I32 { .. }
            | Self::F64 { .. }
            | Self::Varint { .. }
            | Self::Bool
            | Self::String { .. } => Ok(()),
            _ => anyhow::bail!("type is not supported in network schemas"),
        }
    }
}
