use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

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
    window: Option<BufWriter<File>>,
    modloader: Option<BufWriter<File>>,
    ipc: Option<BufWriter<File>>,
    // Arc<Mutex> because VFS methods take &self (Arc<dyn Vfs>) and need shared write access.
    vfs: Option<Arc<Mutex<BufWriter<File>>>>,
    console_mirror: Option<Arc<Mutex<Vec<String>>>>,
}

impl DebugLogger {
    pub fn new(config: &DebugConfig, cwd: &Path) -> anyhow::Result<Self> {
        let dir = cwd.join("debug");
        if config.window || config.modloader || config.ipc || config.vfs {
            fs::create_dir_all(&dir)?;
        }
        let open = |name: &str| -> anyhow::Result<BufWriter<File>> {
            Ok(BufWriter::new(File::create(dir.join(name))?))
        };
        Ok(DebugLogger {
            window: if config.window {
                Some(open("window.log")?)
            } else {
                None
            },
            modloader: if config.modloader {
                Some(open("modloader.log")?)
            } else {
                None
            },
            ipc: if config.ipc {
                Some(open("ipc.log")?)
            } else {
                None
            },
            vfs: if config.vfs {
                Some(Arc::new(Mutex::new(open("vfs.log")?)))
            } else {
                None
            },
            console_mirror: if config.window || config.modloader || config.ipc || config.vfs {
                Some(Arc::new(Mutex::new(Vec::new())))
            } else {
                None
            },
        })
    }

    /// Returns a cloneable handle to the console mirror buffer, if any debug category is enabled.
    /// Engine drains this each frame and forwards lines to the dev console.
    pub fn console_mirror(&self) -> Option<Arc<Mutex<Vec<String>>>> {
        self.console_mirror.clone()
    }

    pub fn window(&mut self, msg: &str) {
        Self::write_line(&mut self.window, msg);
        Self::push_to_mirror(&self.console_mirror, "window", msg);
    }

    pub fn modloader(&mut self, msg: &str) {
        Self::write_line(&mut self.modloader, msg);
        Self::push_to_mirror(&self.console_mirror, "modloader", msg);
    }

    pub fn ipc(&mut self, mod_id: &str, direction: &str, msg: &str) {
        let line = format!("[{mod_id}] {direction} {msg}");
        Self::write_line(&mut self.ipc, &line);
        Self::push_to_mirror(&self.console_mirror, "ipc", &line);
    }

    /// Returns a handle to the VFS log writer for `LayeredVfs` to use directly.
    pub fn vfs_writer(&self) -> Option<Arc<Mutex<BufWriter<File>>>> {
        self.vfs.clone()
    }

    /// Writes a timestamped line to a shared VFS log handle and optionally mirrors to the console.
    /// Called by `LayeredVfs`.
    pub fn write_vfs(
        writer: &Arc<Mutex<BufWriter<File>>>,
        mirror: Option<&Arc<Mutex<Vec<String>>>>,
        msg: &str,
    ) {
        Self::write_shared(writer, msg);
        Self::push_to_mirror(&mirror.cloned(), "vfs", msg);
    }

    fn push_to_mirror(mirror: &Option<Arc<Mutex<Vec<String>>>>, category: &str, msg: &str) {
        if let Some(m) = mirror &&
            let Ok(mut v) = m.lock() {
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

    fn write_shared(writer: &Mutex<BufWriter<File>>, msg: &str) {
        if let Ok(mut w) = writer.lock() {
            let now = chrono::Local::now().format("%H:%M:%S%.3f");
            let _ = writeln!(w, "[{now}] {msg}");
            let _ = w.flush();
        }
    }
}
