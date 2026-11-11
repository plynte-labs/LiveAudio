// SPDX-License-Identifier: MIT

use crate::error::NetworkError;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

pub const WS_PORT_FALLBACK_RANGE: u16 = 10;
pub const MAX_REJECTED_ORIGINS: usize = 20;
pub const MAX_REJECTED_ORIGIN_LEN: usize = 128;
pub static WS_ALLOWED_ORIGINS: &[&str] = &[
    "http://absolute",
    // Tauri custom-protocol origins (WebView2 on Windows, WebKitGTK on Linux). Exact matches only,
    // no port: they are only reachable by local Tauri webviews, never web pages.
    "http://tauri.localhost",
    "https://tauri.localhost",
    "tauri://localhost",
];
pub static WS_LOOPBACK_HOSTS: &[&str] = &["localhost", "127.0.0.1", "::1"];

/// Calculate candidate fallback ports in range [base .. base + fallback_range - 1].
pub fn candidate_ports(base: u16, fallback_range: Option<u16>) -> Vec<u16> {
    let range = fallback_range.unwrap_or(WS_PORT_FALLBACK_RANGE);
    let top = (base as u32 + range as u32 - 1).min(65535) as u16;
    (base..=top).collect()
}

/// Extract the port number from a loopback Origin string, or return None if not a valid loopback origin.
pub fn extract_loopback_port(origin_str: &str) -> Option<u16> {
    let origin_str = origin_str.trim();
    let (default_port, without_scheme) = if let Some(stripped) = origin_str.strip_prefix("http://") {
        (80, stripped)
    } else if let Some(stripped) = origin_str.strip_prefix("https://") {
        (443, stripped)
    } else {
        return None;
    };

    let authority = without_scheme.split('/').next().unwrap_or("");
    if authority.is_empty() {
        return None;
    }

    // Userinfo check: if authority has '@', real host is after the last '@'
    let host_and_port = if let Some(idx) = authority.rfind('@') {
        &authority[idx + 1..]
    } else {
        authority
    };

    // Split off the host; bracketed IPv6 literals keep their colons inside the brackets.
    let (host, port) = if let Some(bracketed) = host_and_port.strip_prefix('[') {
        let end = bracketed.find(']')?;
        match &bracketed[end + 1..] {
            "" => (&bracketed[..end], None),
            after => match after.strip_prefix(':') {
                Some(port) => (&bracketed[..end], Some(port)),
                None => return None,
            },
        }
    } else {
        match host_and_port.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (host_and_port, None),
        }
    };

    if !WS_LOOPBACK_HOSTS.contains(&host) {
        return None;
    }

    match port {
        Some(text) => text.parse::<u16>().ok().filter(|&p| p > 0),
        None => Some(default_port),
    }
}

/// Validate whether an HTTP/WebSocket Origin header is permitted.
///
/// Rules:
/// 1. Missing origin or empty -> Allowed (native desktop/CLI clients and scripts).
/// 2. `http://absolute` -> Allowed (CEF internal origin for OBS Browser Source).
/// 3. `http://tauri.localhost` / `https://tauri.localhost` / `tauri://localhost` -> Allowed (exact match, no port;
///    origin of the installed Tauri desktop webview on Windows/Linux). Other `*.localhost` hosts are rejected.
/// 4. Loopback hosts (`localhost`, `127.0.0.1`, `::1`) -> Allowed on `bound_port` or any port in `allowed_ports`.
/// 5. Any external web origin or "null" -> Strictly rejected (prevents drive-by eavesdropping and CSWSH).
pub fn is_origin_allowed(origin: Option<&str>, bound_port: u16, allowed_ports: &[u16]) -> bool {
    let origin_str = match origin {
        Some(o) if !o.trim().is_empty() => o.trim(),
        _ => return true, // Non-browser client has no origin header
    };

    if WS_ALLOWED_ORIGINS.contains(&origin_str) {
        return true;
    }

    if let Some(port) = extract_loopback_port(origin_str) {
        port == bound_port || allowed_ports.contains(&port)
    } else {
        false
    }
}

/// Validate slice of Origin header values from incoming handshake request.
/// Returns Ok(()) if allowed, or Err(NetworkError::ForbiddenOrigin) if forbidden.
pub fn validate_origin_headers(
    origins: &[&str],
    bound_port: u16,
    allowed_ports: &[u16],
) -> Result<(), NetworkError> {
    if origins.is_empty() {
        return Ok(());
    }
    if origins.len() > 1 {
        let joined = origins.join(", ");
        return Err(NetworkError::ForbiddenOrigin(joined));
    }
    let origin = origins[0];
    if is_origin_allowed(Some(origin), bound_port, allowed_ports) {
        Ok(())
    } else {
        Err(NetworkError::ForbiddenOrigin(origin.to_string()))
    }
}

/// Record of a rejected connection attempt by Origin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RejectedOriginEntry {
    pub origin: String,
    pub port: Option<u16>,
    pub count: u64,
    pub last_attempt_timestamp_ms: u64,
}

/// In-memory bounded tracker for recent rejected connection attempts.
#[derive(Debug, Clone, Default)]
pub struct RejectedOriginTracker {
    entries: Vec<RejectedOriginEntry>,
}

impl RejectedOriginTracker {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn record(&mut self, origin_raw: &str) {
        let trimmed = origin_raw.trim();
        if trimmed.is_empty() {
            return;
        }
        let sanitized: String = trimmed
            .chars()
            .filter(|c| !c.is_ascii_control())
            .take(MAX_REJECTED_ORIGIN_LEN)
            .collect();

        if sanitized.is_empty() {
            return;
        }

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        if let Some(entry) = self.entries.iter_mut().find(|e| e.origin == sanitized) {
            entry.count = entry.count.saturating_add(1);
            entry.last_attempt_timestamp_ms = now_ms;
        } else {
            let port = extract_loopback_port(&sanitized);
            if self.entries.len() >= MAX_REJECTED_ORIGINS {
                if let Some((idx, _)) = self
                    .entries
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, e)| e.last_attempt_timestamp_ms)
                {
                    self.entries.remove(idx);
                }
            }
            self.entries.push(RejectedOriginEntry {
                origin: sanitized,
                port,
                count: 1,
                last_attempt_timestamp_ms: now_ms,
            });
        }
    }

    pub fn remove_by_port(&mut self, port: u16) {
        self.entries.retain(|e| e.port != Some(port));
    }

    pub fn snapshot(&self) -> Vec<RejectedOriginEntry> {
        let mut copy = self.entries.clone();
        copy.sort_by(|a, b| b.last_attempt_timestamp_ms.cmp(&a.last_attempt_timestamp_ms));
        copy
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Verify that the remote peer socket is a loopback connection.
pub fn is_remote_addr_allowed(addr: &SocketAddr) -> bool {
    addr.ip().is_loopback()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allowed_origins() {
        assert!(is_origin_allowed(None, 8765, &[]));
        assert!(is_origin_allowed(Some(""), 8765, &[]));
        assert!(is_origin_allowed(Some("http://absolute"), 8765, &[]));
        assert!(is_origin_allowed(Some("http://tauri.localhost"), 8765, &[]));
        assert!(is_origin_allowed(Some("https://tauri.localhost"), 8765, &[]));
        assert!(is_origin_allowed(Some("tauri://localhost"), 8765, &[]));
        assert!(is_origin_allowed(Some("http://127.0.0.1:8765"), 8765, &[]));
        assert!(is_origin_allowed(Some("http://localhost:8765"), 8765, &[]));
        assert!(is_origin_allowed(Some("http://[::1]:8765"), 8765, &[]));
    }

    #[test]
    fn test_rejected_origins_including_null() {
        assert!(!is_origin_allowed(Some("null"), 8765, &[]));
        assert!(!is_origin_allowed(Some(" null "), 8765, &[]));
        assert!(!is_origin_allowed(Some("http://evil.com"), 8765, &[]));
        assert!(!is_origin_allowed(Some("https://malicious.org"), 8765, &[]));
        assert!(!is_origin_allowed(Some("http://attacker.localhost"), 8765, &[]));
        assert!(!is_origin_allowed(Some("file://"), 8765, &[]));
        assert!(!is_origin_allowed(Some("tauri://attacker.com"), 8765, &[]));
    }

    #[test]
    fn test_loopback_origins_allowed_only_on_the_bound_port() {
        for origin in [
            "http://localhost:8765",
            "http://127.0.0.1:8765",
            "http://[::1]:8765",
        ] {
            assert!(is_origin_allowed(Some(origin), 8765, &[]), "{origin} should be allowed");
        }
        for origin in [
            "http://localhost:1420",
            "http://127.0.0.1:5173",
            "https://localhost:3000",
            "http://[::1]:8080",
            "http://localhost",
            "https://127.0.0.1",
            "http://localhost:8765:1",
            "http://localhost:",
            "http://localhost:99999",
            "http://[::1]x:8765",
            "http://localhost:8765@evil.com",
        ] {
            assert!(!is_origin_allowed(Some(origin), 8765, &[]), "{origin} should be rejected");
        }
    }

    #[test]
    fn test_loopback_origins_with_allowed_ports() {
        let allowed = [1420, 5173];
        assert!(is_origin_allowed(Some("http://localhost:1420"), 8765, &allowed));
        assert!(is_origin_allowed(Some("http://127.0.0.1:5173"), 8765, &allowed));
        assert!(is_origin_allowed(Some("http://[::1]:1420"), 8765, &allowed));
        // Not in allowed list:
        assert!(!is_origin_allowed(Some("http://localhost:3000"), 8765, &allowed));
        // External origin with allowed port must still be rejected:
        assert!(!is_origin_allowed(Some("http://evil.com:1420"), 8765, &allowed));
    }

    #[test]
    fn test_loopback_origin_without_port_uses_scheme_default() {
        assert!(is_origin_allowed(Some("http://localhost"), 80, &[]));
        assert!(is_origin_allowed(Some("https://127.0.0.1"), 443, &[]));
        assert!(!is_origin_allowed(Some("https://127.0.0.1"), 80, &[]));
    }

    #[test]
    fn test_tauri_and_obs_origins_ignore_bound_port() {
        assert!(is_origin_allowed(Some("tauri://localhost"), 1, &[]));
        assert!(is_origin_allowed(Some("http://absolute"), 1, &[]));
    }

    #[test]
    fn test_validate_origin_headers() {
        assert!(validate_origin_headers(&[], 8765, &[]).is_ok());
        assert!(validate_origin_headers(&["tauri://localhost"], 8765, &[]).is_ok());
        assert!(validate_origin_headers(&["http://127.0.0.1:8765"], 8765, &[]).is_ok());
        assert!(validate_origin_headers(&["http://127.0.0.1:1420"], 8765, &[1420]).is_ok());
        assert!(matches!(
            validate_origin_headers(&["null"], 8765, &[]),
            Err(NetworkError::ForbiddenOrigin(ref o)) if o == "null"
        ));
        assert!(matches!(
            validate_origin_headers(&["tauri://localhost", "http://localhost:8765"], 8765, &[]),
            Err(NetworkError::ForbiddenOrigin(_))
        ));
    }

    #[test]
    fn test_rejected_origin_tracker() {
        let mut tracker = RejectedOriginTracker::new();
        assert_eq!(tracker.len(), 0);

        tracker.record("http://localhost:5173");
        assert_eq!(tracker.len(), 1);
        let snap = tracker.snapshot();
        assert_eq!(snap[0].origin, "http://localhost:5173");
        assert_eq!(snap[0].port, Some(5173));
        assert_eq!(snap[0].count, 1);

        // Deduplication
        tracker.record("http://localhost:5173");
        assert_eq!(tracker.len(), 1);
        let snap = tracker.snapshot();
        assert_eq!(snap[0].count, 2);

        // External origin
        tracker.record("https://evil.com/path");
        assert_eq!(tracker.len(), 2);
        let snap = tracker.snapshot();
        let evil = snap.iter().find(|e| e.origin == "https://evil.com/path").unwrap();
        assert_eq!(evil.port, None);

        // Remove by port
        tracker.remove_by_port(5173);
        assert_eq!(tracker.len(), 1);
        assert_eq!(tracker.snapshot()[0].origin, "https://evil.com/path");

        // Cap to MAX_REJECTED_ORIGINS (20)
        for i in 1..=30 {
            tracker.record(&format!("http://localhost:{}", 1000 + i));
        }
        assert_eq!(tracker.len(), MAX_REJECTED_ORIGINS);
    }
}
