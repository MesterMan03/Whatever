use anyhow::{Context, Result};
use std::{
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
};

struct Line {
    text: String,
    queued: Arc<AtomicUsize>,
}
impl Drop for Line {
    fn drop(&mut self) {
        self.queued.fetch_sub(self.text.len(), Ordering::Release);
    }
}

/// A stopped or slow mod must never block the simulation on an OS pipe write.
pub struct PipeWriter {
    sender: mpsc::SyncSender<Line>,
    queued: Arc<AtomicUsize>,
    max_bytes: usize,
}
impl PipeWriter {
    pub fn new(writer: impl Write + Send + 'static, mod_id: String) -> Result<Self> {
        Self::with_limits(writer, mod_id, 256, 64 * 1024 * 1024)
    }
    fn with_limits(
        mut writer: impl Write + Send + 'static,
        mod_id: String,
        capacity: usize,
        max_bytes: usize,
    ) -> Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Line>(capacity);
        thread::Builder::new()
            .name(format!("mod-{mod_id}-stdin"))
            .spawn(move || {
                while let Ok(line) = receiver.recv() {
                    if let Err(error) = writer.write_all(line.text.as_bytes()) {
                        tracing::debug!(mod_id, "mod stdin closed: {error}");
                        break;
                    }
                }
            })?;
        Ok(Self {
            sender,
            queued: Arc::new(AtomicUsize::new(0)),
            max_bytes,
        })
    }
    pub fn send(&self, mut text: String) -> Result<()> {
        text.push('\n');
        let mut queued = self.queued.load(Ordering::Acquire);
        loop {
            let reserved = queued
                .checked_add(text.len())
                .filter(|size| *size <= self.max_bytes)
                .ok_or_else(|| anyhow::anyhow!("mod IPC byte queue is full"))?;
            match self.queued.compare_exchange_weak(
                queued,
                reserved,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(current) => queued = current,
            }
        }
        self.sender
            .try_send(Line {
                text,
                queued: Arc::clone(&self.queued),
            })
            .context("mod IPC message queue is full or closed")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Blocked {
        started: Option<mpsc::Sender<()>>,
        resume: mpsc::Receiver<()>,
    }
    impl Write for Blocked {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if let Some(started) = self.started.take() {
                let _ = started.send(());
                let _ = self.resume.recv();
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn blocked_mod_pipe_has_bounded_nonblocking_writes() {
        let (started, ready) = mpsc::channel();
        let (resume, receiver) = mpsc::channel();
        let writer = PipeWriter::with_limits(
            Blocked {
                started: Some(started),
                resume: receiver,
            },
            "test".into(),
            2,
            16,
        )
        .expect("writer");
        writer.send("first".into()).expect("first line");
        ready
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("writer blocked");
        let (done, result) = mpsc::channel();
        let producer = thread::spawn(move || {
            let failed = (0..10).any(|_| writer.send("next".into()).is_err());
            let _ = done.send(failed);
        });
        let nonblocking = result.recv_timeout(std::time::Duration::from_secs(1));
        let _ = resume.send(());
        producer.join().expect("producer");
        assert!(nonblocking.expect("enqueue must never wait for the pipe"));
    }
}
