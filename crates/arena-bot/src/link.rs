//! Simulated network impairments, applied inside the bot instead of a proxy.
//!
//! Each direction of a connection goes through a [`DelayLine`]: an item
//! queued at time `t` is released at `t + one_way + U(0, jitter)` (optionally
//! quantized to `clump` boundaries and stalled by simulated TCP retransmits
//! when `loss_rate > 0`), and never before the item queued ahead of it,
//! because WebSockets run over TCP and stay ordered. A lost packet therefore
//! stalls everything behind it (head-of-line blocking) and releases queued
//! frames in a burst once retransmitted.

use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::Instant;

#[derive(Clone, Copy, Debug, Default)]
pub struct Link {
    pub one_way: Duration,
    pub jitter: Duration,
    /// Probability in `[0.0, 1.0]` that a frame triggers a simulated TCP
    /// packet-loss retransmit stall (head-of-line blocking everything behind it).
    pub loss_rate: f64,
    /// Extra stall duration added on simulated loss. When zero, defaults to
    /// `max(2 × one_way, 20 ms) + jitter`.
    pub retransmit_delay: Duration,
    /// Optional relay/Wi-Fi batching window; non-zero holds frames until the
    /// next clump flush while preserving TCP order.
    pub clump: Duration,
}
impl Link {
    /// A link with the given base round trip and per-direction jitter.
    pub fn round_trip(rtt: Duration, jitter: Duration) -> Self {
        Self {
            one_way: rtt / 2,
            jitter,
            loss_rate: 0.0,
            retransmit_delay: Duration::ZERO,
            clump: Duration::ZERO,
        }
    }

    /// Sets the per-frame simulated TCP packet-loss probability (clamped to `0.0..=1.0`).
    pub fn with_loss(mut self, loss_rate: f64) -> Self {
        self.loss_rate = if loss_rate.is_finite() {
            loss_rate.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self
    }

    /// Sets an optional relay clumping period.
    pub fn with_clump(mut self, clump: Duration) -> Self {
        self.clump = clump;
        self
    }

    /// Extra head-of-line delay added when a simulated packet loss occurs.
    pub fn effective_retransmit_delay(&self) -> Duration {
        if !self.retransmit_delay.is_zero() {
            self.retransmit_delay
        } else {
            self.one_way
                .saturating_mul(2)
                .max(Duration::from_millis(20))
                .saturating_add(self.jitter)
        }
    }
}

/// Small deterministic generator (xorshift64*), so runs can be repeated.
#[derive(Clone, Debug)]
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        // Never zero, and different seeds diverge immediately.
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    pub fn below(&mut self, limit: Duration) -> Duration {
        limit.mul_f64(self.unit())
    }
}

pub struct DelayLine<T> {
    sender: mpsc::UnboundedSender<(Instant, T)>,
}
impl<T> DelayLine<T> {
    /// Queues `item` now; `false` once the far side has gone.
    pub fn push(&self, item: T) -> bool {
        self.sender.send((Instant::now(), item)).is_ok()
    }
}

/// Starts a delay line; items come out of the receiver after the delay.
/// Dropping the [`DelayLine`] closes the receiver once queued items are out.
pub fn delay_line<T: Send + 'static>(
    link: Link,
    mut rng: Rng,
) -> (DelayLine<T>, mpsc::UnboundedReceiver<T>) {
    let (sender, mut queued) = mpsc::unbounded_channel::<(Instant, T)>();
    let (released, receiver) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let start = Instant::now();
        let mut last = start;
        let loss_rate = if link.loss_rate.is_finite() {
            link.loss_rate.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let retransmit = link.effective_retransmit_delay();
        while let Some((at, item)) = queued.recv().await {
            let mut due = at + link.one_way + rng.below(link.jitter);
            if !link.clump.is_zero() {
                // Always quantize to the next strictly-future clump boundary, matching
                // `((t / period).floor() + 1) * period` in the server's relay-clump model.
                let period = link.clump.as_nanos();
                let elapsed = due.saturating_duration_since(start).as_nanos();
                let bucket = elapsed / period + 1;
                let quantized = u64::try_from(bucket.saturating_mul(period)).unwrap_or(u64::MAX);
                due = start + Duration::from_nanos(quantized);
            }
            if loss_rate > 0.0 && rng.unit() < loss_rate {
                due += retransmit;
            }
            due = due.max(last);
            tokio::time::sleep_until(due).await;
            last = due;
            if released.send(item).is_err() {
                break;
            }
        }
    });
    (DelayLine { sender }, receiver)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn items_arrive_late_by_the_delay_and_in_order() {
        let link = Link::round_trip(Duration::from_millis(100), Duration::from_millis(20));
        let (line, mut out) = delay_line(link, Rng::new(7));
        let start = Instant::now();
        for item in 0..50 {
            assert!(line.push(item));
        }
        drop(line);
        let mut expected = 0;
        let mut previous = start;
        while let Some(item) = out.recv().await {
            assert_eq!(item, expected, "order is preserved");
            expected += 1;
            let waited = Instant::now() - start;
            assert!(waited >= Duration::from_millis(50), "{waited:?}");
            assert!(waited <= Duration::from_millis(71), "{waited:?}");
            assert!(Instant::now() >= previous);
            previous = Instant::now();
        }
        assert_eq!(expected, 50);
    }

    #[tokio::test(start_paused = true)]
    async fn simulated_tcp_loss_causes_head_of_line_stalls_without_reordering() {
        let link = Link::round_trip(Duration::from_millis(100), Duration::ZERO).with_loss(0.25);
        let (line, mut out) = delay_line(link, Rng::new(42));
        let start = Instant::now();
        let producer = tokio::spawn(async move {
            for item in 0..40 {
                assert!(line.push(item));
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
        let mut expected = 0;
        let mut saw_stall = false;
        let mut saw_burst = false;
        let mut previous = start;
        while let Some(item) = out.recv().await {
            assert_eq!(item, expected, "TCP order is preserved under loss");
            let now = Instant::now();
            if expected > 0 {
                let gap = now - previous;
                if gap >= Duration::from_millis(90) {
                    saw_stall = true;
                }
                if gap == Duration::ZERO {
                    saw_burst = true;
                }
            }
            previous = now;
            expected += 1;
        }
        producer.await.unwrap();
        assert_eq!(expected, 40);
        assert!(saw_stall, "expected at least one retransmit HoL stall");
        assert!(saw_burst, "expected queued frames to burst after a stall");
    }

    #[tokio::test(start_paused = true)]
    async fn relay_clumping_quantizes_arrivals_while_keeping_order() {
        let link = Link::round_trip(Duration::from_millis(20), Duration::ZERO)
            .with_clump(Duration::from_millis(50));
        let (line, mut out) = delay_line(link, Rng::new(9));
        let start = Instant::now();
        let producer = tokio::spawn(async move {
            for item in 0..10 {
                assert!(line.push(item));
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        let mut expected = 0;
        while let Some(item) = out.recv().await {
            assert_eq!(item, expected);
            let elapsed_ms = (Instant::now() - start).as_millis();
            assert_eq!(
                elapsed_ms % 50,
                0,
                "arrival {elapsed_ms}ms should land on a 50ms clump boundary"
            );
            expected += 1;
        }
        producer.await.unwrap();
        assert_eq!(expected, 10);
    }

    #[test]
    fn the_generator_is_deterministic_and_bounded() {
        let mut a = Rng::new(3);
        let mut b = Rng::new(3);
        assert_eq!(a.next_u64(), b.next_u64());
        assert_ne!(Rng::new(3).next_u64(), Rng::new(4).next_u64());
        let mut rng = Rng::new(1);
        assert!((0..10_000).all(|_| (0.0..1.0).contains(&rng.unit())));
        assert_eq!(rng.below(Duration::ZERO), Duration::ZERO);
        assert_eq!(Link::default().with_loss(f64::NAN).loss_rate, 0.0);
        assert_eq!(Link::default().with_loss(-0.5).loss_rate, 0.0);
        assert_eq!(Link::default().with_loss(2.0).loss_rate, 1.0);
        assert_eq!(
            Link::default().effective_retransmit_delay(),
            Duration::from_millis(20)
        );
    }
}
