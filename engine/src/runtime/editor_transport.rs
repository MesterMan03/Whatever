//! Private loopback transport for an editor-owned player, independent of mod IPC.
use super::{
    bridge::RuntimeCommand,
    editor::{EditorBatch, EditorControl, EditorReply},
};
use crate::ecs::World;
use anyhow::{Context, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
#[cfg(test)]
use std::time::Instant;
use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::Duration,
};
const LIMIT: usize = 32 * 1024 * 1024;
const PROTOCOL: u32 = 1;
#[derive(Serialize, Deserialize)]
struct Hello {
    protocol: u32,
    token: String,
}
#[derive(Serialize, Deserialize)]
enum Request {
    Inspect(bool),
    Edit { sequence: u64, batch: EditorBatch },
}
#[derive(Serialize, Deserialize)]
enum Update {
    Snapshot(Box<EditorSnapshot>),
    Reply(EditorReply),
}
fn write_frame(stream: &mut TcpStream, value: &impl Serialize) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    ensure!(bytes.len() <= LIMIT, "editor message exceeds size limit");
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(&bytes)?;
    Ok(())
}
fn read_frame<T: DeserializeOwned>(stream: &mut TcpStream) -> anyhow::Result<T> {
    let mut size = [0; 4];
    stream.read_exact(&mut size)?;
    let size = u32::from_be_bytes(size) as usize;
    ensure!(size <= LIMIT, "editor message exceeds size limit");
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}
pub type EditorMailbox = Mutex<Option<EditorSnapshot>>;
#[derive(Serialize, Deserialize)]
pub struct EditorSnapshot {
    pub world: World,
    pub camera: Option<crate::ecs::EntityId>,
    pub sequence: u64,
}
#[derive(Default)]
struct Inbox {
    snapshot: Option<EditorSnapshot>,
    replies: VecDeque<EditorReply>,
    error: Option<String>,
}
struct Life {
    stop: AtomicBool,
    socket: Mutex<Option<TcpStream>>,
}
impl Life {
    fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(socket) = self.socket.lock()
            && let Some(socket) = socket.as_ref()
        {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }
}
fn life() -> Arc<Life> {
    Arc::new(Life {
        stop: AtomicBool::new(false),
        socket: Mutex::new(None),
    })
}
/// Latest-state mailbox prevents a slow editor from accumulating snapshots.
pub struct EditorServer {
    pub endpoint: String,
    pub token: String,
    life: Arc<Life>,
    inbox: Arc<Mutex<Inbox>>,
    sender: mpsc::SyncSender<Request>,
    worker: Option<JoinHandle<()>>,
    sequence: u64,
}
impl EditorServer {
    pub fn listen() -> anyhow::Result<Self> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let endpoint = listener.local_addr()?.to_string();
        let mut random = [0; 32];
        rustls::crypto::ring::default_provider()
            .secure_random
            .fill(&mut random)
            .map_err(|_| anyhow::anyhow!("editor session random source failed"))?;
        let token = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let expected = token.clone();
        let alive = life();
        let worker_life = Arc::clone(&alive);
        let inbox = Arc::new(Mutex::new(Inbox::default()));
        let input = Arc::clone(&inbox);
        let (sender, outgoing) = mpsc::sync_channel(128);
        let worker = std::thread::Builder::new()
            .name("editor-player-link".into())
            .spawn(move || {
                let result = (|| -> anyhow::Result<()> {
                    let mut socket = loop {
                        if worker_life.stop.load(Ordering::Relaxed) {
                            return Ok(());
                        }
                        match listener.accept() {
                            Ok((mut stream, peer)) => {
                                // Winsock inherits the nonblocking listener mode on accepted sockets.
                                stream.set_nonblocking(false)?;
                                if !peer.ip().is_loopback() {
                                    continue;
                                }
                                stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                                let auth = read_frame::<Hello>(&mut stream);
                                if !auth
                                    .is_ok_and(|h| h.protocol == PROTOCOL && h.token == expected)
                                {
                                    continue;
                                }
                                break stream;
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                std::thread::sleep(Duration::from_millis(10))
                            }
                            Err(e) => return Err(e.into()),
                        }
                    };
                    socket.set_read_timeout(None)?;
                    socket.set_write_timeout(Some(Duration::from_secs(2)))?;
                    socket.set_nodelay(true)?;
                    *worker_life
                        .socket
                        .lock()
                        .map_err(|_| anyhow::anyhow!("link lock"))? = Some(socket.try_clone()?);
                    let mut writer = socket.try_clone()?;
                    let write_life = Arc::clone(&worker_life);
                    let writer_worker = std::thread::spawn(move || {
                        while !write_life.stop.load(Ordering::Relaxed) {
                            match outgoing.recv_timeout(Duration::from_millis(20)) {
                                Ok(request) => {
                                    if write_frame(&mut writer, &request).is_err() {
                                        break;
                                    }
                                }
                                Err(mpsc::RecvTimeoutError::Timeout) => {}
                                Err(_) => break,
                            }
                        }
                        let _ = writer.shutdown(Shutdown::Both);
                    });
                    while !worker_life.stop.load(Ordering::Relaxed) {
                        match read_frame::<Update>(&mut socket)? {
                            Update::Snapshot(world) => {
                                if let Ok(mut inbox) = input.lock() {
                                    inbox.snapshot = Some(*world);
                                }
                            }
                            Update::Reply(reply) => {
                                if let Ok(mut inbox) = input.lock() {
                                    if inbox.replies.len() >= 128 {
                                        inbox.replies.pop_front();
                                    }
                                    inbox.replies.push_back(reply);
                                }
                            }
                        }
                    }
                    worker_life.stop();
                    let _ = writer_worker.join();
                    Ok(())
                })();
                worker_life.stop();
                if let Err(error) = result
                    && let Ok(mut inbox) = input.lock()
                {
                    inbox.error = Some(format!("Player connection: {error:#}"));
                }
            })?;
        Ok(Self {
            endpoint,
            token,
            life: alive,
            inbox,
            sender,
            worker: Some(worker),
            sequence: 0,
        })
    }
    pub fn inspect(&self, active: bool) {
        let _ = self.sender.try_send(Request::Inspect(active));
    }
    pub fn edit(&mut self, batch: EditorBatch) -> anyhow::Result<u64> {
        self.sequence += 1;
        self.sender
            .try_send(Request::Edit {
                sequence: self.sequence,
                batch,
            })
            .context("player edit queue is full or disconnected")?;
        Ok(self.sequence)
    }
    pub fn take(&self) -> (Option<EditorSnapshot>, Vec<EditorReply>, Option<String>) {
        match self.inbox.lock() {
            Ok(mut inbox) => (
                inbox.snapshot.take(),
                inbox.replies.drain(..).collect(),
                inbox.error.take(),
            ),
            Err(_) => (None, vec![], Some("player mailbox unavailable".into())),
        }
    }
}
impl Drop for EditorServer {
    fn drop(&mut self) {
        self.life.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
pub struct EditorClient {
    life: Arc<Life>,
    snapshot: Arc<Mutex<Option<EditorSnapshot>>>,
    inspecting: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
    #[cfg(test)]
    last: Instant,
}
impl EditorClient {
    pub fn connect(
        commands: mpsc::Sender<RuntimeCommand>,
        shared: &Arc<super::bridge::Shared>,
    ) -> anyhow::Result<Option<Self>> {
        let Some(endpoint) = std::env::var_os("WHATEVER_EDITOR_ENDPOINT") else {
            return Ok(None);
        };
        let endpoint = endpoint
            .to_str()
            .context("editor endpoint")?
            .parse::<std::net::SocketAddr>()?;
        ensure!(
            endpoint.ip().is_loopback(),
            "editor endpoint must be loopback"
        );
        let token = std::env::var("WHATEVER_EDITOR_TOKEN")?;
        let client = Self::connect_to(endpoint, token, commands)?;
        *shared
            .editor_snapshot
            .lock()
            .map_err(|_| anyhow::anyhow!("editor mailbox lock"))? =
            Some(Arc::downgrade(&client.snapshot));
        Ok(Some(client))
    }
    fn connect_to(
        endpoint: std::net::SocketAddr,
        token: String,
        commands: mpsc::Sender<RuntimeCommand>,
    ) -> anyhow::Result<Self> {
        ensure!(
            endpoint.ip().is_loopback(),
            "editor endpoint must be loopback"
        );
        let mut stream = TcpStream::connect_timeout(&endpoint, Duration::from_secs(5))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        stream.set_nodelay(true)?;
        write_frame(
            &mut stream,
            &Hello {
                protocol: PROTOCOL,
                token,
            },
        )?;
        let alive = life();
        *alive
            .socket
            .lock()
            .map_err(|_| anyhow::anyhow!("link lock"))? = Some(stream.try_clone()?);
        let mut write_socket = stream.try_clone()?;
        let snapshot = Arc::new(Mutex::new(None::<EditorSnapshot>));
        let shared = Arc::clone(&snapshot);
        let reading = Arc::clone(&alive);
        let inspecting = Arc::new(AtomicBool::new(false));
        let inspection = Arc::clone(&inspecting);
        let (reply_sender, replies) = mpsc::sync_channel(128);
        let reader = std::thread::Builder::new()
            .name("player-editor-input".into())
            .spawn(move || {
                let result = (|| -> anyhow::Result<()> {
                    while !reading.stop.load(Ordering::Relaxed) {
                        match read_frame::<Request>(&mut stream)? {
                            Request::Inspect(value) => inspection.store(value, Ordering::Relaxed),
                            Request::Edit { sequence, batch } => {
                                commands.send(RuntimeCommand::Editor(EditorControl {
                                    sequence,
                                    batch,
                                    reply: reply_sender.clone(),
                                }))?
                            }
                        }
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    tracing::debug!("editor link closed: {error:#}");
                }
                reading.stop();
            })?;
        let writing = Arc::clone(&alive);
        let writer = std::thread::Builder::new()
            .name("player-editor-snapshots".into())
            .spawn(move || {
                let result = (|| -> anyhow::Result<()> {
                    while !writing.stop.load(Ordering::Relaxed) {
                        for reply in replies.try_iter() {
                            write_frame(&mut write_socket, &Update::Reply(reply))?;
                        }
                        let next = shared.lock().ok().and_then(|mut slot| slot.take());
                        if let Some(world) = next {
                            write_frame(&mut write_socket, &Update::Snapshot(Box::new(world)))?;
                        }
                        std::thread::sleep(Duration::from_millis(16));
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    tracing::debug!("editor snapshots stopped: {error:#}");
                }
                writing.stop();
            })?;
        Ok(Self {
            life: alive,
            snapshot,
            inspecting,
            reader: Some(reader),
            writer: Some(writer),
            #[cfg(test)]
            last: Instant::now() - Duration::from_secs(1),
        })
    }
    pub fn inspecting(&self) -> bool {
        self.inspecting.load(Ordering::Relaxed)
    }
    #[cfg(test)]
    fn publish(&mut self, world: &World, camera: Option<crate::ecs::EntityId>) {
        if self.last.elapsed() < Duration::from_millis(50) || self.life.stop.load(Ordering::Relaxed)
        {
            return;
        }
        self.last = Instant::now();
        if let Ok(mut slot) = self.snapshot.lock() {
            *slot = Some(EditorSnapshot {
                world: world.clone(),
                camera,
                sequence: 0,
            });
        }
    }
}
impl Drop for EditorClient {
    fn drop(&mut self) {
        self.life.stop();
        for worker in [&mut self.reader, &mut self.writer] {
            if let Some(worker) = worker.take() {
                let _ = worker.join();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::editor::{EntityEdit, apply_editor_batch};
    use super::*;
    use std::collections::BTreeMap;
    #[test]
    fn authenticated_session_exchanges_snapshots_inspection_and_tick_commands() {
        let mut server = EditorServer::listen().expect("listen");
        let address = server.endpoint.parse().expect("address");
        let mut wrong = TcpStream::connect(address).expect("connect");
        wrong
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("timeout");
        write_frame(
            &mut wrong,
            &Hello {
                protocol: PROTOCOL,
                token: "wrong session".into(),
            },
        )
        .expect("hello");
        let mut byte = [0];
        assert_eq!(wrong.read(&mut byte).expect("rejected peer"), 0);
        let (tx, rx) = mpsc::channel();
        let mut client =
            EditorClient::connect_to(address, server.token.clone(), tx).expect("client");
        let mut world = World::new();
        let stale = world.create_entity();
        world.destroy_entity(stale);
        let id = world.create_entity();
        assert_ne!(id.generation, stale.generation);
        world.names.insert(id.index, "Runtime cube".into());
        let physics_world = world.create_entity();
        world.set_component(&physics_world, "core:physics_world", serde_json::json!({}));
        world.set_component(
            &id,
            "core:transform",
            serde_json::json!({"position":[0,5,0],"rotation":[0,0,0,1],"scale":[1,1,1]}),
        );
        world.set_component(
            &id,
            "core:rigid_body",
            serde_json::json!({"type":"dynamic","world":{"$entity":physics_world.to_string()}}),
        );
        world.rigid_body_states.insert(
            id.index,
            crate::ecs::RigidBodyState {
                world: physics_world,
                position: [0.0, 5.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                linear_velocity: [0.0; 3],
                angular_velocity: [0.0; 3],
                sleeping: false,
                mass: 1.0,
                center_of_mass: [0.0; 3],
                tick_number: 1,
            },
        );
        client.publish(&world, Some(id));
        let deadline = Instant::now() + Duration::from_secs(3);
        let snapshot = loop {
            if let Some(snapshot) = server.take().0 {
                break snapshot;
            }
            assert!(Instant::now() < deadline, "snapshot timed out");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(snapshot.world.is_alive(&id));
        assert_eq!(snapshot.camera, Some(id));
        assert_eq!(
            snapshot.world.rigid_body_states[&id.index].world,
            physics_world
        );
        assert_eq!(snapshot.world.names[&id.index], "Runtime cube");
        server.inspect(true);
        while !client.inspecting() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let sequence = server
            .edit(EditorBatch {
                edits: vec![EntityEdit {
                    id: id.to_string(),
                    name: Some("Edited live".into()),
                    enabled: Some(false),
                    components: BTreeMap::new(),
                }],
                ..Default::default()
            })
            .expect("request");
        let RuntimeCommand::Editor(control) = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("tick command")
        else {
            panic!("expected editor control")
        };
        assert_eq!(control.sequence, sequence);
        let created = apply_editor_batch(&mut world, &control.batch).expect("apply");
        control
            .reply
            .send(EditorReply {
                sequence,
                created,
                error: None,
            })
            .expect("reply");
        let reply = loop {
            let (_, mut replies, _) = server.take();
            if let Some(reply) = replies.pop() {
                break reply;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        assert!(reply.error.is_none());
        assert!(!world.enabled(&id));
        assert_eq!(world.names[&id.index], "Edited live");
        drop(client);
        drop(server);
    }
    #[test]
    fn oversized_frames_are_rejected_before_allocating_the_payload() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).expect("listen");
        let mut writer =
            TcpStream::connect(listener.local_addr().expect("address")).expect("connect");
        let (mut reader, _) = listener.accept().expect("accept");
        writer
            .write_all(&((LIMIT + 1) as u32).to_be_bytes())
            .expect("header");
        assert!(read_frame::<Hello>(&mut reader).is_err());
    }
}
