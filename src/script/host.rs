use super::ipc::{EngineMessage, ScriptMessage};
use crate::debug::DebugLogger;
use crate::sandbox::{SandboxConfig, SandboxGuard};
use anyhow::Context;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};
use std::sync::mpsc;
use std::thread;

pub struct ScriptProcess {
    pub mod_id: String,
    child: Child,
    stdin: ChildStdin,
    stdout_rx: mpsc::Receiver<String>,
    _sandbox_guard: SandboxGuard,
}

impl ScriptProcess {
    pub fn send(&mut self, msg: &EngineMessage, debug: &mut DebugLogger) -> anyhow::Result<()> {
        let line = serde_json::to_string(msg)?;
        debug.ipc(&self.mod_id, "←", &line);
        writeln!(self.stdin, "{line}")?;
        Ok(())
    }
}

pub struct PendingReply {
    pub sender_mod_id: String,
    /// The original numeric request_id from the sender's counter, sent back in ModMessageReplyDelivered.
    pub original_request_id: String,
}

pub struct ScriptHost {
    processes: HashMap<String, ScriptProcess>,
    /// Maps "<sender_mod_id>-<original_request_id>" → PendingReply.
    pending_replies: HashMap<String, PendingReply>,
}

impl ScriptHost {
    pub fn new() -> Self {
        ScriptHost {
            processes: HashMap::new(),
            pending_replies: HashMap::new(),
        }
    }

    pub fn spawn(
        &mut self,
        mod_id: &str,
        entry: &Path,
        sandbox_cfg: &SandboxConfig,
        debug: &mut DebugLogger,
    ) -> anyhow::Result<()> {
        tracing::info!(mod_id, entry = %entry.display(), "spawning script process");

        if !entry.exists() {
            anyhow::bail!("script entry not found: {}", entry.display());
        }

        let mut cmd = std::process::Command::new("bun");
        cmd.arg("run")
            .arg(entry)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        crate::sandbox::apply_pre_spawn(&mut cmd, sandbox_cfg)?;

        let mut child = cmd.spawn().with_context(|| {
            format!(
                "failed to spawn bun for mod '{mod_id}' (entry: {}). \
                 Is bun installed and in PATH?",
                entry.display()
            )
        })?;

        let sandbox_guard = crate::sandbox::apply_post_spawn(&child, sandbox_cfg)?;

        let stdin = child.stdin.take().context("take stdin")?;
        let stdout = child.stdout.take().context("take stdout")?;
        let stderr = child.stderr.take().context("take stderr")?;

        // Stdout reader thread — sends lines into a channel so drain_messages is non-blocking.
        let (tx, rx) = mpsc::channel::<String>();
        let mod_id_out = mod_id.to_owned();
        thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
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
        debug.ipc(mod_id, "←", &format!("spawned bun: {}", entry.display()));
        self.processes.insert(
            mod_id.to_owned(),
            ScriptProcess {
                mod_id: mod_id.to_owned(),
                child,
                stdin,
                stdout_rx: rx,
                _sandbox_guard: sandbox_guard,
            },
        );
        Ok(())
    }

    pub fn mod_ids(&self) -> impl Iterator<Item = &str> {
        self.processes.keys().map(String::as_str)
    }

    pub fn send(&mut self, mod_id: &str, msg: &EngineMessage, debug: &mut DebugLogger) {
        if let Some(proc) = self.processes.get_mut(mod_id)
            && let Err(e) = proc.send(msg, debug)
        {
            tracing::warn!(mod_id, "send error: {e}");
        }
    }

    pub fn drain_messages(&mut self, debug: &mut DebugLogger) -> Vec<(String, ScriptMessage)> {
        let mut out = Vec::new();
        for proc in self.processes.values_mut() {
            while let Ok(line) = proc.stdout_rx.try_recv() {
                debug.ipc(&proc.mod_id, "→", &line);
                match serde_json::from_str::<ScriptMessage>(&line) {
                    Ok(msg) => out.push((proc.mod_id.clone(), msg)),
                    Err(e) => tracing::warn!(mod_id = %proc.mod_id, "bad IPC message: {e}: {line}"),
                }
            }
        }
        out
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

    /// Sends Shutdown to all scripts, waits briefly for final messages (e.g. exit-handler logs),
    /// then kills any processes still running. Returns the final drained messages.
    pub fn shutdown_all(
        &mut self,
        exit_code: i32,
        debug: &mut DebugLogger,
    ) -> Vec<(String, ScriptMessage)> {
        let msg = EngineMessage::Shutdown { exit_code };
        for proc in self.processes.values_mut() {
            let _ = proc.send(&msg, debug);
        }
        // Give scripts a brief window to handle the exit event and write final messages.
        thread::sleep(std::time::Duration::from_millis(100));
        let final_messages = self.drain_messages(debug);
        for proc in self.processes.values_mut() {
            let _ = proc.child.kill();
        }
        final_messages
    }
}
