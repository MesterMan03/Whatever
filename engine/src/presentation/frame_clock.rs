use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

/// Frame deadlines stay on their original phase rather than accumulating wake-up lateness.
pub(crate) struct FrameClock {
    cap: Option<f64>,
    interval: Option<Duration>,
    next: Instant,
}
impl Default for FrameClock {
    fn default() -> Self {
        Self {
            cap: None,
            interval: None,
            next: Instant::now(),
        }
    }
}
impl FrameClock {
    pub fn set_cap(&mut self, cap: Option<f64>, now: Instant) -> anyhow::Result<()> {
        if self.cap == cap {
            return Ok(());
        }
        let interval = match cap {
            Some(rate) => {
                anyhow::ensure!(
                    rate.is_finite() && rate > 0.0,
                    "FPS cap must be positive and finite"
                );
                let duration = Duration::try_from_secs_f64(1.0 / rate)?;
                anyhow::ensure!(
                    !duration.is_zero() && now.checked_add(duration).is_some(),
                    "FPS cap is out of range"
                );
                Some(duration)
            }
            None => None,
        };
        self.cap = cap;
        self.interval = interval;
        self.next = now;
        Ok(())
    }
    pub fn ready(&self, now: Instant) -> bool {
        self.interval.is_none() || now >= self.next
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.interval.map(|_| self.next)
    }
    pub fn complete(&mut self, start: Instant) {
        let Some(interval) = self.interval else {
            return;
        };
        if start < self.next {
            return;
        }
        let steps = start.duration_since(self.next).as_nanos() / interval.as_nanos() + 1;
        self.next = u32::try_from(steps)
            .ok()
            .and_then(|steps| interval.checked_mul(steps))
            .and_then(|duration| self.next.checked_add(duration))
            .or_else(|| start.checked_add(interval))
            .unwrap_or(start);
    }
}

/// A time-weighted rate of actual frames, rather than averaging reciprocals of noisy durations.
#[derive(Default)]
pub(crate) struct FrameRate {
    samples: VecDeque<Instant>,
}
impl FrameRate {
    pub fn record(&mut self, now: Instant) -> f32 {
        self.samples.push_back(now);
        while self.samples.len() > 2
            && now.duration_since(self.samples[0]) > Duration::from_millis(500)
        {
            self.samples.pop_front();
        }
        let elapsed = now.duration_since(self.samples[0]).as_secs_f32();
        if elapsed > 0.0 {
            (self.samples.len() - 1) as f32 / elapsed
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lateness_does_not_accumulate_and_missed_frames_do_not_burst() {
        let start = Instant::now();
        let mut clock = FrameClock::default();
        clock.set_cap(Some(60.0), start).expect("cap");
        let period = Duration::from_secs_f64(1.0 / 60.0);
        for n in 0..120 {
            let late = start + period * n + Duration::from_micros(800);
            assert!(clock.ready(late));
            clock.complete(late);
            assert_eq!(clock.deadline(), Some(start + period * (n + 1)));
        }
        clock.complete(start + period * 130 + Duration::from_micros(200));
        assert_eq!(clock.deadline(), Some(start + period * 131));
        assert!(!clock.ready(start + period * 131 - Duration::from_nanos(1)));
        clock.set_cap(None, start).expect("uncap");
        assert!(clock.ready(start));
        assert!(clock.deadline().is_none());
        assert!(clock.set_cap(Some(f64::NAN), start).is_err());
        assert!(clock.set_cap(Some(0.0), start).is_err());
    }
    #[test]
    fn fps_counts_elapsed_time_without_reciprocal_bias() {
        let start = Instant::now();
        let mut meter = FrameRate::default();
        meter.record(start);
        for n in 1..=20 {
            meter.record(start + Duration::from_millis(n * 20 - 5));
        }
        assert!((meter.record(start + Duration::from_millis(400)) - 52.5).abs() < 0.01);
    }
}
