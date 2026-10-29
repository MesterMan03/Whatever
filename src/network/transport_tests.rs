use super::{NetworkRegistry, NetworkRequest, NetworkSettings};
use crate::runtime::RuntimeMode;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "whatever-network-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).expect("test directory");
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn specification() -> super::schema::Specification {
    serde_json::from_value(json!({"id":"fixture:wire","states":[{"name":"chat", "serverbound":[{"name":"message","fields":[{"name":"body","schema":{"kind":"string","max_length":65536}}]}],"clientbound":[{"name":"message","fields":[{"name":"body","schema":{"kind":"string","max_length":65536}}]}]}]})).expect("specification")
}
struct Node {
    registry: NetworkRegistry,
    id: String,
    side: RuntimeMode,
    events: Vec<Value>,
    sequence: usize,
    pause_connect: bool,
}
impl Node {
    fn new(settings: NetworkSettings, side: RuntimeMode) -> Self {
        let mut registry = NetworkRegistry::new(settings);
        let data = if side == RuntimeMode::Server {
            Some(serde_json::from_value(json!({"schema":{"kind":"struct","fields":[{"name":"tags","schema":{"kind":"array","element":{"kind":"string"}}}]},"initial":{"tags":[]}})).expect("client data"))
        } else {
            None
        };
        let info = registry
            .request(
                "fixture",
                side,
                NetworkRequest::Register {
                    specification: specification(),
                    side,
                    client_data: data,
                },
            )
            .expect("registration");
        Self {
            registry,
            id: info["registration_id"].as_str().expect("handle").into(),
            side,
            events: vec![],
            sequence: 0,
            pause_connect: false,
        }
    }
    fn operation(&mut self, mut request: Value) -> String {
        self.sequence += 1;
        let token = format!("test:{}", self.sequence);
        request["registration_id"] = json!(self.id);
        let request = serde_json::from_value(request).expect("request");
        assert!(
            self.registry
                .dispatch("fixture", self.side, request, &token)
                .is_none(),
            "operation must be deferred"
        );
        token
    }
    fn request(&mut self, mut request: Value) -> anyhow::Result<Value> {
        request["registration_id"] = json!(self.id);
        self.registry
            .request("fixture", self.side, serde_json::from_value(request)?)
    }
    fn ack(&mut self, event: &Value) {
        self.request(json!({"op":"EventDone","client_id":event["client_id"],"event_id":event["event_id"],"error":null})).expect("acknowledge native event");
    }
    fn pump(&mut self) {
        for (_, message) in self.registry.poll() {
            let event = serde_json::to_value(message).expect("event JSON");
            if event["event_id"].is_string()
                && !(self.pause_connect && event["event"] == "connected")
            {
                self.ack(&event);
            }
            self.events.push(event);
        }
    }
    fn response(&self, token: &str) -> Option<&Value> {
        self.events
            .iter()
            .find(|event| event["request_id"] == token)
    }
    fn connected(&self) -> Vec<&Value> {
        self.events
            .iter()
            .filter(|e| e["event"] == "connected")
            .collect()
    }
    fn packets(&self) -> Vec<&Value> {
        self.events
            .iter()
            .filter(|e| e["event"] == "packet")
            .collect()
    }
}
fn wait(nodes: &mut [Node], condition: impl Fn(&[Node]) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        for node in nodes.iter_mut() {
            node.pump();
        }
        if condition(nodes) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "network operation timed out: {:?}",
            nodes.iter().map(|n| &n.events).collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn settings(root: &Directory, name: &str) -> NetworkSettings {
    NetworkSettings {
        directory: root.0.join(name),
        ..Default::default()
    }
}
fn listen(nodes: &mut [Node], address: &str) -> String {
    let token = nodes[0].operation(json!({"op":"Listen","address":address,"max_clients":64}));
    wait(nodes, |nodes| nodes[0].response(&token).is_some());
    let response = nodes[0].response(&token).expect("listen response");
    assert!(response["error"].is_null(), "{response}");
    response["result"]["address"]
        .as_str()
        .expect("listen address")
        .into()
}
fn relisten(nodes: &mut [Node], address: &str) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let token = nodes[0].operation(json!({"op":"Listen","address":address}));
        wait(nodes, |nodes| nodes[0].response(&token).is_some());
        if nodes[0].response(&token).expect("response")["error"].is_null() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "old endpoint did not release its socket"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn real_quic_multi_client_streams_datagrams_and_connection_initialization() {
    let directory = Directory::new();
    let mut nodes = vec![Node::new(
        settings(&directory, "server"),
        RuntimeMode::Server,
    )];
    for i in 0..3 {
        nodes.push(Node::new(
            settings(&directory, &format!("client{i}")),
            RuntimeMode::Client,
        ));
    }
    nodes[0].pause_connect = true;
    let address = listen(&mut nodes, "127.0.0.1:0");
    let strict = nodes[1].operation(json!({"op":"Connect","address":address,"trust":"strict"}));
    wait(&mut nodes, |nodes| nodes[1].response(&strict).is_some());
    assert!(
        nodes[1].response(&strict).expect("strict response")["error"].is_string(),
        "self-signed certificate must fail strict validation"
    );
    assert!(nodes[0].connected().is_empty());
    for i in 1..=3 {
        let token = nodes[i].operation(json!({"op":"Connect","address":address,"trust":"tofu"}));
        wait(&mut nodes, |nodes| {
            nodes[i].response(&token).is_some() && nodes[0].connected().len() == i
        });
        assert!(nodes[i].response(&token).expect("connect response")["error"].is_null());
    }
    nodes[1].request(json!({"op":"Send","client_id":null,"packet":"chat.message","data":{"body":"wait for initialization"}})).expect("queue message");
    let until = Instant::now() + Duration::from_millis(100);
    while Instant::now() < until {
        for node in &mut nodes {
            node.pump();
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        nodes[0].packets().is_empty(),
        "packets must wait for onConnect acknowledgement"
    );
    let connected: Vec<_> = nodes[0].connected().into_iter().cloned().collect();
    for event in &connected {
        assert_eq!(event["data"], json!({"tags":[]}));
        nodes[0].ack(event);
    }
    wait(&mut nodes, |nodes| nodes[0].packets().len() == 1);
    let long = "fragmented stream payload ".repeat(1600);
    nodes[1]
        .request(
            json!({"op":"Send","packet":"chat.message","data":{"body":long},"delivery":"stream"}),
        )
        .expect("large reliable frame");
    nodes[1].request(json!({"op":"Send","packet":"chat.message","data":{"body":"unreliable"},"delivery":"datagram"})).expect("datagram");
    wait(&mut nodes, |nodes| {
        nodes[0].packets().iter().any(|p| p["data"]["body"] == long)
            && nodes[0]
                .packets()
                .iter()
                .any(|p| p["transport"] == "datagram")
    });
    assert!(nodes[1].request(json!({"op":"Send","packet":"chat.message","data":{"body":long},"delivery":"datagram"})).is_err(),"oversized datagrams must fail without fragmentation");
    let target = connected[1]["client_id"].clone();
    nodes[0].request(json!({"op":"Send","client_id":target,"packet":"chat.message","data":{"body":"private message"}})).expect("server routing");
    wait(&mut nodes, |nodes| nodes[2].packets().len() == 1);
    assert_eq!(nodes[2].packets()[0]["data"]["body"], "private message");
    assert!(nodes[1].packets().is_empty() && nodes[3].packets().is_empty());
    nodes[0]
        .request(json!({"op":"Broadcast","packet":"chat.message","data":{"body":"everyone"}}))
        .expect("broadcast");
    wait(&mut nodes, |nodes| {
        nodes[1..].iter().all(|node| {
            node.packets()
                .iter()
                .any(|p| p["data"]["body"] == "everyone")
        })
    });
    nodes[2]
        .request(json!({"op":"Disconnect"}))
        .expect("disconnect");
    wait(&mut nodes, |nodes| {
        nodes[0].events.iter().any(|e| e["event"] == "disconnected")
    });
    assert!(nodes[0].request(json!({"op":"Send","client_id":target,"packet":"chat.message","data":{"body":"stale"}})).is_err());
    let reconnect = nodes[2].operation(json!({"op":"Connect","address":address,"trust":"tofu"}));
    wait(&mut nodes, |nodes| {
        nodes[2].response(&reconnect).is_some() && nodes[0].connected().len() == 4
    });
    assert_ne!(nodes[0].connected()[3]["client_id"], target);
    assert_eq!(nodes[0].connected()[3]["data"], json!({"tags":[]}));
}

#[test]
fn tofu_persists_and_rejects_server_key_changes() {
    let directory = Directory::new();
    let mut nodes = vec![
        Node::new(settings(&directory, "server"), RuntimeMode::Server),
        Node::new(settings(&directory, "client"), RuntimeMode::Client),
    ];
    let address = listen(&mut nodes, "127.0.0.1:0");
    let token = nodes[1].operation(json!({"op":"Connect","address":address,"trust":"tofu"}));
    wait(&mut nodes, |nodes| nodes[1].response(&token).is_some());
    assert!(nodes[1].response(&token).expect("response")["error"].is_null());
    nodes[0].request(json!({"op":"Stop"})).expect("stop");
    wait(&mut nodes, |nodes| {
        nodes[1].events.iter().any(|e| e["event"] == "disconnected")
    });
    std::fs::remove_file(directory.0.join("server/server-identity.json"))
        .expect("rotate test identity");
    // Let QUIC's old endpoint release its UDP socket before rebinding the same address.
    relisten(&mut nodes, &address);
    // A new client runtime must enforce the record written by the old runtime.
    nodes[1] = Node::new(settings(&directory, "client"), RuntimeMode::Client);
    let token = nodes[1].operation(json!({"op":"Connect","address":address,"trust":"tofu"}));
    wait(&mut nodes, |nodes| nodes[1].response(&token).is_some());
    let error = nodes[1].response(&token).expect("response")["error"]
        .as_str()
        .expect("changed identity rejected");
    assert!(error.contains("identity changed"), "{error}");
}

#[test]
fn strict_checks_names_and_trusted_certificates_and_tofu_never_downgrades_pki() {
    let directory = Directory::new();
    let cert =
        rcgen::generate_simple_self_signed(vec!["localhost".into()]).expect("test certificate");
    let certificate = directory.0.join("server.pem");
    let key = directory.0.join("server.key");
    let authority = directory.0.join("authority.pem");
    std::fs::write(&certificate, cert.cert.pem()).expect("certificate");
    std::fs::write(&key, cert.signing_key.serialize_pem()).expect("key");
    std::fs::write(&authority, cert.cert.pem()).expect("trusted test authority");
    let mut server_settings = settings(&directory, "server");
    server_settings.certificate = Some(certificate.clone());
    server_settings.private_key = Some(key.clone());
    let mut client_settings = settings(&directory, "client");
    client_settings.authority = Some(authority);
    let mut nodes = vec![
        Node::new(server_settings, RuntimeMode::Server),
        Node::new(client_settings, RuntimeMode::Client),
    ];
    let address = listen(&mut nodes, "127.0.0.1:0");
    let token = nodes[1].operation(json!({"op":"Connect","address":address,"trust":"strict"}));
    wait(&mut nodes, |nodes| nodes[1].response(&token).is_some());
    assert!(
        nodes[1].response(&token).expect("response")["error"].is_string(),
        "a trusted certificate for the wrong name must still fail"
    );
    let hostname = format!("localhost:{}", address.rsplit(':').next().expect("port"));
    let token = nodes[1].operation(json!({"op":"Connect","address":hostname,"trust":"strict"}));
    wait(&mut nodes, |nodes| nodes[1].response(&token).is_some());
    assert!(
        nodes[1].response(&token).expect("response")["error"].is_null(),
        "strict connection with a valid trusted certificate: {:?}",
        nodes[1].response(&token)
    );
    nodes[1]
        .request(json!({"op":"Disconnect"}))
        .expect("disconnect");
    let token = nodes[1].operation(json!({"op":"Connect","address":hostname,"trust":"tofu"}));
    wait(&mut nodes, |nodes| nodes[1].response(&token).is_some());
    assert!(nodes[1].response(&token).expect("response")["error"].is_null());
    nodes[0].request(json!({"op":"Stop"})).expect("stop");
    wait(&mut nodes, |nodes| {
        nodes[1]
            .events
            .iter()
            .filter(|e| e["event"] == "disconnected")
            .count()
            == 2
    });
    let replacement =
        rcgen::generate_simple_self_signed(vec!["localhost".into()]).expect("replacement identity");
    std::fs::write(certificate, replacement.cert.pem()).expect("new certificate");
    std::fs::write(key, replacement.signing_key.serialize_pem()).expect("new key");
    relisten(&mut nodes, &address);
    let token = nodes[1].operation(json!({"op":"Connect","address":hostname,"trust":"tofu"}));
    wait(&mut nodes, |nodes| nodes[1].response(&token).is_some());
    assert!(
        nodes[1].response(&token).expect("response")["error"].is_string(),
        "a known PKI server must never fall back to a new TOFU key"
    );
}

#[test]
fn schema_mismatch_is_rejected_before_mod_events_and_does_not_pin_an_identity() {
    let directory = Directory::new();
    let mut nodes = vec![
        Node::new(settings(&directory, "server"), RuntimeMode::Server),
        Node::new(settings(&directory, "client"), RuntimeMode::Client),
    ];
    let mut changed = specification();
    changed.states[0].serverbound[0].name = "different".into();
    let id = nodes[1].id.clone();
    nodes[1]
        .registry
        .registrations
        .get_mut(&id)
        .expect("registration")
        .compiled = std::sync::Arc::new(changed.compile("fixture").expect("changed schema"));
    let address = listen(&mut nodes, "127.0.0.1:0");
    let token = nodes[1].operation(json!({"op":"Connect","address":address,"trust":"tofu"}));
    wait(&mut nodes, |nodes| nodes[1].response(&token).is_some());
    assert!(nodes[1].response(&token).expect("response")["error"].is_string());
    assert!(nodes[0].connected().is_empty() && nodes[1].connected().is_empty());
    assert!(
        !directory.0.join("client/trusted-servers").exists(),
        "a rejected application handshake must not establish trust"
    );
}

#[test]
fn one_listener_accepts_many_concurrent_clients() {
    let directory = Directory::new();
    let mut nodes = vec![Node::new(
        settings(&directory, "server"),
        RuntimeMode::Server,
    )];
    let address = listen(&mut nodes, "127.0.0.1:0");
    let mut tokens = Vec::new();
    for i in 0..16 {
        let mut client = Node::new(
            settings(&directory, &format!("client{i}")),
            RuntimeMode::Client,
        );
        tokens.push(client.operation(json!({"op":"Connect","address":address,"trust":"tofu"})));
        nodes.push(client);
    }
    wait(&mut nodes, |nodes| {
        nodes[0].connected().len() == 16
            && tokens
                .iter()
                .enumerate()
                .all(|(i, token)| nodes[i + 1].response(token).is_some())
    });
    for (i, token) in tokens.iter().enumerate() {
        assert!(nodes[i + 1].response(token).expect("response")["error"].is_null());
    }
    let ids: std::collections::HashSet<_> = nodes[0]
        .connected()
        .iter()
        .map(|e| e["client_id"].as_str().expect("client ID"))
        .collect();
    assert_eq!(ids.len(), 16);
    nodes[0]
        .request(
            json!({"op":"Broadcast","packet":"chat.message","data":{"body":"sixteen clients"}}),
        )
        .expect("broadcast");
    wait(&mut nodes, |nodes| {
        nodes[1..].iter().all(|node| {
            node.packets()
                .iter()
                .any(|p| p["data"]["body"] == "sixteen clients")
        })
    });
}

#[test]
fn listener_can_stop_and_immediately_reopen_the_same_port() {
    let directory = Directory::new();
    let mut nodes = vec![
        Node::new(settings(&directory, "server"), RuntimeMode::Server),
        Node::new(settings(&directory, "client"), RuntimeMode::Client),
    ];
    let address = listen(&mut nodes, "127.0.0.1:0");
    for _ in 0..3 {
        let token = nodes[1].operation(json!({"op":"Connect","address":address,"trust":"tofu"}));
        wait(&mut nodes, |nodes| nodes[1].response(&token).is_some());
        assert!(nodes[1].response(&token).expect("response")["error"].is_null());
        nodes[0].request(json!({"op":"Stop"})).expect("stop");
        // No retries or delay: transport setup must finish retiring the old endpoint first.
        assert_eq!(listen(&mut nodes, &address), address);
        wait(&mut nodes, |nodes| {
            nodes[1]
                .registry
                .registrations
                .values()
                .all(|r| r.token.is_none())
        });
    }
}
