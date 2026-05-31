use crate::logging::{SharedLogWriter, strip_ansi};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

pub struct DebugSwitches {
    window: AtomicBool,
    modloader: AtomicBool,
    ipc: AtomicBool,
    vfs: AtomicBool,
    audio: AtomicBool,
}

impl DebugSwitches {
    pub fn new(window: bool, modloader: bool, ipc: bool, vfs: bool, audio: bool) -> Self {
        DebugSwitches {
            window: AtomicBool::new(window),
            modloader: AtomicBool::new(modloader),
            ipc: AtomicBool::new(ipc),
            vfs: AtomicBool::new(vfs),
            audio: AtomicBool::new(audio),
        }
    }

    pub fn window(&self) -> bool {
        self.window.load(Relaxed)
    }
    pub fn modloader(&self) -> bool {
        self.modloader.load(Relaxed)
    }
    pub fn ipc(&self) -> bool {
        self.ipc.load(Relaxed)
    }
    pub fn vfs(&self) -> bool {
        self.vfs.load(Relaxed)
    }
    pub fn audio(&self) -> bool {
        self.audio.load(Relaxed)
    }

    pub fn set_window(&self, v: bool) {
        self.window.store(v, Relaxed)
    }
    pub fn set_modloader(&self, v: bool) {
        self.modloader.store(v, Relaxed)
    }
    pub fn set_ipc(&self, v: bool) {
        self.ipc.store(v, Relaxed)
    }
    pub fn set_vfs(&self, v: bool) {
        self.vfs.store(v, Relaxed)
    }
    pub fn set_audio(&self, v: bool) {
        self.audio.store(v, Relaxed)
    }

    pub fn toggle_window(&self) -> bool {
        !self.window.fetch_xor(true, Relaxed)
    }
    pub fn toggle_modloader(&self) -> bool {
        !self.modloader.fetch_xor(true, Relaxed)
    }
    pub fn toggle_ipc(&self) -> bool {
        !self.ipc.fetch_xor(true, Relaxed)
    }
    pub fn toggle_vfs(&self) -> bool {
        !self.vfs.fetch_xor(true, Relaxed)
    }
    pub fn toggle_audio(&self) -> bool {
        !self.audio.fetch_xor(true, Relaxed)
    }
}

pub type SharedDebugSwitches = Arc<DebugSwitches>;

pub struct DebugConfig {
    pub window: bool,
    pub modloader: bool,
    pub ipc: bool,
    pub vfs: bool,
    pub audio: bool,
}

impl DebugConfig {
    pub fn from_args(args: &[String]) -> Self {
        let mut cfg = DebugConfig {
            window: false,
            modloader: false,
            ipc: false,
            vfs: false,
            audio: false,
        };
        for arg in args {
            let val = if let Some(v) = arg.strip_prefix("--debug=") {
                v
            } else {
                continue;
            };
            for part in val.split(',') {
                match part.trim() {
                    "all" => {
                        cfg.window = true;
                        cfg.modloader = true;
                        cfg.ipc = true;
                        cfg.vfs = true;
                        cfg.audio = true;
                    }
                    "window" => cfg.window = true,
                    "modloader" => cfg.modloader = true,
                    "ipc" => cfg.ipc = true,
                    "vfs" => cfg.vfs = true,
                    "audio" => cfg.audio = true,
                    _ => {}
                }
            }
        }
        cfg
    }
}

pub struct DebugLogger {
    switches: SharedDebugSwitches,
    log_writer: SharedLogWriter,
    console_mirror: Arc<Mutex<Vec<String>>>,
}

impl DebugLogger {
    pub fn new(config: &DebugConfig, log_writer: SharedLogWriter) -> Self {
        DebugLogger {
            switches: Arc::new(DebugSwitches::new(
                config.window,
                config.modloader,
                config.ipc,
                config.vfs,
                config.audio,
            )),
            log_writer,
            console_mirror: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn shared_switches(&self) -> SharedDebugSwitches {
        Arc::clone(&self.switches)
    }

    pub fn console_mirror(&self) -> Arc<Mutex<Vec<String>>> {
        Arc::clone(&self.console_mirror)
    }

    pub fn log_writer(&self) -> SharedLogWriter {
        Arc::clone(&self.log_writer)
    }

    pub fn window(&self, msg: &str) {
        if self.switches.window() {
            self.write_debug_line("window", msg);
        }
    }

    pub fn modloader(&self, msg: &str) {
        if self.switches.modloader() {
            self.write_debug_line("modloader", msg);
        }
    }

    pub fn ipc(&self, mod_id: &str, direction: &str, msg: &str) {
        if self.switches.ipc() {
            self.write_debug_line("ipc", &format!("[{mod_id}] {direction} {msg}"));
        }
    }

    pub fn audio(&self, mod_id: &str, msg: &str) {
        if self.switches.audio() {
            self.write_debug_line("audio", &format!("[{mod_id}] {msg}"));
        }
    }

    fn write_debug_line(&self, category: &str, msg: &str) {
        let now = chrono::Local::now().format("%H:%M:%S%.3f");
        if let Ok(mut w) = self.log_writer.lock() {
            let _ = writeln!(w, "[{now}] [{category}] {}", strip_ansi(msg));
            let _ = w.flush();
        }
        Self::mirror(&self.console_mirror, category, msg);
    }

    fn mirror(m: &Arc<Mutex<Vec<String>>>, category: &str, msg: &str) {
        if let Ok(mut v) = m.lock() {
            let now = chrono::Local::now().format("%H:%M:%S%.3f");
            v.push(format!("[{now}] [{category}] {msg}"));
        }
    }
}
