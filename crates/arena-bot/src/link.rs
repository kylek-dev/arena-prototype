//! Simulated network delay, applied inside the bot instead of a proxy.
//!
//! Each direction of a connection goes through a [`DelayLine`]: an item
//! queued at time `t` is released at `t + one_way + U(0, jitter)`, but never
//! before the item queued ahead of it, because WebSockets run over TCP and
//! stay ordered. A round trip therefore costs between `2 × one_way` and
//! `2 × (one_way + jitter)`.

use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::Instant;

#[derive(Clone, Copy, Debug, Default)]
pub struct Link {
    pub one_way: Duration,
    pub jitter: Duration,
}
impl Link {
    /// A link with the given base round trip and per-direction jitter.
    pub fn round_trip(rtt: Duration, jitter: Duration) -> Self {
        Self {
            one_way: rtt / 2,
            jitter,
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
        let mut last = Instant::now();
        while let Some((at, item)) = queued.recv().await {
            let due = (at + link.one_way + rng.below(link.jitter)).max(last);
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

    #[test]
    fn the_generator_is_deterministic_and_bounded() {
        let mut a = Rng::new(3);
        let mut b = Rng::new(3);
        assert_eq!(a.next_u64(), b.next_u64());
        assert_ne!(Rng::new(3).next_u64(), Rng::new(4).next_u64());
        let mut rng = Rng::new(1);
        assert!((0..10_000).all(|_| (0.0..1.0).contains(&rng.unit())));
        assert_eq!(rng.below(Duration::ZERO), Duration::ZERO);
    }
}
