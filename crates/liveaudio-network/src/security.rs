// SPDX-License-Identifier: MIT

use crate::error::NetworkError;
use std::net::SocketAddr;

pub const WS_PORT_FALLBACK_RANGE: u16 = 10;
pub static WS_ALLOWED_ORIGINS: &[&str] = &["http://absolute"];
pub static WS_LOOPBACK_HOSTS: &[&str] = &["localhost", "127.0.0.1", "::1"];

/// Calculate candidate fallback ports in range [base .. base + fallback_range - 1].
pub fn candidate_ports(base: u16, fallback_range: Option<u16>) -> Vec<u16> {
    let range = fallback_range.unwrap_or(WS_PORT_FALLBACK_RANGE);
    let top = (base as u32 + range as u32 - 1).min(65535) as u16;
    (base..=top).collect()
}

/// Validate whether an HTTP/WebSocket Origin header is permitted.
///
/// Rules:
/// 1. Missing origin or empty -> Allowed (native desktop/CLI clients and scripts).
/// 2. `http://absolute` -> Allowed (CEF internal origin for OBS Browser Source).
/// 3. Loopback hosts (`localhost`, `127.0.0.1`, `::1`) -> Allowed for local web interfaces.
/// 4. Any external web origin -> Strictly rejected (prevents drive-by eavesdropping).
pub fn is_origin_allowed(origin: Option<&str>) -> bool {
    let origin_str = match origin {
        Some(o) if !o.trim().is_empty() => o.trim(),
        _ => return true, // Non-browser client has no origin header
    };

    if WS_ALLOWED_ORIGINS.contains(&origin_str) {
        return true;
    }

    let without_scheme = if let Some(stripped) = origin_str.strip_prefix("http://") {
        stripped
    } else if let Some(stripped) = origin_str.strip_prefix("https://") {
        stripped
    } else {
        return false;
    };

    let authority = without_scheme.split('/').next().unwrap_or("");
    if authority.is_empty() {
        return false;
    }

    // Userinfo check: if authority has '@', real host is after the last '@'
    let host_and_port = if let Some(idx) = authority.rfind('@') {
        &authority[idx + 1..]
    } else {
        authority
    };

    let host = if host_and_port.starts_with('[') {
        if let Some(end) = host_and_port.find(']') {
            &host_and_port[1..end]
        } else {
            return false;
        }
    } else {
        host_and_port.split(':').next().unwrap_or("")
    };

    WS_LOOPBACK_HOSTS.contains(&host)
}

/// Validate slice of Origin header values from incoming handshake request.
/// Returns Ok(()) if allowed, or Err(NetworkError::ForbiddenOrigin) if forbidden.
pub fn validate_origin_headers(origins: &[&str]) -> Result<(), NetworkError> {
    if origins.is_empty() {
        return Ok(());
    }
    if origins.len() > 1 {
        let joined = origins.join(", ");
        return Err(NetworkError::ForbiddenOrigin(joined));
    }
    let origin = origins[0];
    if is_origin_allowed(Some(origin)) {
        Ok(())
    } else {
        Err(NetworkError::ForbiddenOrigin(origin.to_string()))
    }
}

/// Verify that the remote peer socket is a loopback connection.
pub fn is_remote_addr_allowed(addr: &SocketAddr) -> bool {
    addr.ip().is_loopback()
}
