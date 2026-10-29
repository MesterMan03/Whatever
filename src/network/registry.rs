use super::{
    codec,
    schema::{
        ClientDataDefinition, CompiledSpecification, Direction, MAX_BYTES, MAX_NODES, PacketInfo,
        Specification,
    },
};
use crate::runtime::RuntimeMode;
use anyhow::{Result, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub enum NetworkRequest {
    Register {
        specification: Specification,
        side: RuntimeMode,
        client_data: Option<ClientDataDefinition>,
    },
    Encode {
        registration_id: String,
        packet: String,
        data: Value,
    },
    Decode {
        registration_id: String,
        bytes: String,
    },
    ValidateClientData {
        registration_id: String,
        data: Value,
    },
    UpdateClientData {
        registration_id: String,
        client_id: String,
        data: Value,
    },
    Send {
        registration_id: String,
        client_id: Option<String>,
        packet: String,
        data: Value,
    },
    Close {
        registration_id: String,
    },
}
struct Registration {
    owner: String,
    side: RuntimeMode,
    specification_id: String,
    compiled: CompiledSpecification,
    client_data: Option<ClientDataDefinition>,
    clients: HashMap<String, Value>,
}
#[derive(Default)]
pub struct NetworkRegistry {
    registrations: HashMap<String, Registration>,
    next: u64,
}
impl NetworkRegistry {
    pub fn request(
        &mut self,
        owner: &str,
        mode: RuntimeMode,
        request: NetworkRequest,
    ) -> Result<Value> {
        if let NetworkRequest::Register {
            specification,
            side,
            client_data,
        } = request
        {
            ensure!(
                side == mode,
                "network registration side does not match Engine.mode"
            );
            ensure!(
                side == RuntimeMode::Server || client_data.is_none(),
                "client data belongs to server registrations"
            );
            let compiled = specification.compile(owner)?;
            if let Some(data) = &client_data {
                ensure!(
                    matches!(data.schema, super::schema::Schema::Struct { .. }),
                    "client data must be a struct"
                );
                let mut budget = MAX_NODES;
                data.schema.validate_definition(0, &mut budget)?;
                data.schema.normalize(&data.initial)?;
                ensure!(
                    serde_json::to_vec(&data.initial)?.len() <= MAX_BYTES,
                    "client defaults exceed size limit"
                );
            }
            ensure!(
                self.registrations.len() < 1024,
                "too many network registrations"
            );
            for registration in self.registrations.values() {
                if registration.specification_id == specification.id {
                    ensure!(
                        registration.compiled.canonical == compiled.canonical,
                        "conflicting specification definition"
                    );
                    bail!("specification is already registered by this mod");
                }
            }
            self.next = self
                .next
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("registration IDs exhausted"))?;
            let id = format!("network:{}", self.next);
            let packets: Vec<PacketInfo> =
                compiled.packets.iter().map(|p| p.info.clone()).collect();
            let response = json!({"registration_id":id,"specification_id":specification.id,"fingerprint":compiled.fingerprint,"side":side,"packets":packets});
            self.registrations.insert(
                id,
                Registration {
                    owner: owner.to_owned(),
                    side,
                    specification_id: specification.id,
                    compiled,
                    client_data,
                    clients: HashMap::new(),
                },
            );
            return Ok(response);
        }
        let id = match &request {
            NetworkRequest::Encode {
                registration_id, ..
            }
            | NetworkRequest::Decode {
                registration_id, ..
            }
            | NetworkRequest::ValidateClientData {
                registration_id, ..
            }
            | NetworkRequest::UpdateClientData {
                registration_id, ..
            }
            | NetworkRequest::Send {
                registration_id, ..
            }
            | NetworkRequest::Close { registration_id } => registration_id,
            NetworkRequest::Register { .. } => bail!("unexpected registration"),
        };
        let registration = self
            .registrations
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("unknown or closed network registration"))?;
        ensure!(
            registration.owner == owner,
            "network registration belongs to another mod"
        );
        match request {
            NetworkRequest::Encode { packet, data, .. } => {
                let direction = if registration.side == RuntimeMode::Client {
                    Direction::Serverbound
                } else {
                    Direction::Clientbound
                };
                let definition = registration
                    .compiled
                    .packets
                    .iter()
                    .find(|p| p.info.packet == packet && p.info.direction == direction)
                    .ok_or_else(|| anyhow::anyhow!("unknown outgoing packet '{packet}'"))?;
                let bytes = codec::encode(definition, &data)?;
                Ok(json!({"bytes":STANDARD.encode(bytes)}))
            }
            NetworkRequest::Send {
                client_id,
                packet,
                data,
                ..
            } => {
                if registration.side == RuntimeMode::Server {
                    let client_id = client_id
                        .ok_or_else(|| anyhow::anyhow!("server sends require a client"))?;
                    ensure!(
                        registration.clients.contains_key(&client_id),
                        "unknown or disconnected client"
                    );
                } else {
                    ensure!(
                        client_id.is_none(),
                        "client sends cannot specify a recipient"
                    );
                }
                let direction = if registration.side == RuntimeMode::Client {
                    Direction::Serverbound
                } else {
                    Direction::Clientbound
                };
                let definition = registration
                    .compiled
                    .packets
                    .iter()
                    .find(|p| p.info.packet == packet && p.info.direction == direction)
                    .ok_or_else(|| anyhow::anyhow!("unknown outgoing packet '{packet}'"))?;
                codec::encode(definition, &data)?;
                bail!(
                    "Network transport is not implemented; use encodePacket for offline validation"
                )
            }
            NetworkRequest::Decode { bytes, .. } => {
                ensure!(
                    bytes.len() <= MAX_BYTES.div_ceil(3) * 4,
                    "encoded packet exceeds size limit"
                );
                codec::decode(&registration.compiled.packets, &STANDARD.decode(bytes)?)
            }
            NetworkRequest::ValidateClientData { data, .. } => {
                let definition = registration
                    .client_data
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("registration has no client data"))?;
                Ok(definition.schema.normalize(&data)?)
            }
            NetworkRequest::UpdateClientData {
                client_id, data, ..
            } => {
                ensure!(
                    registration.clients.contains_key(&client_id),
                    "unknown or disconnected client"
                );
                let definition = registration
                    .client_data
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("registration has no client data"))?;
                let data = definition.schema.normalize(&data)?;
                ensure!(
                    serde_json::to_vec(&data)?.len() <= MAX_BYTES,
                    "client data exceeds size limit"
                );
                registration.clients.insert(client_id, data);
                Ok(Value::Null)
            }
            NetworkRequest::Close { registration_id } => {
                self.registrations.remove(&registration_id);
                Ok(Value::Null)
            }
            NetworkRequest::Register { .. } => bail!("unexpected registration"),
        }
    }
    /// Reserved for the future transport's trusted connection lifecycle.
    #[allow(dead_code)]
    pub fn attach_client(&mut self, registration_id: &str, client_id: &str) -> Result<Value> {
        let registration = self
            .registrations
            .get_mut(registration_id)
            .ok_or_else(|| anyhow::anyhow!("unknown registration"))?;
        ensure!(
            registration.side == RuntimeMode::Server,
            "clients belong to server registrations"
        );
        ensure!(
            !registration.clients.contains_key(client_id),
            "client already attached"
        );
        let definition = registration
            .client_data
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing client data schema"))?;
        let data = definition.schema.normalize(&definition.initial)?;
        registration
            .clients
            .insert(client_id.to_owned(), data.clone());
        Ok(data)
    }
    #[allow(dead_code)]
    pub fn detach_client(&mut self, registration_id: &str, client_id: &str) -> Option<Value> {
        self.registrations
            .get_mut(registration_id)?
            .clients
            .remove(client_id)
    }
    pub fn remove_mod(&mut self, owner: &str) {
        self.registrations.retain(|_, r| r.owner != owner);
    }
}
