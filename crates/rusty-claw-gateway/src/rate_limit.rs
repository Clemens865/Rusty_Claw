//! Per-IP WebSocket connection rate limiter.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tracing::{debug, warn};

/// Simple in-memory per-IP rate limiter for WebSocket connections.
pub struct RateLimiter {
    max_connections_per_ip: u32,
    connections: Arc<Mutex<HashMap<IpAddr, Vec<Instant>>>>,
}

impl RateLimiter {
    /// Create a new rate limiter.
    pub fn new(max_connections_per_ip: u32) -> Self {
        let limiter = Self {
            max_connections_per_ip,
            connections: Arc::new(Mutex::new(HashMap::new())),
        };

        // Spawn background cleanup task
        let connections = limiter.connections.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                let mut map = connections.lock().unwrap();
                let cutoff = Instant::now() - std::time::Duration::from_secs(60);
                map.retain(|_, timestamps| {
                    timestamps.retain(|t| *t > cutoff);
                    !timestamps.is_empty()
                });
                debug!(entries = map.len(), "Rate limiter cleanup");
            }
        });

        limiter
    }

    /// Check if a connection from this IP should be allowed.
    /// Returns true if allowed, false if rate limited.
    pub fn check(&self, ip: IpAddr) -> bool {
        let mut map = self.connections.lock().unwrap();
        let cutoff = Instant::now() - std::time::Duration::from_secs(60);

        let timestamps = map.entry(ip).or_default();

        // Remove stale entries
        timestamps.retain(|t| *t > cutoff);

        if timestamps.len() >= self.max_connections_per_ip as usize {
            warn!(%ip, count = timestamps.len(), limit = self.max_connections_per_ip,
                "Rate limited: too many connections from IP");
            return false;
        }

        timestamps.push(Instant::now());
        true
    }

    /// Record that a connection from this IP was closed.
    pub fn release(&self, ip: IpAddr) {
        let mut map = self.connections.lock().unwrap();
        if let Some(timestamps) = map.get_mut(&ip) {
            // Remove the oldest entry
            if !timestamps.is_empty() {
                timestamps.remove(0);
            }
            if timestamps.is_empty() {
                map.remove(&ip);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[tokio::test]
    async fn test_rate_limiter_allows() {
        let limiter = RateLimiter::new(3);
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));

        assert!(limiter.check(ip));
        assert!(limiter.check(ip));
        assert!(limiter.check(ip));
    }

    #[tokio::test]
    async fn test_rate_limiter_blocks() {
        let limiter = RateLimiter::new(2);
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));

        assert!(limiter.check(ip));
        assert!(limiter.check(ip));
        assert!(!limiter.check(ip)); // Should be blocked
    }

    #[tokio::test]
    async fn test_rate_limiter_different_ips() {
        let limiter = RateLimiter::new(1);
        let ip1 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));

        assert!(limiter.check(ip1));
        assert!(limiter.check(ip2));
        assert!(!limiter.check(ip1)); // Blocked
        assert!(!limiter.check(ip2)); // Blocked
    }

    #[tokio::test]
    async fn test_rate_limiter_release() {
        let limiter = RateLimiter::new(1);
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));

        assert!(limiter.check(ip));
        assert!(!limiter.check(ip));
        limiter.release(ip);
        assert!(limiter.check(ip));
    }
}
