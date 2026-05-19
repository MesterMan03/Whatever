use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

pub struct DebugConfig {
    pub window: bool,
    pub modloader: bool,
    pub ipc: bool,
}

impl DebugConfig {
    pub fn from_args(args: &[String]) -> Self {
        let mut cfg = DebugConfig { window: false, modloader: false, ipc: false };
        for arg in args {
            let val = if let Some(v) = arg.strip_prefix("--debug=") {
                v
            } else {
                continue;
            };
            for part in val.split(',') {
                match part.trim() {
                    "all" => { cfg.window = true; cfg.modloader = true; cfg.ipc = true; }
                    "window" => cfg.window = true,
                    "modloader" => cfg.modloader = true,
                    "ipc" => cfg.ipc = true,
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
}

impl DebugLogger {
    pub fn new(config: &DebugConfig, cwd: &Path) -> anyhow::Result<Self> {
        let dir = cwd.join("debug");
        if config.window || config.modloader || config.ipc {
            fs::create_dir_all(&dir)?;
        }
        let open = |name: &str| -> anyhow::Result<BufWriter<File>> {
            Ok(BufWriter::new(File::create(dir.join(name))?))
        };
        Ok(DebugLogger {
            window: if config.window { Some(open("window.log")?) } else { None },
            modloader: if config.modloader { Some(open("modloader.log")?) } else { None },
            ipc: if config.ipc { Some(open("ipc.log")?) } else { None },
        })
    }

    pub fn window(&mut self, msg: &str) {
        Self::write_line(&mut self.window, msg);
    }

    pub fn modloader(&mut self, msg: &str) {
        Self::write_line(&mut self.modloader, msg);
    }

    pub fn ipc(&mut self, mod_id: &str, direction: &str, msg: &str) {
        let line = format!("[{mod_id}] {direction} {msg}");
        Self::write_line(&mut self.ipc, &line);
    }

    fn write_line(writer: &mut Option<BufWriter<File>>, msg: &str) {
        if let Some(w) = writer {
            let _ = writeln!(w, "{msg}");
            let _ = w.flush();
        }
    }
}