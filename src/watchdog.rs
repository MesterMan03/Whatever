use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

pub struct Watchdog {
    state: Arc<Mutex<Option<Instant>>>,
}

impl Watchdog {
    pub fn new(warn_after: Duration) -> Self {
        let state = Arc::new(Mutex::new(None::<Instant>));
        let weak: Weak<Mutex<Option<Instant>>> = Arc::downgrade(&state);

        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let Some(arc) = weak.upgrade() else { return };
                let Ok(guard) = arc.lock() else { return };
                if let Some(start) = *guard {
                    let elapsed = start.elapsed();
                    if elapsed >= warn_after * 2 {
                        tracing::error!(
                            elapsed_ms = elapsed.as_millis(),
                            "tick has been running for {:?} — script may be hung",
                            elapsed
                        );
                    } else if elapsed >= warn_after {
                        tracing::warn!(
                            elapsed_ms = elapsed.as_millis(),
                            "tick is taking {:?} — script slow?",
                            elapsed
                        );
                    }
                }
            }
        });

        Watchdog { state }
    }

    pub fn start_tick(&self) {
        if let Ok(mut g) = self.state.lock() {
            *g = Some(Instant::now());
        }
    }

    pub fn end_tick(&self) {
        if let Ok(mut g) = self.state.lock() {
            *g = None;
        }
    }
}
