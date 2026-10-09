// SPDX-License-Identifier: MIT

use liveaudio_network::security::{
    is_origin_allowed, is_remote_addr_allowed, validate_origin_headers,
};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

#[test]
fn test_obs_cef_origin_allowed() {
    assert!(is_origin_allowed(Some("http://absolute")));
}

#[test]
fn test_loopback_origins_allowed_on_any_port() {
    let allowed = [
        "http://localhost:1420",
        "http://localhost",
        "http://127.0.0.1:5173",
        "https://localhost:3000",
        "http://[::1]:8080",
        "https://127.0.0.1",
        "http://127.0.0.1:8765",
    ];

    for origin in allowed {
        assert!(
            is_origin_allowed(Some(origin)),
            "Origin {} should be allowed",
            origin
        );
        assert!(
            validate_origin_headers(&[origin]).is_ok(),
            "validate_origin_headers should allow {}",
            origin
        );
    }
}

#[test]
fn test_native_clients_empty_origin_allowed() {
    assert!(is_origin_allowed(None));
    assert!(is_origin_allowed(Some("")));
    assert!(is_origin_allowed(Some("   ")));
    assert!(validate_origin_headers(&[]).is_ok());
    assert!(validate_origin_headers(&[""]).is_ok());
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
            !is_origin_allowed(Some(origin)),
            "Origin {} should be rejected",
            origin
        );
        assert!(
            validate_origin_headers(&[origin]).is_err(),
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
            !is_origin_allowed(Some(origin)),
            "Non-http scheme {} must be rejected",
            origin
        );
        assert!(validate_origin_headers(&[origin]).is_err());
    }
}

#[test]
fn test_malformed_origin_rejected_without_panicking() {
    assert!(!is_origin_allowed(Some("http://[::1")));
    assert!(!is_origin_allowed(Some("not-a-valid-url")));
    assert!(!is_origin_allowed(Some("http://")));
}

#[test]
fn test_multiple_origin_headers_rejected() {
    let multiple = ["http://localhost:1420", "http://absolute"];
    assert!(validate_origin_headers(&multiple).is_err());

    let duplicates = ["http://localhost:1420", "http://localhost:1420"];
    assert!(validate_origin_headers(&duplicates).is_err());
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
