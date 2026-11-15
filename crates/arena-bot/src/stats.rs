//! Sample collection and percentile summaries.

use std::time::Duration;

use serde::Serialize;

/// Durations in microseconds, summarized at the end of a run.
#[derive(Clone, Debug, Default)]
pub struct Samples(Vec<u32>);
impl Samples {
    pub fn record(&mut self, duration: Duration) {
        self.0
            .push(u32::try_from(duration.as_micros()).unwrap_or(u32::MAX));
    }
    pub fn extend(&mut self, other: Samples) {
        self.0.extend(other.0);
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn summary(&mut self) -> Summary {
        self.0.sort_unstable();
        let at = |fraction: f64| -> f64 {
            if self.0.is_empty() {
                return 0.0;
            }
            // Nearest-rank percentile.
            let rank = ((fraction * self.0.len() as f64).ceil() as usize).clamp(1, self.0.len());
            f64::from(self.0[rank - 1]) / 1000.0
        };
        Summary {
            count: self.0.len(),
            p50_ms: at(0.50),
            p95_ms: at(0.95),
            p99_ms: at(0.99),
            max_ms: at(1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Summary {
    pub count: usize,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
}
impl std::fmt::Display for Summary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "p50 {:7.1} ms  p95 {:7.1} ms  p99 {:7.1} ms  max {:7.1} ms  (n={})",
            self.p50_ms, self.p95_ms, self.p99_ms, self.max_ms, self.count
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_use_nearest_rank() {
        let mut samples = Samples::default();
        assert_eq!(samples.summary(), Summary::default());
        for ms in 1..=100 {
            samples.record(Duration::from_millis(ms));
        }
        let summary = samples.summary();
        assert_eq!(summary.count, 100);
        assert_eq!(
            (
                summary.p50_ms,
                summary.p95_ms,
                summary.p99_ms,
                summary.max_ms
            ),
            (50.0, 95.0, 99.0, 100.0)
        );
    }
}
