//! Who is asking, and whether they may: the client's address (behind a
//! trusted reverse proxy), the page origin a browser reports, and
//! per-address limits. Shared by the world server and the front door.
//! Clocks are passed in so the limits are deterministic in tests.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::http::HeaderMap;
use axum::http::header::ORIGIN;
use tokio::time::Instant;

/// An address range in CIDR notation (`10.0.0.0/8`, `::1/128`, or a bare
/// address for exactly that one).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cidr {
    network: IpAddr,
    prefix: u8,
}
impl Cidr {
    pub fn parse(text: &str) -> Option<Self> {
        let (address, prefix) = match text.split_once('/') {
            Some((address, prefix)) => (address, Some(prefix.parse::<u8>().ok()?)),
            None => (text, None),
        };
        let network: IpAddr = address.parse().ok()?;
        let bits = if network.is_ipv4() { 32 } else { 128 };
        let prefix = prefix.unwrap_or(bits);
        (prefix <= bits).then_some(Self { network, prefix })
    }
    pub fn contains(&self, address: IpAddr) -> bool {
        match (self.network, canonical(address)) {
            (IpAddr::V4(network), IpAddr::V4(address)) => {
                let mask = u32::MAX
                    .checked_shl(32 - u32::from(self.prefix))
                    .unwrap_or(0);
                u32::from(network) & mask == u32::from(address) & mask
            }
            (IpAddr::V6(network), IpAddr::V6(address)) => {
                let mask = u128::MAX
                    .checked_shl(128 - u32::from(self.prefix))
                    .unwrap_or(0);
                u128::from(network) & mask == u128::from(address) & mask
            }
            _ => false,
        }
    }
}

/// IPv4-mapped IPv6 addresses (`::ffff:a.b.c.d`) as plain IPv4.
fn canonical(address: IpAddr) -> IpAddr {
    match address {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        v4 => v4,
    }
}

/// The client's address. Behind a reverse proxy every connection comes
/// from the proxy, so when the peer is a trusted proxy the address it
/// appended last to `X-Forwarded-For` is used instead. Earlier entries were
/// written by the client or proxies we do not control, and are ignored.
pub fn client_address(peer: SocketAddr, headers: &HeaderMap, trusted: &[Cidr]) -> IpAddr {
    let peer = canonical(peer.ip());
    if !trusted.iter().any(|range| range.contains(peer)) {
        return peer;
    }
    headers
        .get_all("x-forwarded-for")
        .iter()
        .next_back()
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit(',').next())
        .and_then(|last| last.trim().parse::<IpAddr>().ok())
        .map_or(peer, canonical)
}

/// The unit per-address limits count: one IPv4 address, or one IPv6 /64,
/// since a single home or phone usually holds a whole /64.
pub fn limit_key(address: IpAddr) -> IpAddr {
    match canonical(address) {
        IpAddr::V6(v6) => IpAddr::V6(Ipv6Addr::from(u128::from(v6) & (u128::MAX << 64))),
        v4 => v4,
    }
}

/// Which pages may open connections or create matches. Browsers always
/// report the page's origin on WebSocket handshakes and on `POST`s, so this
/// stops other websites from using our servers through their visitors.
/// Clients that are not browsers send no origin and are let through; their
/// tickets and the per-address limits still apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origins {
    Any,
    Only(Vec<String>),
}
impl Origins {
    /// Normalizes `https://Example.com/` to `https://example.com`. `None`
    /// unless it is an `http` or `https` origin without a path.
    pub fn parse_origin(text: &str) -> Option<String> {
        let origin = text.trim().trim_end_matches('/').to_ascii_lowercase();
        let rest = origin
            .strip_prefix("https://")
            .or_else(|| origin.strip_prefix("http://"))?;
        (!rest.is_empty() && !rest.contains(['/', '?', '#', '@', ' '])).then_some(origin)
    }
    pub fn allows(&self, headers: &HeaderMap) -> bool {
        let Self::Only(allowed) = self else {
            return true;
        };
        match headers.get(ORIGIN) {
            None => true,
            Some(origin) => origin
                .to_str()
                .ok()
                .and_then(Self::parse_origin)
                .is_some_and(|origin| allowed.contains(&origin)),
        }
    }
}

/// Counts open connections per address and refuses beyond a limit.
#[derive(Debug)]
pub struct OpenPerAddress {
    limit: u32,
    open: Mutex<HashMap<IpAddr, u32>>,
}

/// One counted connection; dropping it releases the count.
#[derive(Debug)]
pub struct OpenSlot {
    owner: Arc<OpenPerAddress>,
    key: IpAddr,
}
impl Drop for OpenSlot {
    fn drop(&mut self) {
        let mut open = self.owner.open.lock().expect("address count lock");
        if let Some(count) = open.get_mut(&self.key) {
            *count -= 1;
            if *count == 0 {
                open.remove(&self.key);
            }
        }
    }
}

impl OpenPerAddress {
    /// `limit` of zero means unlimited.
    pub fn new(limit: u32) -> Arc<Self> {
        Arc::new(Self {
            limit,
            open: Mutex::default(),
        })
    }
    /// A slot for one more connection from `address`, or `None` when that
    /// address already holds the limit.
    pub fn open(self: &Arc<Self>, address: IpAddr) -> Option<OpenSlot> {
        let key = limit_key(address);
        let mut open = self.open.lock().expect("address count lock");
        let count = open.entry(key).or_insert(0);
        if self.limit != 0 && *count >= self.limit {
            return None;
        }
        *count += 1;
        Some(OpenSlot {
            owner: self.clone(),
            key,
        })
    }
    pub fn addresses(&self) -> usize {
        self.open.lock().expect("address count lock").len()
    }
}

/// A token bucket: up to `burst` actions at once, refilling
/// one every `every`, per key: client addresses (match creation, login
/// routes) or accounts (friend requests and invites).
#[derive(Debug)]
pub struct RateLimit<K> {
    burst: u32,
    every: Duration,
    buckets: Mutex<HashMap<K, Bucket>>,
}
/// Per client address.
pub type RatePerAddress = RateLimit<IpAddr>;
/// Per account (friend requests and invites, social-identity step 4).
pub type RatePerAccount = RateLimit<u64>;
#[derive(Clone, Copy, Debug)]
struct Bucket {
    tokens: u32,
    /// When the next token is added.
    refill_at: Instant,
}
/// Buckets kept before full ones are forgotten.
const BUCKETS_BEFORE_PRUNING: usize = 4096;

/// What a rate limit counts by: addresses group IPv6 by /64, as
/// `limit_key` does; accounts count alone.
pub trait RateKey: Eq + std::hash::Hash + Copy {
    fn bucket(self) -> Self;
}
impl RateKey for IpAddr {
    fn bucket(self) -> Self {
        limit_key(self)
    }
}
impl RateKey for u64 {
    fn bucket(self) -> Self {
        self
    }
}

impl<K: RateKey> RateLimit<K> {
    pub fn new(burst: u32, every: Duration) -> Self {
        assert!(burst > 0 && !every.is_zero());
        Self {
            burst,
            every,
            buckets: Mutex::default(),
        }
    }
    fn refill(&self, bucket: &mut Bucket, now: Instant) {
        while bucket.tokens < self.burst && now >= bucket.refill_at {
            bucket.tokens += 1;
            bucket.refill_at += self.every;
        }
        if bucket.tokens == self.burst {
            bucket.refill_at = now + self.every;
        }
    }
    /// Takes one token; `Err` with how long until the next one otherwise.
    pub fn take(&self, address: K, now: Instant) -> Result<(), Duration> {
        let mut buckets = self.buckets.lock().expect("rate bucket lock");
        if buckets.len() >= BUCKETS_BEFORE_PRUNING {
            buckets.retain(|_, bucket| {
                self.refill(bucket, now);
                bucket.tokens < self.burst
            });
        }
        let bucket = buckets.entry(address.bucket()).or_insert(Bucket {
            tokens: self.burst,
            refill_at: now + self.every,
        });
        self.refill(bucket, now);
        if bucket.tokens == 0 {
            return Err(bucket.refill_at.saturating_duration_since(now));
        }
        bucket.tokens -= 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }
    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.append(*name, HeaderValue::from_str(value).unwrap());
        }
        headers
    }

    #[test]
    fn ranges_match_their_addresses_only() {
        let private = Cidr::parse("172.16.0.0/12").unwrap();
        assert!(private.contains(ip("172.31.255.1")));
        assert!(!private.contains(ip("172.32.0.1")));
        assert!(private.contains(ip("::ffff:172.20.0.5")), "mapped IPv4");
        assert!(!private.contains(ip("fd00::1")));
        let one = Cidr::parse("127.0.0.1").unwrap();
        assert!(one.contains(ip("127.0.0.1")) && !one.contains(ip("127.0.0.2")));
        assert!(Cidr::parse("0.0.0.0/0").unwrap().contains(ip("8.8.8.8")));
        assert!(Cidr::parse("fd00::/8").unwrap().contains(ip("fd12::1")));
        for bad in [
            "",
            "10.0.0.0/33",
            "::/129",
            "10.0.0/8",
            "10.0.0.0/x",
            "host",
        ] {
            assert_eq!(Cidr::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn forwarded_addresses_count_only_from_trusted_proxies() {
        let proxy: SocketAddr = "10.0.0.2:5000".parse().unwrap();
        let stranger: SocketAddr = "203.0.113.9:5000".parse().unwrap();
        let trusted = [Cidr::parse("10.0.0.0/8").unwrap()];
        // A client claims to be 1.1.1.1; the proxy appends the real address.
        let forwarded = headers(&[("x-forwarded-for", "1.1.1.1, 198.51.100.7")]);
        assert_eq!(
            client_address(proxy, &forwarded, &trusted),
            ip("198.51.100.7")
        );
        // Repeated headers: the last one is the proxy's.
        let repeated = headers(&[
            ("x-forwarded-for", "1.1.1.1"),
            ("x-forwarded-for", "198.51.100.8"),
        ]);
        assert_eq!(
            client_address(proxy, &repeated, &trusted),
            ip("198.51.100.8")
        );
        // A direct connection cannot claim an address.
        assert_eq!(
            client_address(stranger, &forwarded, &trusted),
            ip("203.0.113.9")
        );
        assert_eq!(client_address(proxy, &forwarded, &[]), ip("10.0.0.2"));
        // Missing or garbled headers fall back to the peer.
        assert_eq!(
            client_address(proxy, &HeaderMap::new(), &trusted),
            ip("10.0.0.2")
        );
        let garbled = headers(&[("x-forwarded-for", "1.1.1.1, nonsense")]);
        assert_eq!(client_address(proxy, &garbled, &trusted), ip("10.0.0.2"));
    }

    #[test]
    fn origins_are_normalized_and_checked_only_when_present() {
        assert_eq!(
            Origins::parse_origin("HTTPS://Arena.Example.com/"),
            Some("https://arena.example.com".into())
        );
        assert_eq!(
            Origins::parse_origin("http://localhost:8000"),
            Some("http://localhost:8000".into())
        );
        for bad in [
            "arena.example.com",
            "https://",
            "https://a.com/path",
            "ftp://a.com",
            "null",
        ] {
            assert_eq!(Origins::parse_origin(bad), None, "{bad}");
        }
        let only = Origins::Only(vec!["https://arena.example.com".into()]);
        assert!(only.allows(&headers(&[("origin", "https://arena.example.com")])));
        assert!(only.allows(&headers(&[("origin", "https://ARENA.example.com")])));
        assert!(!only.allows(&headers(&[("origin", "https://evil.example")])));
        assert!(!only.allows(&headers(&[("origin", "null")])));
        assert!(only.allows(&HeaderMap::new()), "not a browser");
        assert!(Origins::Any.allows(&headers(&[("origin", "https://evil.example")])));
    }

    #[test]
    fn open_connections_are_limited_per_address_and_released_on_drop() {
        let limits = OpenPerAddress::new(2);
        let first = limits.open(ip("198.51.100.1")).unwrap();
        let _second = limits.open(ip("198.51.100.1")).unwrap();
        assert!(limits.open(ip("198.51.100.1")).is_none());
        assert!(limits.open(ip("198.51.100.2")).is_some(), "another address");
        drop(first);
        assert!(limits.open(ip("198.51.100.1")).is_some());
        // One IPv6 /64 counts as one address.
        let _a = limits.open(ip("2001:db8:1:2::1")).unwrap();
        let _b = limits.open(ip("2001:db8:1:2::ffff")).unwrap();
        assert!(limits.open(ip("2001:db8:1:2:abcd::9")).is_none());
        assert!(limits.open(ip("2001:db8:1:3::1")).is_some());
        let unlimited = OpenPerAddress::new(0);
        let held: Vec<_> = (0..100)
            .map(|_| unlimited.open(ip("10.0.0.1")).unwrap())
            .collect();
        drop(held);
        assert_eq!(unlimited.addresses(), 0, "released counts are forgotten");
    }

    #[test]
    fn creation_buckets_allow_a_burst_then_refill_steadily() {
        let start = Instant::now();
        let every = Duration::from_secs(10);
        let rate = RatePerAddress::new(3, every);
        let address = ip("198.51.100.1");
        assert!((0..3).all(|_| rate.take(address, start).is_ok()));
        assert_eq!(rate.take(address, start), Err(every));
        assert!(rate.take(ip("198.51.100.2"), start).is_ok());
        assert_eq!(
            rate.take(address, start + Duration::from_secs(4)),
            Err(Duration::from_secs(6))
        );
        assert!(rate.take(address, start + every).is_ok());
        assert!(rate.take(address, start + every).is_err());
        // A long pause refills to the burst, never beyond it.
        let later = start + every * 100;
        assert!((0..3).all(|_| rate.take(address, later).is_ok()));
        assert!(rate.take(address, later).is_err());
    }
}
