use super::ipc::{EngineMessage, ScriptPacket};
use super::runtime::{find_host, host_command};
use crate::debug::DebugLogger;
use crate::sandbox::{SandboxConfig, SandboxGuard};
use anyhow::Context;
use std::collections::HashMap;
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub struct ScriptProcess {
    pub mod_id: String,
    child: Child,
    writer: super::writer::PipeWriter,
    ready: bool,
    _sandbox_guard: SandboxGuard,
}

impl ScriptProcess {
    pub fn send(&mut self, msg: &EngineMessage, debug: &mut DebugLogger) -> anyhow::Result<()> {
        let line = serde_json::to_string(msg)?;
        debug.ipc(&self.mod_id, "←", &line);
        self.writer.send(line)
    }
}

/// Outcome of a blocking receive on the shared script channel.
pub enum RecvOutcome {
    /// A valid message was decoded from the channel.
    Message(String, ScriptPacket),
    /// The receive timed out; try again later.
    Timeout,
    /// All stdout reader threads have exited — no more messages will ever
    /// arrive (all script processes have terminated).
    Disconnected,
}

pub struct PendingReply {
    pub sender_mod_id: String,
    /// The original numeric request_id from the sender's counter, sent back in ModMessageReplyDelivered.
    pub original_request_id: String,
}

pub struct ScriptHost {
    runtime: std::path::PathBuf,
    processes: HashMap<String, ScriptProcess>,
    /// Maps "<sender_mod_id>-<original_request_id>" → PendingReply.
    pending_replies: HashMap<String, PendingReply>,
    shared_rx: mpsc::Receiver<(String, String)>,
    shared_tx: mpsc::SyncSender<(String, String)>,
    responses: Option<Vec<(String, EngineMessage)>>,
}

impl ScriptHost {
    pub fn new() -> anyhow::Result<Self> {
        let (shared_tx, shared_rx) = mpsc::sync_channel(4096);
        Ok(ScriptHost {
            runtime: find_host()?,
            processes: HashMap::new(),
            pending_replies: HashMap::new(),
            shared_rx,
            shared_tx,
            responses: None,
        })
    }

    pub fn spawn(
        &mut self,
        mod_id: &str,
        entry: &Path,
        mode: crate::runtime::RuntimeMode,
        sandbox_cfg: &SandboxConfig,
        debug: &mut DebugLogger,
    ) -> anyhow::Result<()> {
        tracing::info!(mod_id, entry = %entry.display(), "spawning bundled mod host");

        if !entry.exists() {
            anyhow::bail!("script entry not found: {}", entry.display());
        }

        let entry = entry.canonicalize().context("resolve mod script entry")?;
        let mod_root = sandbox_cfg
            .mod_root
            .canonicalize()
            .context("resolve mod root")?;
        if !entry.starts_with(&mod_root) {
            anyhow::bail!("script entry escapes mod root: {}", entry.display());
        }
        let mut cmd = host_command(&self.runtime);
        cmd.arg(&entry)
            .arg(&mod_root)
            .arg(mode.to_string())
            .arg(mod_id)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        crate::sandbox::apply_pre_spawn(&mut cmd, sandbox_cfg)?;

        let mut child = cmd.spawn().with_context(|| {
            format!(
                "failed to spawn bundled mod host for mod '{mod_id}' (entry: {})",
                entry.display()
            )
        })?;

        let sandbox_guard = crate::sandbox::apply_post_spawn(&child, sandbox_cfg)?;

        let stdin = child.stdin.take().context("take stdin")?;
        let writer = match super::writer::PipeWriter::new(stdin, mod_id.to_owned()) {
            Ok(writer) => writer,
            Err(error) => {
                let _ = child.kill();
                return Err(error);
            }
        };
        let stdout = child.stdout.take().context("take stdout")?;
        let stderr = child.stderr.take().context("take stderr")?;

        // Stdout reader thread — posts (mod_id, line) to the shared channel.
        let tx = self.shared_tx.clone();
        let mod_id_out = mod_id.to_owned();
        thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(l) => {
                        if tx.send((mod_id_out.clone(), l)).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::debug!(mod_id = %mod_id_out, "stdout closed: {e}");
                        break;
                    }
                }
            }
            tracing::debug!(mod_id = %mod_id_out, "stdout reader thread exiting");
        });

        // Stderr reader thread — forwards Bun runtime errors as warnings.
        let mod_id_err = mod_id.to_owned();
        thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            for line in BufReader::new(stderr).lines() {
                match line {
                    Ok(l) => tracing::warn!(mod_id = %mod_id_err, "[script stderr] {l}"),
                    Err(_) => break,
                }
            }
        });

        tracing::info!(mod_id, "script process ready");
        debug.ipc(
            mod_id,
            "←",
            &format!("spawned bundled mod host: {}", entry.display()),
        );
        self.processes.insert(
            mod_id.to_owned(),
            ScriptProcess {
                mod_id: mod_id.to_owned(),
                child,
                writer,
                ready: false,
                _sandbox_guard: sandbox_guard,
            },
        );
        Ok(())
    }

    pub fn reap_exited(&mut self) -> Vec<String> {
        let exited: Vec<String> = self
            .processes
            .iter_mut()
            .filter_map(|(id, process)| match process.child.try_wait() {
                Ok(Some(status)) => {
                    tracing::info!(mod_id=%id,%status,"mod process exited");
                    Some(id.clone())
                }
                _ => None,
            })
            .collect();
        for id in &exited {
            self.processes.remove(id);
        }
        exited
    }

    pub fn runtime_path(&self) -> &Path {
        &self.runtime
    }

    /// Send a message to a script process.  Returns `true` if the write
    /// succeeded, `false` if the process is dead or the pipe is broken.
    pub fn send(&mut self, mod_id: &str, msg: &EngineMessage, debug: &mut DebugLogger) -> bool {
        if let Some(responses) = self.responses.as_mut() {
            if !self.processes.contains_key(mod_id) {
                return false;
            }
            responses.push((mod_id.to_owned(), msg.clone()));
            return true;
        }
        match self.processes.get_mut(mod_id) {
            None => false,
            Some(proc) => match proc.send(msg, debug) {
                Ok(()) => true,
                Err(e) => {
                    tracing::warn!(mod_id, "send error: {e}");
                    // Prevent unresolved RPCs after backpressure or a broken pipe.
                    let _ = proc.child.kill();
                    false
                }
            },
        }
    }

    /// Encode a committed scene delta once and share the immutable bytes across pipe writers.
    pub fn broadcast_scene(
        &mut self,
        delta: crate::ecs::SceneDelta,
        debug: &mut DebugLogger,
    ) -> anyhow::Result<usize> {
        let mut line = serde_json::to_string(&EngineMessage::SceneDelta { delta })?;
        line.push('\n');
        let bytes = line.len();
        let line: std::sync::Arc<str> = line.into();
        for (id, process) in &mut self.processes {
            if !process.ready {
                continue;
            }
            debug.ipc(id, "←", &line);
            if let Err(error) = process.writer.send_shared(std::sync::Arc::clone(&line)) {
                tracing::warn!(mod_id=%id, %error, "scene publication failed");
                let _ = process.child.kill();
            }
        }
        Ok(bytes)
    }

    /// Bound collection so a continuously writing process cannot prevent a tick.
    pub fn drain_messages(&mut self, debug: &mut DebugLogger) -> Vec<(String, ScriptPacket)> {
        let lines: Vec<_> = self.shared_rx.try_iter().take(4096).collect();
        lines
            .into_iter()
            .filter_map(|(id, line)| self.decode_packet(id, line, debug))
            .collect()
    }

    fn decode_packet(
        &mut self,
        mod_id: String,
        line: String,
        debug: &mut DebugLogger,
    ) -> Option<(String, ScriptPacket)> {
        debug.ipc(&mod_id, "→", &line);
        match serde_json::from_str::<ScriptPacket>(&line) {
            Ok(packet) => Some((mod_id, packet)),
            Err(error) => {
                tracing::warn!(mod_id, "bad IPC packet: {error}");
                self.terminate(&mod_id);
                None
            }
        }
    }

    /// Blocks until a message arrives or `timeout` elapses.
    /// Used by the tick wait loop — callers must handle `Disconnected` by
    /// abandoning the loop (no further messages will ever arrive).
    pub fn recv_blocking(&mut self, timeout: Duration, debug: &mut DebugLogger) -> RecvOutcome {
        match self.shared_rx.recv_timeout(timeout) {
            Ok((mod_id, line)) => self
                .decode_packet(mod_id, line, debug)
                .map_or(RecvOutcome::Timeout, |(id, packet)| {
                    RecvOutcome::Message(id, packet)
                }),
            Err(mpsc::RecvTimeoutError::Timeout) => RecvOutcome::Timeout,
            Err(mpsc::RecvTimeoutError::Disconnected) => RecvOutcome::Disconnected,
        }
    }

    pub fn mark_ready(&mut self, mod_id: &str) -> bool {
        let Some(process) = self.processes.get_mut(mod_id) else {
            return false;
        };
        if process.ready {
            return false;
        }
        process.ready = true;
        true
    }

    pub fn is_ready(&self, mod_id: &str) -> bool {
        self.processes
            .get(mod_id)
            .is_some_and(|process| process.ready)
    }

    pub fn terminate(&mut self, mod_id: &str) {
        if let Some(process) = self.processes.get_mut(mod_id) {
            let _ = process.child.kill();
        }
    }

    /// Hold command results until the entire command/physics phase has finished.
    pub fn begin_responses(&mut self) {
        self.responses = Some(Vec::new());
    }

    pub fn flush_responses(&mut self, tick_number: u64, debug: &mut DebugLogger) {
        let mut grouped: Vec<(String, Vec<EngineMessage>)> = Vec::new();
        for (owner, message) in self.responses.take().unwrap_or_default() {
            if let Some((_, messages)) = grouped.iter_mut().find(|(id, _)| id == &owner) {
                messages.push(message);
            } else {
                grouped.push((owner, vec![message]));
            }
        }
        for (owner, messages) in grouped {
            self.send(
                &owner,
                &EngineMessage::CommandResults {
                    tick_number,
                    messages,
                },
                debug,
            );
        }
    }

    pub fn has_process(&self, mod_id: &str) -> bool {
        self.processes.contains_key(mod_id)
    }

    pub fn add_pending_reply(
        &mut self,
        sender_mod_id: &str,
        original_request_id: &str,
        reply: PendingReply,
    ) {
        let key = format!("{sender_mod_id}-{original_request_id}");
        self.pending_replies.insert(key, reply);
    }

    pub fn take_pending_reply(&mut self, namespaced_key: &str) -> Option<PendingReply> {
        self.pending_replies.remove(namespaced_key)
    }

    /// Sends Shutdown to all scripts, waits briefly for final messages, then
    /// kills remaining processes and removes them from the process table so
    /// any subsequent `send` call is a guaranteed no-op.
    pub fn shutdown_all(
        &mut self,
        exit_code: i32,
        debug: &mut DebugLogger,
    ) -> Vec<(String, ScriptPacket)> {
        let msg = EngineMessage::Shutdown { exit_code };
        for proc in self.processes.values_mut() {
            let _ = proc.send(&msg, debug);
        }
        thread::sleep(Duration::from_millis(100));
        let final_messages = self.drain_messages(debug);
        for proc in self.processes.values_mut() {
            let _ = proc.child.kill();
            let _ = proc.child.wait();
        }
        // Remove all processes so subsequent send() calls are no-ops even if
        // the engine somehow tries to tick after shutdown.
        self.processes.clear();
        final_messages
    }
}

impl Drop for ScriptHost {
    fn drop(&mut self) {
        for process in self.processes.values_mut() {
            let _ = process.child.kill();
            let _ = process.child.wait();
        }
    }
}
