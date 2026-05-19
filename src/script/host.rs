use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process::{Child, ChildStdin, Stdio};
use anyhow::Context;
use crate::debug::DebugLogger;
use super::ipc::{EngineMessage, ScriptMessage};

pub struct ScriptProcess {
    pub mod_id: String,
    child: Child,
    stdin: ChildStdin,
}

impl ScriptProcess {
    pub fn send(&mut self, msg: &EngineMessage, debug: &mut DebugLogger) -> anyhow::Result<()> {
        let line = serde_json::to_string(msg)?;
        debug.ipc(&self.mod_id, "→", &line);
        writeln!(self.stdin, "{line}")?;
        Ok(())
    }
}

pub struct ScriptHost {
    processes: HashMap<String, ScriptProcess>,
}

impl ScriptHost {
    pub fn new() -> Self {
        ScriptHost { processes: HashMap::new() }
    }

    pub fn spawn(
        &mut self,
        mod_id: &str,
        entry: &Path,
        debug: &mut DebugLogger,
    ) -> anyhow::Result<()> {
        let mut child = std::process::Command::new("bun")
            .arg("run")
            .arg(entry)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("spawning bun for mod '{mod_id}'"))?;

        let stdin = child.stdin.take().context("take stdin")?;
        debug.ipc(mod_id, "→", "spawned bun process");

        self.processes.insert(mod_id.to_owned(), ScriptProcess {
            mod_id: mod_id.to_owned(),
            child,
            stdin,
        });
        Ok(())
    }

    pub fn send_all(&mut self, msg: &EngineMessage, debug: &mut DebugLogger) {
        for proc in self.processes.values_mut() {
            if let Err(e) = proc.send(msg, debug) {
                tracing::warn!(mod_id = %proc.mod_id, "send error: {e}");
            }
        }
    }

    pub fn drain_messages(&mut self, debug: &mut DebugLogger) -> Vec<(String, ScriptMessage)> {
        use std::io::{BufRead, BufReader};
        let mut out = Vec::new();
        for proc in self.processes.values_mut() {
            // non-blocking read: try_read won't exist; we use try from stdout if available
            // For the prototype we do a best-effort non-blocking read via BufReader
            // Real impl would use tokio async; this is synchronous skeleton
            let Some(stdout) = proc.child.stdout.as_mut() else { continue };
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            // peek — only read if data ready (we'll improve with tokio in engine.rs)
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => {}
                Ok(_) => {
                    let line = line.trim_end();
                    debug.ipc(&proc.mod_id, "←", line);
                    match serde_json::from_str::<ScriptMessage>(line) {
                        Ok(msg) => out.push((proc.mod_id.clone(), msg)),
                        Err(e) => tracing::warn!(mod_id = %proc.mod_id, "bad IPC message: {e}: {line}"),
                    }
                }
            }
        }
        out
    }

    pub fn shutdown_all(&mut self, debug: &mut DebugLogger) {
        let msg = EngineMessage::Shutdown;
        for proc in self.processes.values_mut() {
            let _ = proc.send(&msg, debug);
            let _ = proc.child.kill();
        }
    }
}