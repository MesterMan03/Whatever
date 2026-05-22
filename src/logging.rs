use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Context;
use flate2::Compression;
use flate2::write::GzEncoder;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Shared buffer fed by the tracing layer and drained each frame into the dev console.
/// Each entry is (level_str, formatted_message) e.g. ("WARN", "some message").
pub type LogMirror = Arc<Mutex<Vec<(String, String)>>>;

/// Shared handle to the log file; passed to subsystems that write debug lines directly.
pub type SharedLogWriter = Arc<Mutex<BufWriter<File>>>;

// ── Field visitor ──────────────────────────────────────────────────────────────

#[derive(Default)]
struct FieldCollector {
    message: String,
    extras: String,
}

impl FieldCollector {
    fn format(&self) -> String {
        if self.extras.is_empty() {
            self.message.clone()
        } else {
            format!("{} {}", self.extras, self.message)
        }
    }
}

impl tracing::field::Visit for FieldCollector {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message.push_str(value);
        } else {
            if !self.extras.is_empty() {
                self.extras.push(' ');
            }
            self.extras.push_str(&format!("{}={value}", field.name()));
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message.push_str(&format!("{value:?}"));
        } else {
            if !self.extras.is_empty() {
                self.extras.push(' ');
            }
            self.extras.push_str(&format!("{}={value:?}", field.name()));
        }
    }
}

/// Remove ANSI CSI escape sequences (e.g. `\x1b[31m`) from `s`.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['
            // consume parameter/intermediate bytes until the final byte (ASCII letter)
            for nc in chars.by_ref() {
                if nc.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn level_label(level: &tracing::Level) -> &'static str {
    match *level {
        tracing::Level::ERROR => "ERROR",
        tracing::Level::WARN => "WARN",
        tracing::Level::INFO => "INFO",
        tracing::Level::DEBUG => "DEBUG",
        tracing::Level::TRACE => "TRACE",
    }
}

// ── File layer ─────────────────────────────────────────────────────────────────

struct FileLogLayer {
    writer: SharedLogWriter,
}

impl<S: tracing::Subscriber> Layer<S> for FileLogLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let label = level_label(event.metadata().level());
        let mut collector = FieldCollector::default();
        event.record(&mut collector);
        let now = chrono::Local::now().format("%H:%M:%S%.3f");
        if let Ok(mut w) = self.writer.lock() {
            let _ = writeln!(w, "[{now}] [{label}] {}", strip_ansi(&collector.format()));
            let _ = w.flush();
        }
    }
}

// ── Console mirror layer ───────────────────────────────────────────────────────

struct ConsoleMirrorLayer {
    mirror: LogMirror,
}

impl<S: tracing::Subscriber> Layer<S> for ConsoleMirrorLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let level = event.metadata().level();
        // Only forward INFO and above to the in-game console.
        if *level > tracing::Level::INFO {
            return;
        }
        // Don't echo events that originated inside the console back to it.
        if event.metadata().target().starts_with("Whatever::console") {
            return;
        }
        let label = level_label(level).to_owned();
        let mut collector = FieldCollector::default();
        event.record(&mut collector);
        if let Ok(mut v) = self.mirror.lock() {
            v.push((label, collector.format()));
        }
    }
}

// ── Log rotation ───────────────────────────────────────────────────────────────

fn rotate_log(logs_dir: &Path) -> anyhow::Result<()> {
    let latest = logs_dir.join("latest.log");
    if !latest.exists() {
        return Ok(());
    }

    let meta = fs::metadata(&latest).context("stat latest.log")?;
    let file_time = meta
        .created()
        .or_else(|_| meta.modified())
        .context("get file time")?;
    let dt = chrono::DateTime::<chrono::Local>::from(file_time);
    let stamp = dt.format("%Y-%m-%d_%H-%M-%S");
    let archive_path = logs_dir.join(format!("{stamp}.log.gz"));

    let mut buf = Vec::new();
    File::open(&latest)
        .context("open latest.log")?
        .read_to_end(&mut buf)
        .context("read latest.log")?;

    let out = File::create(&archive_path).context("create archive")?;
    let mut gz = GzEncoder::new(out, Compression::default());
    gz.write_all(&buf).context("compress log")?;
    gz.finish().context("finish gz")?;

    fs::remove_file(&latest).context("remove latest.log")?;
    Ok(())
}

// ── Public init ────────────────────────────────────────────────────────────────

/// Initialises the global tracing subscriber and opens `logs/latest.log`.
///
/// Returns the shared log writer (for debug-category messages written outside
/// tracing) and the console mirror (drained each frame into the dev console).
///
/// Only events from this crate are logged by default; set `RUST_LOG` to override.
pub fn init(cwd: &Path, mirror: LogMirror) -> anyhow::Result<SharedLogWriter> {
    let logs_dir = cwd.join("logs");
    fs::create_dir_all(&logs_dir).context("create logs dir")?;

    if let Err(e) = rotate_log(&logs_dir) {
        eprintln!("warn: log rotation failed: {e}");
    }

    let log_file = File::create(logs_dir.join("latest.log")).context("create latest.log")?;
    let shared_writer: SharedLogWriter = Arc::new(Mutex::new(BufWriter::new(log_file)));

    let file_layer = FileLogLayer {
        writer: Arc::clone(&shared_writer),
    };
    let mirror_layer = ConsoleMirrorLayer { mirror };

    let stdout_layer = tracing_subscriber::fmt::layer()
        .with_ansi(true)
        .with_target(false)
        .compact();

    // Default: only our own crate at INFO+. Users can override with RUST_LOG.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("Whatever=info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(stdout_layer)
        .with(file_layer)
        .with(mirror_layer)
        .init();

    Ok(shared_writer)
}
