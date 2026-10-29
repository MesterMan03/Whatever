use super::{
    codec,
    schema::{
        ClientDataDefinition, CompiledSpecification, Direction, MAX_BYTES, MAX_NODES, PacketInfo,
        Specification,
    },
};
use super::{
    security::Trust,
    settings::NetworkSettings,
    transport::{Delivery, Peer, Transport},
};
use crate::runtime::RuntimeMode;
use crate::script::ipc::EngineMessage;
use anyhow::{Context, Result, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};
use tokio::sync::oneshot;

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
        #[serde(default)]
        delivery: Delivery,
        packet: String,
        data: Value,
    },
    Listen {
        registration_id: String,
        address: String,
        #[serde(default = "default_max_clients")]
        max_clients: usize,
    },
    Connect {
        registration_id: String,
        address: String,
        #[serde(default)]
        trust: Trust,
    },
    Stop {
        registration_id: String,
    },
    Disconnect {
        registration_id: String,
        client_id: Option<String>,
        reason: Option<String>,
    },
    Broadcast {
        registration_id: String,
        packet: String,
        data: Value,
        #[serde(default)]
        delivery: Delivery,
    },
    EventDone {
        registration_id: String,
        client_id: String,
        event_id: String,
        error: Option<String>,
    },
    Close {
        registration_id: String,
    },
}
pub(super) fn default_max_clients() -> usize {
    128
}
pub(super) struct Registration {
    pub(super) owner: String,
    pub(super) side: RuntimeMode,
    pub(super) specification_id: String,
    pub(super) compiled: Arc<CompiledSpecification>,
    pub(super) client_data: Option<ClientDataDefinition>,
    pub(super) clients: HashMap<String, Value>,
    pub(super) token: Option<String>,
    pub(super) pending: Option<String>,
    pub(super) listening: bool,
    pub(super) peers: HashMap<String, LivePeer>,
}
pub(super) struct LivePeer {
    pub peer: Peer,
    pub ack: Option<(String, bool, oneshot::Sender<()>)>,
}
#[derive(Default)]
pub struct NetworkRegistry {
    pub(super) settings: NetworkSettings,
    pub(super) transport: Option<Transport>,
    pub(super) outbox: VecDeque<(String, EngineMessage)>,
    pub(super) event_counter: u64,
    pub(super) registrations: HashMap<String, Registration>,
    pub(super) next: u64,
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
                    compiled: Arc::new(compiled),
                    token: None,
                    pending: None,
                    listening: false,
                    peers: HashMap::new(),
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
            | NetworkRequest::Listen {
                registration_id, ..
            }
            | NetworkRequest::Connect {
                registration_id, ..
            }
            | NetworkRequest::Stop { registration_id }
            | NetworkRequest::Disconnect {
                registration_id, ..
            }
            | NetworkRequest::Broadcast {
                registration_id, ..
            }
            | NetworkRequest::EventDone {
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
                delivery,
                client_id,
                packet,
                data,
                ..
            } => {
                if registration.side == RuntimeMode::Server {
                    let client_id = client_id
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("server sends require a client"))?;
                    ensure!(
                        registration.clients.contains_key(client_id.as_str()),
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
                let bytes: Arc<[u8]> = codec::encode(definition, &data)?.into();
                let peer = match client_id.as_ref() {
                    Some(id) => registration.peers.get(id),
                    None => registration.peers.values().next(),
                }
                .ok_or_else(|| anyhow::anyhow!("not connected"))?;
                let (slot, packet) = peer.peer.reserve(bytes, delivery)?;
                slot.send(packet);
                Ok(Value::Null)
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
            NetworkRequest::Broadcast {
                packet,
                data,
                delivery,
                ..
            } => {
                ensure!(
                    registration.side == RuntimeMode::Server && registration.listening,
                    "server is not listening"
                );
                let definition = registration
                    .compiled
                    .packets
                    .iter()
                    .find(|p| p.info.packet == packet && p.info.direction == Direction::Clientbound)
                    .ok_or_else(|| anyhow::anyhow!("unknown outgoing packet"))?;
                let bytes: Arc<[u8]> = codec::encode(definition, &data)?.into();
                let reservations = registration
                    .peers
                    .values()
                    .map(|peer| peer.peer.reserve(Arc::clone(&bytes), delivery))
                    .collect::<Result<Vec<_>>>()?;
                for (slot, packet) in reservations {
                    slot.send(packet);
                }
                Ok(Value::Null)
            }
            NetworkRequest::EventDone {
                client_id,
                event_id,
                error,
                ..
            } => {
                let peer = registration.peers.get_mut(&client_id).ok_or_else(|| {
                    anyhow::anyhow!("client disconnected before callback completed")
                })?;
                ensure!(
                    peer.ack.as_ref().is_some_and(|(id, _, _)| *id == event_id),
                    "unknown callback acknowledgement"
                );
                let (_, connected, ack) = peer
                    .ack
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("missing callback"))?;
                if connected && error.is_some() {
                    peer.peer.close("connection callback failed");
                } else {
                    let _ = ack.send(());
                }
                Ok(Value::Null)
            }
            NetworkRequest::Disconnect {
                registration_id,
                client_id,
                reason,
            } => {
                ensure!(
                    reason.as_ref().is_none_or(|s| s.len() <= 512),
                    "disconnect reason too long"
                );
                let id = if registration.side == RuntimeMode::Client {
                    ensure!(
                        client_id.is_none(),
                        "client disconnect cannot select a peer"
                    );
                    registration.peers.keys().next().cloned()
                } else {
                    Some(client_id.context("server disconnect requires a client")?)
                };
                if let Some(id) = id {
                    self.disconnect_peer(
                        &registration_id,
                        &id,
                        reason.as_deref().unwrap_or("disconnected"),
                    );
                }
                if self
                    .registrations
                    .get(&registration_id)
                    .is_some_and(|r| r.side == RuntimeMode::Client)
                {
                    self.stop_endpoint(&registration_id);
                }
                Ok(Value::Null)
            }
            NetworkRequest::Stop { registration_id } => {
                self.stop_endpoint(&registration_id);
                Ok(Value::Null)
            }
            NetworkRequest::Listen { .. } | NetworkRequest::Connect { .. } => {
                bail!("transport setup requires asynchronous dispatch")
            }
            NetworkRequest::Close { registration_id } => {
                self.stop_endpoint(&registration_id);
                self.registrations.remove(&registration_id);
                Ok(Value::Null)
            }
            NetworkRequest::Register { .. } => bail!("unexpected registration"),
        }
    }
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
    pub fn detach_client(&mut self, registration_id: &str, client_id: &str) -> Option<Value> {
        self.registrations
            .get_mut(registration_id)?
            .clients
            .remove(client_id)
    }
    pub fn remove_mod(&mut self, owner: &str) {
        let ids: Vec<_> = self
            .registrations
            .iter()
            .filter(|(_, r)| r.owner == owner)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.stop_endpoint(&id);
            self.registrations.remove(&id);
        }
    }
}
