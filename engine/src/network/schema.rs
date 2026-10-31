pub use crate::schema::{Field, MAX_BYTES, MAX_ITEMS, MAX_NODES, Schema};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

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
                    schema.validate_network()?;
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
        let hash = super::security::digest(canonical.as_bytes());
        Ok(CompiledSpecification {
            packets,
            canonical,
            fingerprint: format!("sha256:{hash}"),
        })
    }
}
