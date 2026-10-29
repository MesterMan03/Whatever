use anyhow::{Result, bail, ensure};
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PacketDefinition {
    pub name: String,
    pub fields: Vec<Field>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StateDefinition {
    pub name: String,
    pub serverbound: Vec<PacketDefinition>,
    pub clientbound: Vec<PacketDefinition>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Specification {
    pub id: String,
    pub states: Vec<StateDefinition>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientDataDefinition {
    pub schema: Schema,
    pub initial: Value,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Serverbound,
    Clientbound,
}
#[derive(Debug, Clone, Serialize)]
pub struct PacketInfo {
    pub id: u16,
    pub packet: String,
    pub state: String,
    pub direction: Direction,
}
#[derive(Debug, Clone)]
pub struct CompiledPacket {
    pub info: PacketInfo,
    pub schema: Schema,
}
#[derive(Debug, Clone)]
pub struct CompiledSpecification {
    pub packets: Vec<CompiledPacket>,
    pub canonical: String,
    pub fingerprint: String,
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
                    !matches!(inner.as_ref(), Self::Optional { .. }),
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
            Self::Bool => {}
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
impl Specification {
    pub fn compile(&self, owner: &str) -> Result<CompiledSpecification> {
        let Some((namespace, key)) = self.id.split_once(':') else {
            bail!("specification ID must be mod_id:name")
        };
        ensure!(
            namespace == owner && name(key),
            "specification must use the registering mod's namespace"
        );
        ensure!(
            self.states.len() <= 1024,
            "specification exceeds 1024 states"
        );
        let mut states = HashSet::new();
        let mut packets = Vec::new();
        let mut budget = MAX_NODES;
        for state in &self.states {
            ensure!(
                name(&state.name) && states.insert(&state.name),
                "invalid or duplicate state '{}'",
                state.name
            );
            for (direction, definitions) in [
                (Direction::Serverbound, &state.serverbound),
                (Direction::Clientbound, &state.clientbound),
            ] {
                let mut names = HashSet::new();
                for packet in definitions {
                    ensure!(
                        name(&packet.name) && names.insert(&packet.name),
                        "invalid or duplicate packet '{}'",
                        packet.name
                    );
                    ensure!(
                        packets.len() < 65_536,
                        "specification exceeds 65536 packets"
                    );
                    let schema = Schema::Struct {
                        fields: packet.fields.clone(),
                    };
                    schema.validate_definition(0, &mut budget)?;
                    packets.push(CompiledPacket {
                        info: PacketInfo {
                            id: packets.len() as u16,
                            packet: format!("{}.{}", state.name, packet.name),
                            state: state.name.clone(),
                            direction,
                        },
                        schema,
                    });
                }
            }
        }
        ensure!(!packets.is_empty(), "specification must contain packets");
        let canonical = serde_json::to_string(self)?;
        // Diagnostic compatibility fingerprint. Registry identity uses the full manifest,
        // never this non-cryptographic digest; transport negotiation is not implemented.
        let hash = canonical.bytes().fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
        });
        Ok(CompiledSpecification {
            packets,
            canonical,
            fingerprint: format!("fnv1a64:{hash:016x}"),
        })
    }
}
