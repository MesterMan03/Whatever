use super::{
    codec, framing,
    schema::{CompiledSpecification, Direction, MAX_BYTES},
    security::{self, Trust, TrustStore, Verifier},
    settings::NetworkSettings,
};
use anyhow::{Context, Result, bail, ensure};
use quinn::{Connection, Endpoint, RecvStream, SendStream};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    runtime::Runtime,
    sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot, watch},
};

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Delivery {
    #[default]
    Stream,
    Datagram,
}
pub struct Outgoing {
    bytes: Arc<[u8]>,
    delivery: Delivery,
    _permit: OwnedSemaphorePermit,
}
pub struct Peer {
    pub id: String,
    pub connection: Connection,
    sender: mpsc::Sender<Outgoing>,
    budget: Arc<Semaphore>,
}
impl Peer {
    pub fn reserve(
        &self,
        bytes: Arc<[u8]>,
        delivery: Delivery,
    ) -> Result<(mpsc::OwnedPermit<Outgoing>, Outgoing)> {
        ensure!(
            self.connection.close_reason().is_none(),
            "client is disconnected"
        );
        if matches!(delivery, Delivery::Datagram) {
            ensure!(
                self.connection
                    .max_datagram_size()
                    .is_some_and(|max| bytes.len() <= max),
                "datagram exceeds the connection's current size limit or datagrams are unavailable"
            );
        }
        let permit = Arc::clone(&self.budget)
            .try_acquire_many_owned(bytes.len() as u32)
            .context("outgoing byte queue is full")?;
        let slot = self
            .sender
            .clone()
            .try_reserve_owned()
            .context("outgoing packet queue is full")?;
        Ok((
            slot,
            Outgoing {
                bytes,
                delivery,
                _permit: permit,
            },
        ))
    }
    pub fn close(&self, reason: &str) {
        self.connection
            .close(0u8.into(), truncate(reason).as_bytes());
    }
}
pub enum Event {
    Response {
        registration: String,
        token: String,
        result: std::result::Result<Value, String>,
    },
    Connected {
        registration: String,
        token: String,
        peer: Peer,
        ack: oneshot::Sender<()>,
        client: bool,
    },
    Packet {
        registration: String,
        token: String,
        peer: String,
        packet: Value,
        delivery: Delivery,
        ack: oneshot::Sender<()>,
    },
    Disconnected {
        registration: String,
        token: String,
        peer: String,
        reason: String,
    },
}
pub enum Command {
    Listen {
        registration: String,
        token: String,
        specification: Arc<CompiledSpecification>,
        address: SocketAddr,
        max_clients: usize,
    },
    Connect {
        registration: String,
        token: String,
        specification: Arc<CompiledSpecification>,
        address: String,
        trust: Trust,
    },
    Stop(String),
}
pub struct Transport {
    runtime: Option<Runtime>,
    commands: mpsc::Sender<(Command, watch::Receiver<bool>)>,
    events: mpsc::Receiver<Event>,
    stops: Mutex<HashMap<String, watch::Sender<bool>>>,
}
impl Transport {
    pub fn new(settings: NetworkSettings) -> Result<Self> {
        ensure!(
            !settings.directory.as_os_str().is_empty(),
            "network storage is not configured"
        );
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("whatever-network")
            .enable_all()
            .build()?;
        let (commands, receiver) = mpsc::channel(256);
        let (sender, events) = mpsc::channel(256);
        runtime.spawn(supervise(settings, receiver, sender));
        Ok(Self {
            runtime: Some(runtime),
            commands,
            events,
            stops: Mutex::new(HashMap::new()),
        })
    }
    pub fn command(&self, command: Command) -> Result<()> {
        let mut stops = self
            .stops
            .lock()
            .map_err(|_| anyhow::anyhow!("network cancellation lock poisoned"))?;
        let id = match &command {
            Command::Stop(id) => {
                if let Some(stop) = stops.remove(id) {
                    let _ = stop.send(true);
                }
                return Ok(());
            }
            Command::Listen { registration, .. } | Command::Connect { registration, .. } => {
                registration.clone()
            }
        };
        let (stop, cancellation) = watch::channel(false);
        self.commands
            .try_send((command, cancellation))
            .map_err(|_| anyhow::anyhow!("network command queue is full or closed"))?;
        if let Some(previous) = stops.insert(id, stop) {
            let _ = previous.send(true);
        }
        Ok(())
    }
    pub fn poll(&mut self) -> Option<Event> {
        self.events.try_recv().ok()
    }
}
impl Drop for Transport {
    fn drop(&mut self) {
        if let Ok(stops) = self.stops.lock() {
            for stop in stops.values() {
                let _ = stop.send(true);
            }
        }
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(Duration::from_secs(1));
        }
    }
}
fn truncate(reason: &str) -> String {
    reason.chars().take(160).collect()
}
async fn wait_for_stop(receiver: &mut watch::Receiver<bool>) {
    let _ = receiver.wait_for(|stop| *stop).await;
}
async fn supervise(
    settings: NetworkSettings,
    mut commands: mpsc::Receiver<(Command, watch::Receiver<bool>)>,
    events: mpsc::Sender<Event>,
) {
    let settings = Arc::new(settings);
    let store = Arc::new(TrustStore::new(settings.directory.join("trusted-servers")));
    let ids = Arc::new(AtomicU64::new(0));
    let mut listeners: HashMap<String, tokio::task::JoinHandle<()>> = HashMap::new();
    while let Some((command, cancellation)) = commands.recv().await {
        listeners.retain(|_, task| !task.is_finished());
        if *cancellation.borrow() {
            continue;
        }
        match command {
            Command::Stop(_) => {}
            Command::Listen {
                registration,
                token,
                specification,
                address,
                max_clients,
            } => {
                if let Some(previous) = listeners.remove(&registration)
                    && let Err(error) = previous.await
                {
                    tracing::warn!("previous QUIC listener stopped unexpectedly: {error}");
                }
                let result = security::server_config(&settings)
                    .and_then(|config| Endpoint::server(config, address).map_err(Into::into));
                match result {
                    Ok(endpoint) => {
                        let address = match endpoint.local_addr() {
                            Ok(address) => address.to_string(),
                            Err(error) => {
                                let _ = events
                                    .send(Event::Response {
                                        registration,
                                        token,
                                        result: Err(error.to_string()),
                                    })
                                    .await;
                                continue;
                            }
                        };
                        let _ = events
                            .send(Event::Response {
                                registration: registration.clone(),
                                token: token.clone(),
                                result: Ok(json!({"address": address})),
                            })
                            .await;
                        let listener_id = registration.clone();
                        let task = tokio::spawn(listen(
                            endpoint,
                            max_clients,
                            Session {
                                registration,
                                token,
                                specification,
                                ids: Arc::clone(&ids),
                                events: events.clone(),
                                cancellation,
                                client: false,
                            },
                        ));
                        listeners.insert(listener_id, task);
                    }
                    Err(error) => {
                        let _ = events
                            .send(Event::Response {
                                registration,
                                token,
                                result: Err(format!("listen failed: {error:#}")),
                            })
                            .await;
                    }
                }
            }
            Command::Connect {
                registration,
                token,
                specification,
                address,
                trust,
            } => {
                let settings = Arc::clone(&settings);
                let store = Arc::clone(&store);
                let events = events.clone();
                let ids = Arc::clone(&ids);
                tokio::spawn(async move {
                    let result = connect(
                        &settings,
                        store,
                        &address,
                        trust,
                        &specification,
                        cancellation.clone(),
                    )
                    .await;
                    match result {
                        Ok((endpoint, connection, send, recv, verifier)) => {
                            if let Err(error) = verifier.commit() {
                                connection.close(1u8.into(), b"identity verification failed");
                                let _ = events
                                    .send(Event::Response {
                                        registration,
                                        token,
                                        result: Err(error.to_string()),
                                    })
                                    .await;
                                return;
                            }
                            let _endpoint = endpoint;
                            session(
                                connection,
                                send,
                                recv,
                                Session {
                                    registration,
                                    token,
                                    specification,
                                    ids,
                                    events,
                                    cancellation,
                                    client: true,
                                },
                            )
                            .await;
                        }
                        Err(error) => {
                            let _ = events
                                .send(Event::Response {
                                    registration,
                                    token,
                                    result: Err(format!("connection failed: {error:#}")),
                                })
                                .await;
                        }
                    }
                });
            }
        }
    }
}
fn handshake(specification: &CompiledSpecification) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut bytes = b"WHATEVER/1\0".to_vec();
    bytes.extend_from_slice(&Sha256::digest(specification.canonical.as_bytes()));
    bytes
}
async fn negotiate(
    connection: &Connection,
    specification: &CompiledSpecification,
    client: bool,
) -> Result<(SendStream, RecvStream)> {
    let expected = handshake(specification);
    if client {
        let (mut send, mut recv) = connection.open_bi().await?;
        send.write_all(&expected).await?;
        send.finish()?;
        ensure!(
            recv.read_to_end(128).await? == expected,
            "network specification mismatch"
        );
        let (send, mut recv) = connection.accept_bi().await?;
        let mut header = [255];
        recv.read_exact(&mut header).await?;
        ensure!(header == [0], "invalid application stream header");
        Ok((send, recv))
    } else {
        let (mut send, mut recv) = connection.accept_bi().await?;
        ensure!(
            recv.read_to_end(128).await? == expected,
            "network specification mismatch"
        );
        send.write_all(&expected).await?;
        send.finish()?;
        let (mut send, recv) = connection.open_bi().await?;
        send.write_all(&[0]).await?;
        Ok((send, recv))
    }
}
async fn connect(
    settings: &NetworkSettings,
    store: Arc<TrustStore>,
    address: &str,
    trust: Trust,
    specification: &CompiledSpecification,
    mut cancellation: watch::Receiver<bool>,
) -> Result<(Endpoint, Connection, SendStream, RecvStream, Arc<Verifier>)> {
    let operation = async {
        let (host, port) = address
            .rsplit_once(':')
            .context("address must be host:port (IPv6: [address]:port)")?;
        let host = host.trim_start_matches('[').trim_end_matches(']');
        ensure!(
            !host.is_empty() && address.len() <= 512,
            "invalid server address"
        );
        let port: u16 = port.parse().context("invalid server port")?;
        ensure!(port > 0, "server port must be nonzero");
        let mut addresses: Vec<_> = tokio::net::lookup_host((host, port)).await?.collect();
        addresses.sort_by_key(|address| !address.is_ipv4());
        ensure!(!addresses.is_empty(), "server address has no IP addresses");
        let verifier = Verifier::new(
            settings,
            store,
            format!("{}:{}", host.to_ascii_lowercase(), port),
            trust,
        )?;
        let config = security::client_config(Arc::clone(&verifier))?;
        let mut last_error = None;
        for remote in addresses {
            let bind = if remote.is_ipv4() {
                "0.0.0.0:0"
            } else {
                "[::]:0"
            };
            let mut endpoint = Endpoint::client(bind.parse()?)?;
            endpoint.set_default_client_config(config.clone());
            match tokio::time::timeout(Duration::from_secs(3), async {
                let connection = endpoint.connect(remote, host)?.await?;
                match negotiate(&connection, specification, true).await {
                    Ok((send, recv)) => Ok((connection, send, recv)),
                    Err(error) => {
                        connection.close(1u8.into(), b"specification mismatch");
                        Err(error)
                    }
                }
            })
            .await
            {
                Ok(Ok((connection, send, recv))) => {
                    return Ok((endpoint, connection, send, recv, verifier));
                }
                Ok(Err(error)) => last_error = Some(error),
                Err(error) => last_error = Some(error.into()),
            }
        }
        Err(last_error.context("could not connect")?)
    };
    tokio::select! { result = tokio::time::timeout(Duration::from_secs(10), operation) => result.context("connection timed out")?, _ = wait_for_stop(&mut cancellation) => bail!("connection cancelled") }
}
async fn listen(endpoint: Endpoint, max_clients: usize, mut state: Session) {
    let capacity = Arc::new(Semaphore::new(max_clients));
    loop {
        let incoming = tokio::select! { incoming = endpoint.accept() => incoming, _ = wait_for_stop(&mut state.cancellation) => break };
        let Some(incoming) = incoming else {
            break;
        };
        let Ok(permit) = Arc::clone(&capacity).try_acquire_owned() else {
            incoming.refuse();
            continue;
        };
        let session = state.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let setup = async {
                let connection = incoming.await?;
                match negotiate(&connection, &session.specification, false).await {
                    Ok((send, recv)) => Ok((connection, send, recv)),
                    Err(error) => {
                        connection.close(1u8.into(), b"specification mismatch");
                        Err(error)
                    }
                }
            };
            let mut cancelled = session.cancellation.clone();
            tokio::select! {
                result = tokio::time::timeout(Duration::from_secs(5), setup) => {
                    match result {
                        Ok(Ok((connection, send, recv))) => session_run(connection, send, recv, session).await,
                        Ok(Err(error)) => tracing::debug!("QUIC handshake rejected: {error:#}"),
                        Err(_) => tracing::debug!("QUIC handshake timed out"),
                    }
                }
                _ = wait_for_stop(&mut cancelled) => {}
            }
        });
    }
    endpoint.close(0u8.into(), b"listener stopped");
    let _ = tokio::time::timeout(Duration::from_secs(1), endpoint.wait_idle()).await;
}
#[derive(Clone)]
struct Session {
    registration: String,
    token: String,
    specification: Arc<CompiledSpecification>,
    ids: Arc<AtomicU64>,
    events: mpsc::Sender<Event>,
    cancellation: watch::Receiver<bool>,
    client: bool,
}
async fn session(connection: Connection, send: SendStream, recv: RecvStream, state: Session) {
    session_run(connection, send, recv, state).await;
}
async fn session_run(
    connection: Connection,
    mut send: SendStream,
    mut recv: RecvStream,
    mut state: Session,
) {
    let Ok(id) = state
        .ids
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
    else {
        connection.close(1u8.into(), b"connection IDs exhausted");
        return;
    };
    let id = format!("peer:{}", id + 1);
    let (sender, mut outgoing) = mpsc::channel::<Outgoing>(64);
    let peer = Peer {
        id: id.clone(),
        connection: connection.clone(),
        sender,
        budget: Arc::new(Semaphore::new(4 * MAX_BYTES)),
    };
    let (ack, ready) = oneshot::channel();
    if state
        .events
        .send(Event::Connected {
            registration: state.registration.clone(),
            token: state.token.clone(),
            peer,
            ack,
            client: state.client,
        })
        .await
        .is_err()
    {
        return;
    }
    let traffic = async {
        let writing = async {
            while let Some(packet) = outgoing.recv().await {
                match packet.delivery {
                    Delivery::Stream => framing::write_frame(&mut send, &packet.bytes).await?,
                    Delivery::Datagram => connection.send_datagram(packet.bytes.to_vec().into())?,
                }
            }
            Ok::<(), anyhow::Error>(())
        };
        let reading = async {
            ready.await.context("connection callback cancelled")?;
            loop {
                // Preserve a partially-read frame while datagrams arrive; read_exact is not cancellation safe.
                let frame = framing::read_frame(&mut recv);
                tokio::pin!(frame);
                loop {
                    let (bytes, delivery, complete_frame) = tokio::select! {
                        frame = &mut frame => (frame?.context("peer closed the packet stream")?, Delivery::Stream, true),
                        datagram = connection.read_datagram() => (datagram?.to_vec(), Delivery::Datagram, false),
                    };
                    ensure!(bytes.len() >= 2, "truncated packet ID");
                    let packet_id = usize::from(u16::from_be_bytes([bytes[0], bytes[1]]));
                    let definition = state
                        .specification
                        .packets
                        .get(packet_id)
                        .context("unknown packet ID")?;
                    let direction = if state.client {
                        Direction::Clientbound
                    } else {
                        Direction::Serverbound
                    };
                    ensure!(
                        definition.info.direction == direction,
                        "wrong packet direction"
                    );
                    let packet = codec::decode(&state.specification.packets, &bytes)?;
                    let (ack, handled) = oneshot::channel();
                    state
                        .events
                        .send(Event::Packet {
                            registration: state.registration.clone(),
                            token: state.token.clone(),
                            peer: id.clone(),
                            packet,
                            delivery,
                            ack,
                        })
                        .await
                        .context("network event queue closed")?;
                    handled.await.context("packet callback cancelled")?;
                    if complete_frame {
                        break;
                    }
                }
            }
            #[allow(unreachable_code)]
            Ok::<(), anyhow::Error>(())
        };
        tokio::select! { result = writing => result, result = reading => result }
    };
    // The writer must run during onConnect: its handler can send/await a welcome packet.
    let result = tokio::select! { result = traffic => result, error = connection.closed() => Err(error.into()), _ = wait_for_stop(&mut state.cancellation) => Err(anyhow::anyhow!("endpoint stopped")) };
    let reason = result
        .err()
        .map(|e| truncate(&format!("{e:#}")))
        .unwrap_or_else(|| "connection closed".into());
    connection.close(1u8.into(), reason.as_bytes());
    drop(send);
    drop(recv);
    drop(connection);
    let _ = state
        .events
        .send(Event::Disconnected {
            registration: state.registration,
            token: state.token,
            peer: id,
            reason,
        })
        .await;
}
