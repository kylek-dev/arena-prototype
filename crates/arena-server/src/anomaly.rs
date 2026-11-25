//! Throttled warnings for anomalies (ADR 0054): tick overruns, dropped
//! frames and refused connections. An overloaded server or a flood of bad
//! connections must not turn into a flood of log lines or span events, so
//! each kind is reported at most once per window, with how many were
//! suppressed since the last report.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How often one kind of anomaly is reported.
pub const REPORT_EVERY: Duration = Duration::from_secs(10);

/// At most one report per window.
#[derive(Debug)]
pub struct Throttle {
    every: Duration,
    next: Option<Instant>,
    suppressed: u64,
}

impl Throttle {
    pub const fn new(every: Duration) -> Self {
        Self {
            every,
            next: None,
            suppressed: 0,
        }
    }

    /// `Some(suppressed)` when this occurrence should be reported, with the
    /// number held back since the last report; `None` to stay quiet.
    pub fn allow(&mut self, now: Instant) -> Option<u64> {
        if self.next.is_some_and(|next| now < next) {
            self.suppressed = self.suppressed.saturating_add(1);
            return None;
        }
        self.next = Some(now + self.every);
        Some(std::mem::take(&mut self.suppressed))
    }
}

/// A [`Throttle`] shared by every connection task.
#[derive(Debug)]
pub struct SharedThrottle(Mutex<Throttle>);

impl SharedThrottle {
    pub const fn new(every: Duration) -> Self {
        Self(Mutex::new(Throttle::new(every)))
    }

    pub fn allow(&self, now: Instant) -> Option<u64> {
        // A poisoned lock only means a panic elsewhere; keep reporting.
        let mut throttle = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        throttle.allow(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_once_per_window_with_the_suppressed_count() {
        let start = Instant::now();
        let mut throttle = Throttle::new(Duration::from_secs(10));
        assert_eq!(throttle.allow(start), Some(0));
        for second in 1..10 {
            assert_eq!(throttle.allow(start + Duration::from_secs(second)), None);
        }
        assert_eq!(throttle.allow(start + Duration::from_secs(10)), Some(9));
        assert_eq!(throttle.allow(start + Duration::from_secs(11)), None);
        assert_eq!(throttle.allow(start + Duration::from_secs(60)), Some(1));
    }

    #[test]
    fn the_shared_throttle_counts_across_threads() {
        let throttle = SharedThrottle::new(Duration::from_secs(3600));
        let now = Instant::now();
        let reported = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|_| scope.spawn(|| (0..250).filter(|_| throttle.allow(now).is_some()).count()))
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .sum::<usize>()
        });
        assert_eq!(reported, 1);
        assert_eq!(throttle.allow(now + Duration::from_secs(3600)), Some(999));
    }
}
