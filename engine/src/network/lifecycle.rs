use super::{
    NetworkRegistry, NetworkRequest,
    registry::LivePeer,
    settings::NetworkSettings,
    transport::{Command, Delivery, Event, Transport},
};
use crate::{runtime::RuntimeMode, script::ipc::EngineMessage};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::sync::Arc;

impl NetworkRegistry {
    pub fn shutdown(&mut self) {
        let ids: Vec<_> = self.registrations.keys().cloned().collect();
        for id in ids {
            self.stop_endpoint(&id);
        }
        self.transport.take();
    }
    pub fn new(settings: NetworkSettings) -> Self {
        Self {
            settings,
            ..Self::default()
        }
    }
    /// Deferred requests return None; their response is emitted by poll().
    pub fn dispatch(
        &mut self,
        owner: &str,
        mode: RuntimeMode,
        request: NetworkRequest,
        token: &str,
    ) -> Option<Result<Value>> {
        match request {
            NetworkRequest::Listen {
                registration_id,
                address,
                max_clients,
            } => {
                let operation = (|| {
                    let registration = self
                        .registrations
                        .get(&registration_id)
                        .context("unknown registration")?;
                    ensure!(
                        registration.owner == owner
                            && registration.side == RuntimeMode::Server
                            && mode == RuntimeMode::Server,
                        "listen requires an owned server registration"
                    );
                    ensure!(
                        registration.token.is_none(),
                        "server is already listening or starting"
                    );
                    ensure!(
                        (1..=65_536).contains(&max_clients),
                        "maxClients must be between 1 and 65536"
                    );
                    let command = Command::Listen {
                        registration: registration_id.clone(),
                        token: token.into(),
                        specification: Arc::clone(&registration.compiled),
                        address: address.parse().context("listen address must be IP:port")?,
                        max_clients,
                    };
                    self.start(&registration_id, token, command)
                })();
                operation.err().map(Err)
            }
            NetworkRequest::Connect {
                registration_id,
                address,
                trust,
            } => {
                let operation = (|| {
                    let registration = self
                        .registrations
                        .get(&registration_id)
                        .context("unknown registration")?;
                    ensure!(
                        registration.owner == owner
                            && registration.side == RuntimeMode::Client
                            && mode == RuntimeMode::Client,
                        "connect requires an owned client registration"
                    );
                    ensure!(
                        registration.token.is_none(),
                        "client is already connected or connecting"
                    );
                    ensure!(
                        address.len() <= 512 && !address.is_empty(),
                        "invalid server address"
                    );
                    let command = Command::Connect {
                        registration: registration_id.clone(),
                        token: token.into(),
                        specification: Arc::clone(&registration.compiled),
                        address,
                        trust,
                    };
                    self.start(&registration_id, token, command)
                })();
                operation.err().map(Err)
            }
            request => Some(self.request(owner, mode, request)),
        }
    }
    fn start(&mut self, id: &str, token: &str, command: Command) -> Result<()> {
        if self.transport.is_none() {
            self.transport = Some(Transport::new(self.settings.clone())?);
        }
        self.transport
            .as_ref()
            .context("missing transport")?
            .command(command)?;
        let registration = self
            .registrations
            .get_mut(id)
            .context("missing registration")?;
        registration.token = Some(token.into());
        registration.pending = Some(token.into());
        Ok(())
    }
    pub(super) fn disconnect_peer(&mut self, id: &str, peer_id: &str, reason: &str) {
        let Some(registration) = self.registrations.get_mut(id) else {
            return;
        };
        let Some(peer) = registration.peers.remove(peer_id) else {
            return;
        };
        peer.peer.close(reason);
        let owner = registration.owner.clone();
        self.detach_client(id, peer_id);
        self.outbox.push_back((owner, EngineMessage::NetworkEvent { payload: json!({"event":"disconnected", "registration_id":id,"client_id":peer_id,"reason":reason}) }));
    }
    pub(super) fn stop_endpoint(&mut self, id: &str) {
        let peers = self
            .registrations
            .get(id)
            .map(|r| r.peers.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for peer in peers {
            self.disconnect_peer(id, &peer, "endpoint stopped");
        }
        if let Some(registration) = self.registrations.get_mut(id) {
            registration.token = None;
            registration.listening = false;
            if let Some(token) = registration.pending.take() {
                self.outbox.push_back((
                    registration.owner.clone(),
                    EngineMessage::NetworkResponse {
                        request_id: token,
                        result: None,
                        error: Some("endpoint stopped".into()),
                    },
                ));
            }
        }
        if let Some(transport) = &self.transport
            && let Err(error) = transport.command(Command::Stop(id.into()))
        {
            tracing::warn!("network stop: {error}");
        }
    }
    pub fn poll(&mut self) -> Vec<(String, EngineMessage)> {
        for _ in 0..64 {
            let Some(event) = self.transport.as_mut().and_then(Transport::poll) else {
                break;
            };
            match event {
                Event::Response {
                    registration: id,
                    token,
                    result,
                } => {
                    let Some(registration) = self
                        .registrations
                        .get_mut(&id)
                        .filter(|r| r.token.as_ref() == Some(&token))
                    else {
                        continue;
                    };
                    registration.pending = None;
                    if result.is_ok() {
                        registration.listening = registration.side == RuntimeMode::Server;
                    } else {
                        registration.token = None;
                    }
                    self.outbox.push_back((
                        registration.owner.clone(),
                        EngineMessage::NetworkResponse {
                            request_id: token,
                            result: result.as_ref().ok().cloned(),
                            error: result.err(),
                        },
                    ));
                }
                Event::Connected {
                    registration: id,
                    token,
                    peer,
                    ack,
                    client,
                } => {
                    let Some(registration) = self
                        .registrations
                        .get(&id)
                        .filter(|r| r.token.as_ref() == Some(&token))
                    else {
                        peer.close("registration closed");
                        continue;
                    };
                    let owner = registration.owner.clone();
                    let data = if client {
                        Value::Null
                    } else {
                        match self.attach_client(&id, &peer.id) {
                            Ok(data) => data,
                            Err(error) => {
                                peer.close(&error.to_string());
                                continue;
                            }
                        }
                    };
                    let Some(next) = self.event_counter.checked_add(1) else {
                        peer.close("event IDs exhausted");
                        continue;
                    };
                    self.event_counter = next;
                    let event_id = format!("event:{next}");
                    let info = json!({"address":peer.connection.remote_address().to_string(),"maxDatagramSize":peer.connection.max_datagram_size()});
                    self.outbox.push_back((owner.clone(), EngineMessage::NetworkEvent { payload: json!({"event":"connected","registration_id":id,"client_id":peer.id,"data":data,"event_id":event_id,"connection":info}) }));
                    let registration = self
                        .registrations
                        .get_mut(&id)
                        .expect("registration still exists");
                    registration.peers.insert(
                        peer.id.clone(),
                        LivePeer {
                            peer,
                            ack: Some((event_id, true, ack)),
                        },
                    );
                    if client {
                        registration.pending = None;
                        self.outbox.push_back((
                            owner,
                            EngineMessage::NetworkResponse {
                                request_id: token,
                                result: Some(info),
                                error: None,
                            },
                        ));
                    }
                }
                Event::Packet {
                    registration: id,
                    token,
                    peer,
                    packet,
                    delivery,
                    ack,
                } => {
                    let Some(registration) = self
                        .registrations
                        .get_mut(&id)
                        .filter(|r| r.token.as_ref() == Some(&token))
                    else {
                        continue;
                    };
                    let Some(live) = registration.peers.get_mut(&peer) else {
                        continue;
                    };
                    let Some(next) = self.event_counter.checked_add(1) else {
                        live.peer.close("event IDs exhausted");
                        continue;
                    };
                    self.event_counter = next;
                    let event_id = format!("event:{next}");
                    let delivery = match delivery {
                        Delivery::Stream => "stream",
                        Delivery::Datagram => "datagram",
                    };
                    live.ack = Some((event_id.clone(), false, ack));
                    self.outbox.push_back((registration.owner.clone(), EngineMessage::NetworkEvent { payload: json!({"event":"packet","registration_id":id,"client_id":peer,"event_id":event_id,"packet":packet["packet"],"data":packet["data"],"transport":delivery}) }));
                }
                Event::Disconnected {
                    registration: id,
                    token,
                    peer,
                    reason,
                } => {
                    if !self
                        .registrations
                        .get(&id)
                        .is_some_and(|r| r.token.as_ref() == Some(&token))
                    {
                        continue;
                    }
                    self.disconnect_peer(&id, &peer, &reason);
                    if let Some(registration) = self
                        .registrations
                        .get_mut(&id)
                        .filter(|r| r.side == RuntimeMode::Client)
                    {
                        registration.token = None;
                    }
                }
            }
        }
        self.outbox.drain(..).collect()
    }
}
