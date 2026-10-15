// SPDX-License-Identifier: MIT

use std::net::TcpListener as StdTcpListener;

use liveaudio_network::server::{start_if_enabled, WsServer, WsServerConfig};
use liveaudio_network::traits::NetworkServer;

fn free_port() -> u16 {
    StdTcpListener::bind(("127.0.0.1", 0))
        .expect("ephemeral bind")
        .local_addr()
        .expect("local addr")
        .port()
}

#[tokio::test]
async fn disabled_obs_does_not_bind_the_listener() {
    let port = free_port();
    let mut server = WsServer::new(WsServerConfig {
        base_port: port,
        ..WsServerConfig::default()
    });

    let bound = start_if_enabled(&mut server, false, port)
        .await
        .expect("disabled start must not fail");

    assert_eq!(bound, None);
    assert_eq!(server.bound_port(), None);
    // Nothing holds the port, so an external bind still succeeds.
    StdTcpListener::bind(("127.0.0.1", port)).expect("port must stay free when obs is disabled");
}

#[tokio::test]
async fn enabled_obs_binds_the_configured_port() {
    let port = free_port();
    let mut server = WsServer::new(WsServerConfig {
        base_port: port,
        ..WsServerConfig::default()
    });

    let bound = start_if_enabled(&mut server, true, port)
        .await
        .expect("enabled start");

    assert_eq!(bound, Some(port));
    assert_eq!(server.bound_port(), Some(port));
    server.stop().await.expect("stop");
}
