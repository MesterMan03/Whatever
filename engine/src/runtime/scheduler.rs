use anyhow::{Context, bail};
use std::time::{Duration, Instant};

const MAX_CATCH_UP_TICKS: u32 = 4;

pub struct TickClock {
    pub interval: Duration,
    pub next: Instant,
}
impl TickClock {
    pub fn new(rate: f64) -> anyhow::Result<Self> {
        let interval = Self::interval_for_rate(rate)?;
        Ok(Self {
            interval,
            next: Instant::now() + interval,
        })
    }
    pub fn interval_for_rate(rate: f64) -> anyhow::Result<Duration> {
        if !rate.is_finite() || rate <= 0.0 {
            bail!("tick rate must be positive and finite");
        }
        let interval =
            Duration::try_from_secs_f64(1.0 / rate).context("tick interval is out of range")?;
        if interval.is_zero() || Instant::now().checked_add(interval).is_none() {
            bail!("tick interval is out of range");
        }
        Ok(interval)
    }
    pub fn set_rate(&mut self, rate: f64) -> anyhow::Result<()> {
        self.interval = Self::interval_for_rate(rate)?;
        self.next = Instant::now() + self.interval;
        Ok(())
    }
    pub fn complete(&mut self, now: Instant) {
        if self.next > now {
            return;
        } // A rate change during this tick already set its next deadline.
        self.next += self.interval;
        // Let short overruns catch up. After a long stall, drop the backlog
        // but run the next step immediately instead of adding an idle interval.
        if now.saturating_duration_since(self.next)
            >= self.interval.saturating_mul(MAX_CATCH_UP_TICKS)
        {
            self.next = now;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deadlines_do_not_depend_on_render_frames() {
        let mut clock = TickClock::new(100.0).expect("clock");
        let first = clock.next;
        clock.complete(first);
        assert_eq!(clock.next, first + Duration::from_millis(10));
        let late = first + Duration::from_secs(1);
        clock.complete(late);
        assert_eq!(clock.next, late);
    }
    #[test]
    fn a_short_overrun_keeps_the_fixed_deadline() {
        let mut clock = TickClock::new(100.0).expect("clock");
        let first = clock.next;
        clock.complete(first + Duration::from_millis(12));
        assert_eq!(clock.next, first + Duration::from_millis(10));
        clock.complete(first + Duration::from_millis(14));
        assert_eq!(clock.next, first + Duration::from_millis(20));
    }
    #[test]
    fn invalid_rates_cannot_panic_the_runtime() {
        for rate in [
            0.0,
            -1.0,
            f64::NAN,
            f64::INFINITY,
            f64::MAX,
            f64::MIN_POSITIVE,
        ] {
            assert!(TickClock::new(rate).is_err());
        }
    }
    #[test]
    fn a_rate_change_during_a_tick_does_not_add_an_extra_interval() {
        let mut clock = TickClock::new(100.0).expect("clock");
        clock.set_rate(50.0).expect("new rate");
        let next = clock.next;
        clock.complete(next - Duration::from_millis(1));
        assert_eq!(clock.next, next);
    }
}
