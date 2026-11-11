// SPDX-License-Identifier: MIT

use liveaudio_network::security::{
    is_origin_allowed, is_remote_addr_allowed, validate_origin_headers, RejectedOriginTracker,
    MAX_REJECTED_ORIGINS,
};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

const BOUND_PORT: u16 = 8765;

#[test]
fn test_obs_cef_origin_allowed() {
    assert!(is_origin_allowed(Some("http://absolute"), BOUND_PORT, &[]));
}

#[test]
fn test_loopback_origins_allowed_on_bound_port_only() {
    let allowed = [
        "http://localhost:8765",
        "http://127.0.0.1:8765",
        "http://[::1]:8765",
    ];

    for origin in allowed {
        assert!(
            is_origin_allowed(Some(origin), BOUND_PORT, &[]),
            "Origin {} should be allowed",
            origin
        );
        assert!(
            validate_origin_headers(&[origin], BOUND_PORT, &[]).is_ok(),
            "validate_origin_headers should allow {}",
            origin
        );
    }
}

#[test]
fn test_loopback_origins_on_other_ports_rejected() {
    let rejected = [
        "http://localhost:1420",
        "http://localhost",
        "http://127.0.0.1:5173",
        "https://localhost:3000",
        "http://[::1]:8080",
        "https://127.0.0.1",
        "http://localhost:8765:1",
    ];

    for origin in rejected {
        assert!(
            !is_origin_allowed(Some(origin), BOUND_PORT, &[]),
            "Loopback origin {} on another port must be rejected",
            origin
        );
        assert!(validate_origin_headers(&[origin], BOUND_PORT, &[]).is_err());
    }
}

#[test]
fn test_loopback_origins_in_allowlist_accepted() {
    let allowed_ports = [1420, 5173];

    assert!(is_origin_allowed(Some("http://localhost:1420"), BOUND_PORT, &allowed_ports));
    assert!(is_origin_allowed(Some("http://127.0.0.1:5173"), BOUND_PORT, &allowed_ports));
    assert!(is_origin_allowed(Some("http://[::1]:1420"), BOUND_PORT, &allowed_ports));

    assert!(validate_origin_headers(&["http://localhost:1420"], BOUND_PORT, &allowed_ports).is_ok());
    assert!(validate_origin_headers(&["http://127.0.0.1:5173"], BOUND_PORT, &allowed_ports).is_ok());

    // Unlisted port still rejected
    assert!(!is_origin_allowed(Some("http://localhost:3000"), BOUND_PORT, &allowed_ports));
    assert!(validate_origin_headers(&["http://localhost:3000"], BOUND_PORT, &allowed_ports).is_err());

    // Non-loopback host strictly rejected even if port matches allowlist
    assert!(!is_origin_allowed(Some("http://evil.com:1420"), BOUND_PORT, &allowed_ports));
    assert!(!is_origin_allowed(Some("http://192.168.1.50:5173"), BOUND_PORT, &allowed_ports));
    assert!(validate_origin_headers(&["http://evil.com:1420"], BOUND_PORT, &allowed_ports).is_err());
}

#[test]
fn test_allowlist_revoke_cycle() {
    let mut allowed_ports = vec![1420, 5173];
    assert!(is_origin_allowed(Some("http://localhost:5173"), BOUND_PORT, &allowed_ports));

    // Revoke 5173
    allowed_ports.retain(|&p| p != 5173);
    assert!(!is_origin_allowed(Some("http://localhost:5173"), BOUND_PORT, &allowed_ports));
    // 1420 still allowed
    assert!(is_origin_allowed(Some("http://localhost:1420"), BOUND_PORT, &allowed_ports));
}

#[test]
fn test_native_clients_empty_origin_allowed() {
    assert!(is_origin_allowed(None, BOUND_PORT, &[]));
    assert!(is_origin_allowed(Some(""), BOUND_PORT, &[]));
    assert!(is_origin_allowed(Some("   "), BOUND_PORT, &[]));
    assert!(validate_origin_headers(&[], BOUND_PORT, &[]).is_ok());
    assert!(validate_origin_headers(&[""], BOUND_PORT, &[]).is_ok());
}

#[test]
fn test_malicious_and_external_origins_rejected() {
    let rejected = [
        "https://microsoft.com",
        "http://localhost.evil.com",
        "https://localhost.evil.com",
        "http://notlocalhost",
        "http://127.0.0.1.evil.com",
        "http://localhost@evil.com",
        "https://attacker.org:8765",
        "http://google.com",
    ];

    for origin in rejected {
        assert!(
            !is_origin_allowed(Some(origin), BOUND_PORT, &[]),
            "Origin {} should be rejected",
            origin
        );
        assert!(
            validate_origin_headers(&[origin], BOUND_PORT, &[]).is_err(),
            "validate_origin_headers should reject {}",
            origin
        );
    }
}

#[test]
fn test_non_http_schemes_rejected() {
    let non_http = [
        "file://localhost",
        "ftp://localhost:21",
        "ws://localhost:1420",
        "wss://127.0.0.1",
    ];

    for origin in non_http {
        assert!(
            !is_origin_allowed(Some(origin), BOUND_PORT, &[]),
            "Non-http scheme {} must be rejected",
            origin
        );
        assert!(validate_origin_headers(&[origin], BOUND_PORT, &[]).is_err());
    }
}

#[test]
fn test_malformed_origin_rejected_without_panicking() {
    assert!(!is_origin_allowed(Some("http://[::1"), BOUND_PORT, &[]));
    assert!(!is_origin_allowed(Some("not-a-valid-url"), BOUND_PORT, &[]));
    assert!(!is_origin_allowed(Some("http://"), BOUND_PORT, &[]));
    assert!(!is_origin_allowed(Some("http://localhost:0"), BOUND_PORT, &[0]));
    assert!(!is_origin_allowed(Some("http://localhost:70000"), BOUND_PORT, &[]));
}

#[test]
fn test_multiple_origin_headers_rejected() {
    let multiple = ["http://localhost:1420", "http://absolute"];
    assert!(validate_origin_headers(&multiple, BOUND_PORT, &[]).is_err());

    let duplicates = ["http://localhost:1420", "http://localhost:1420"];
    assert!(validate_origin_headers(&duplicates, BOUND_PORT, &[]).is_err());
}

#[test]
fn test_remote_peer_addr_loopback_filter() {
    let local_v4 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 54321);
    let local_v6 = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 54321);
    let external_v4 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 54321);
    let external_v6 = SocketAddr::new(
        IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
        54321,
    );

    assert!(is_remote_addr_allowed(&local_v4));
    assert!(is_remote_addr_allowed(&local_v6));
    assert!(!is_remote_addr_allowed(&external_v4));
    assert!(!is_remote_addr_allowed(&external_v6));
}

#[test]
fn test_tauri_custom_protocol_origins_allowed() {
    for origin in [
        "http://tauri.localhost",
        "https://tauri.localhost",
        "tauri://localhost",
    ] {
        assert!(
            is_origin_allowed(Some(origin), BOUND_PORT, &[]),
            "{origin} should be allowed"
        );
        assert!(validate_origin_headers(&[origin], BOUND_PORT, &[]).is_ok());
    }
}

#[test]
fn test_tauri_lookalike_origins_rejected() {
    let rejected = [
        "http://tauri.localhost.evil.com",
        "https://tauri.localhost.evil.com",
        "http://evil.tauri.localhost",
        "http://tauri.localhost:1234",
        "http://tauri.localhost@evil.com",
        "http://other.localhost",
    ];
    for origin in rejected {
        assert!(
            !is_origin_allowed(Some(origin), BOUND_PORT, &[]),
            "{origin} should be rejected"
        );
        assert!(validate_origin_headers(&[origin], BOUND_PORT, &[]).is_err());
    }
}

#[test]
fn test_bounded_rejected_origins_tracker() {
    let mut tracker = RejectedOriginTracker::new();

    // 1. Initial empty state
    assert!(tracker.is_empty());
    assert_eq!(tracker.len(), 0);

    // 2. Record rejected attempts
    tracker.record("http://localhost:5173");
    tracker.record("http://localhost:5173");
    assert_eq!(tracker.len(), 1);
    let entries = tracker.snapshot();
    assert_eq!(entries[0].origin, "http://localhost:5173");
    assert_eq!(entries[0].port, Some(5173));
    assert_eq!(entries[0].count, 2);

    // 3. Foreign origin records without port
    tracker.record("https://attacker.com/ws");
    assert_eq!(tracker.len(), 2);
    let entries = tracker.snapshot();
    let foreign = entries.iter().find(|e| e.origin == "https://attacker.com/ws").unwrap();
    assert_eq!(foreign.port, None);
    assert_eq!(foreign.count, 1);

    // 4. Overfilling drops oldest entry
    for p in 2000..(2000 + MAX_REJECTED_ORIGINS as u16 + 5) {
        tracker.record(&format!("http://localhost:{}", p));
    }
    assert_eq!(tracker.len(), MAX_REJECTED_ORIGINS);

    // 5. Remove by port
    tracker.record("http://localhost:9999");
    assert!(tracker.snapshot().iter().any(|e| e.port == Some(9999)));
    tracker.remove_by_port(9999);
    assert!(!tracker.snapshot().iter().any(|e| e.port == Some(9999)));
}
