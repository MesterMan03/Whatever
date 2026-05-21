use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

pub struct DebugSwitches {
    window: AtomicBool,
    modloader: AtomicBool,
    ipc: AtomicBool,
    vfs: AtomicBool,
}

impl DebugSwitches {
    pub fn new(window: bool, modloader: bool, ipc: bool, vfs: bool) -> Self {
        DebugSwitches {
            window: AtomicBool::new(window),
            modloader: AtomicBool::new(modloader),
            ipc: AtomicBool::new(ipc),
            vfs: AtomicBool::new(vfs),
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

    /// Toggle and return the new value.
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
}

pub type SharedDebugSwitches = Arc<DebugSwitches>;

pub struct DebugConfig {
    pub window: bool,
    pub modloader: bool,
    pub ipc: bool,
    pub vfs: bool,
}

impl DebugConfig {
    pub fn from_args(args: &[String]) -> Self {
        let mut cfg = DebugConfig {
            window: false,
            modloader: false,
            ipc: false,
            vfs: false,
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
                    }
                    "window" => cfg.window = true,
                    "modloader" => cfg.modloader = true,
                    "ipc" => cfg.ipc = true,
                    "vfs" => cfg.vfs = true,
                    _ => {}
                }
            }
        }
        cfg
    }
}

pub struct DebugLogger {
    switches: SharedDebugSwitches,
    debug_dir: PathBuf,
    window: Option<BufWriter<File>>,
    modloader: Option<BufWriter<File>>,
    ipc: Option<BufWriter<File>>,
    vfs: Option<Arc<Mutex<BufWriter<File>>>>,
    console_mirror: Arc<Mutex<Vec<String>>>,
}

impl DebugLogger {
    pub fn new(config: &DebugConfig, cwd: &Path) -> anyhow::Result<Self> {
        let debug_dir = cwd.join("debug");
        let any = config.window || config.modloader || config.ipc || config.vfs;
        if any {
            fs::create_dir_all(&debug_dir)?;
        }
        let open = |name: &str| -> anyhow::Result<BufWriter<File>> {
            Ok(BufWriter::new(File::create(debug_dir.join(name))?))
        };
        let window = if config.window {
            Some(open("window.log")?)
        } else {
            None
        };
        let modloader = if config.modloader {
            Some(open("modloader.log")?)
        } else {
            None
        };
        let ipc = if config.ipc {
            Some(open("ipc.log")?)
        } else {
            None
        };
        let vfs = if config.vfs {
            Some(Arc::new(Mutex::new(open("vfs.log")?)))
        } else {
            None
        };
        Ok(DebugLogger {
            switches: Arc::new(DebugSwitches::new(
                config.window,
                config.modloader,
                config.ipc,
                config.vfs,
            )),
            debug_dir,
            window,
            modloader,
            ipc,
            vfs,
            console_mirror: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn shared_switches(&self) -> SharedDebugSwitches {
        Arc::clone(&self.switches)
    }

    pub fn console_mirror(&self) -> Arc<Mutex<Vec<String>>> {
        Arc::clone(&self.console_mirror)
    }

    pub fn vfs_writer(&self) -> Option<Arc<Mutex<BufWriter<File>>>> {
        self.vfs.clone()
    }

    pub fn window(&mut self, msg: &str) {
        if !self.switches.window() {
            return;
        }
        if self.window.is_none() {
            self.window = Self::try_open(&self.debug_dir, "window.log");
        }
        Self::write_line(&mut self.window, msg);
        Self::mirror(&self.console_mirror, "window", msg);
    }

    pub fn modloader(&mut self, msg: &str) {
        if !self.switches.modloader() {
            return;
        }
        if self.modloader.is_none() {
            self.modloader = Self::try_open(&self.debug_dir, "modloader.log");
        }
        Self::write_line(&mut self.modloader, msg);
        Self::mirror(&self.console_mirror, "modloader", msg);
    }

    pub fn ipc(&mut self, mod_id: &str, direction: &str, msg: &str) {
        if !self.switches.ipc() {
            return;
        }
        let line = format!("[{mod_id}] {direction} {msg}");
        if self.ipc.is_none() {
            self.ipc = Self::try_open(&self.debug_dir, "ipc.log");
        }
        Self::write_line(&mut self.ipc, &line);
        Self::mirror(&self.console_mirror, "ipc", &line);
    }

    fn try_open(debug_dir: &Path, name: &str) -> Option<BufWriter<File>> {
        if let Err(e) = fs::create_dir_all(debug_dir) {
            tracing::warn!("failed to create debug dir for live-enable: {e}");
            return None;
        }
        match File::create(debug_dir.join(name)) {
            Ok(f) => Some(BufWriter::new(f)),
            Err(e) => {
                tracing::warn!("failed to open {name}: {e}");
                None
            }
        }
    }

    fn mirror(m: &Arc<Mutex<Vec<String>>>, category: &str, msg: &str) {
        if let Ok(mut v) = m.lock() {
            let now = chrono::Local::now().format("%H:%M:%S%.3f");
            v.push(format!("[{now}] [{category}] {msg}"));
        }
    }

    fn write_line(writer: &mut Option<BufWriter<File>>, msg: &str) {
        if let Some(w) = writer {
            let now = chrono::Local::now().format("%H:%M:%S%.3f");
            let _ = writeln!(w, "[{now}] {msg}");
            let _ = w.flush();
        }
    }
}
