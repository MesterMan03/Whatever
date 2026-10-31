use super::*;
use crate::{debug::DebugConfig, runtime::LaunchOptions};
use std::{collections::HashMap, path::PathBuf, sync::Mutex};

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Node {
    simulation: Simulation,
    commands: mpsc::Sender<RuntimeCommand>,
    responses: HashMap<String, (Vec<String>, Option<String>)>,
    registered: bool,
    sequence: usize,
    trace: Vec<String>,
}
impl Node {
    fn new(directory: &Directory, name: &str, mode: RuntimeMode) -> Self {
        let root = directory.0.join(name);
        std::fs::create_dir_all(&root).expect("node directory");
        let log = std::fs::File::create(root.join("node.log")).expect("log");
        let game = LoadedGame::load(
            &LaunchOptions {
                project: None,
                mode: Some(mode),
                mods: Some(crate::development_root().join("mods")),
                network: crate::network::NetworkSettings {
                    directory: root.join("network"),
                    ..Default::default()
                },
            },
            &DebugConfig::from_args(&[]),
            &root,
            Arc::new(Mutex::new(std::io::BufWriter::new(log))),
        )
        .expect("load shipped mods");
        let shared = Arc::new(Shared {
            input: Default::default(),
            presentation: Default::default(),
            stop: std::sync::atomic::AtomicBool::new(false),
        });
        let (commands, receiver) = mpsc::channel();
        let mut simulation =
            Simulation::with_audio(game, shared, receiver, AudioManager::headless())
                .expect("simulation without GPU or audio devices");
        simulation.ready = true;
        Self {
            simulation,
            commands,
            responses: HashMap::new(),
            registered: false,
            sequence: 0,
            trace: vec![],
        }
    }
    fn pump(&mut self) {
        self.simulation.controls();
        let batches = self.simulation.tick_commands();
        for (owner, commands) in &batches {
            for message in commands {
                if self.trace.len() < 30 {
                    self.trace.push(format!("{owner}: {message:?}"));
                }
                if owner == "network_example" {
                    match &message {
                        ScriptMessage::RegisterCommand { name, .. } if name == "netdemo" => {
                            self.registered = true
                        }
                        ScriptMessage::CommandResponse {
                            request_id,
                            output,
                            error,
                        } => {
                            self.responses
                                .insert(request_id.clone(), (output.clone(), error.clone()));
                        }
                        ScriptMessage::Log { level, message } if level == "error" => {
                            panic!("network example error: {message}")
                        }
                        _ => {}
                    }
                }
            }
        }
        self.simulation.finish_tick(batches);
        if let Ok(mut slot) = self.simulation.shared.presentation.lock() {
            slot.take();
        }
    }
    fn invoke(&mut self, name: &str, args: Vec<serde_json::Value>) -> String {
        self.sequence += 1;
        let request_id = format!("demo-command:{}", self.sequence);
        self.commands
            .send(RuntimeCommand::Ipc {
                mod_id: "network_example".into(),
                message: EngineMessage::CommandInvoke {
                    request_id: request_id.clone(),
                    command_path: vec!["netdemo".into(), name.into()],
                    args,
                },
            })
            .expect("invoke command");
        request_id
    }
}
impl Drop for Node {
    fn drop(&mut self) {
        self.simulation.network.shutdown();
        self.simulation
            .script_host
            .shutdown_all(0, &mut self.simulation.debug);
    }
}
fn wait(nodes: &mut [Node], predicate: impl Fn(&[Node]) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        for node in nodes.iter_mut() {
            node.pump();
        }
        if predicate(nodes) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "mod demo timed out: {:?}; traces={:?}",
            nodes.iter().map(|n| &n.responses).collect::<Vec<_>>(),
            nodes.iter().map(|n| &n.trace).collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn command(
    nodes: &mut [Node],
    index: usize,
    name: &str,
    args: Vec<serde_json::Value>,
) -> (Vec<String>, Option<String>) {
    let token = nodes[index].invoke(name, args);
    wait(nodes, |nodes| nodes[index].responses.contains_key(&token));
    nodes[index]
        .responses
        .remove(&token)
        .expect("command response")
}
fn successful(
    nodes: &mut [Node],
    index: usize,
    name: &str,
    args: Vec<serde_json::Value>,
) -> Vec<String> {
    let (output, error) = command(nodes, index, name, args);
    assert!(error.is_none(), "{name}: {error:?}");
    output
}
fn await_status(nodes: &mut [Node], index: usize, expected: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let status = successful(nodes, index, "status", vec![]).join("\n");
        if status.contains(expected) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "expected {expected}; got {status}"
        );
    }
}

#[test]
#[ignore = "requires the bundled mod host; run in mod-runtime CI"]
fn network_messenger_runs_with_three_real_mod_clients() {
    use serde_json::json;
    let directory = Directory(std::env::temp_dir().join(format!(
            "whatever-mod-network-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        )));
    let mut nodes = vec![Node::new(&directory, "server", RuntimeMode::Server)];
    for i in 1..=3 {
        nodes.push(Node::new(
            &directory,
            &format!("client{i}"),
            RuntimeMode::Client,
        ));
    }
    wait(&mut nodes, |nodes| nodes.iter().all(|node| node.registered));
    let output = successful(&mut nodes, 0, "host", vec![json!(0), json!(64)]).join("\n");
    let port = output.rsplit(':').next().expect("bound port");
    let address = format!("127.0.0.1:{port}");
    let (_, error) = command(
        &mut nodes,
        1,
        "connect",
        vec![json!(address), json!("strict")],
    );
    assert!(
        error.is_some(),
        "strict validation must reject a self-signed development server"
    );
    for i in 1..=3 {
        successful(&mut nodes, i, "connect", vec![json!(address)]);
        await_status(&mut nodes, i, &format!("player={i};"));
    }
    for i in 1..=3 {
        await_status(&mut nodes, i, "players=1,2,3;");
    }
    successful(
        &mut nodes,
        1,
        "tell",
        vec![json!(2), json!("hello from one to two")],
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let inbox = successful(&mut nodes, 2, "inbox", vec![]).join("\n");
        if inbox.contains("[#1 -> #2] hello from one to two") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "private message missing: {inbox}"
        );
    }
    assert_eq!(
        successful(&mut nodes, 3, "inbox", vec![]),
        vec!["No messages yet."]
    );
    successful(&mut nodes, 2, "say", vec![json!("hello everyone")]);
    for i in 1..=3 {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if successful(&mut nodes, i, "inbox", vec![])
                .join("\n")
                .contains("[#2 -> everyone] hello everyone")
            {
                break;
            }
            assert!(Instant::now() < deadline, "broadcast missing on client {i}");
        }
    }
    successful(
        &mut nodes,
        1,
        "tell",
        vec![json!(9999), json!("missing recipient")],
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if successful(&mut nodes, 1, "inbox", vec![])
            .join("\n")
            .contains("Player #9999 is not connected")
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "missing recipient did not produce an error packet"
        );
    }
    successful(&mut nodes, 1, "ping", vec![]);
    await_status(&mut nodes, 1, "pong=1 (datagram)");
    successful(&mut nodes, 2, "disconnect", vec![]);
    await_status(&mut nodes, 2, "Disconnected; player=none;");
    successful(&mut nodes, 2, "connect", vec![json!(address)]);
    await_status(&mut nodes, 2, "player=4;");
    for i in 1..=3 {
        await_status(&mut nodes, i, "players=1,3,4;");
    }
    successful(&mut nodes, 3, "tell", vec![json!(4), json!("welcome back")]);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if successful(&mut nodes, 2, "inbox", vec![])
            .join("\n")
            .contains("[#3 -> #4] welcome back")
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "routing to reconnected player failed"
        );
    }
}
