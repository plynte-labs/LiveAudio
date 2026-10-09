// SPDX-License-Identifier: MIT

use std::collections::HashMap;
use std::net::TcpListener as StdTcpListener;
use std::time::Duration;

use futures_util::StreamExt;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use liveaudio_network::protocol::{HelloMessage, SubtitleWireMessage};
use liveaudio_network::replay::BacklogPolicy;
use liveaudio_network::server::{WsServer, WsServerConfig};
use liveaudio_network::traits::NetworkServer;

#[tokio::test]
async fn test_server_bind_free_port() {
    let mut server = WsServer::default();
    // Use an ephemeral high base port
    let bound_port = server.start(19870).await.expect("Failed to start server");
    assert_eq!(bound_port, 19870);
    assert_eq!(server.bound_port(), Some(19870));
    assert_eq!(server.client_count(), 0);

    server.stop().await.expect("Failed to stop server");
    assert_eq!(server.bound_port(), None);
}

#[tokio::test]
async fn test_server_port_fallback_when_base_is_busy() {
    let base_port = 19880;
    // Hold base_port with a standard listener
    let blocker = StdTcpListener::bind(("127.0.0.1", base_port)).expect("Bind blocker");

    let mut server = WsServer::default();
    let effective = server
        .start(base_port)
        .await
        .expect("Fallback should succeed");

    // Must bind next candidate in range
    assert_eq!(effective, base_port + 1);
    assert_eq!(server.bound_port(), Some(base_port + 1));

    drop(blocker);
    server.stop().await.expect("Stop server");
}

#[tokio::test]
async fn test_server_port_exhaustion() {
    let base_port = 19890;
    let mut blockers = Vec::new();
    for p in base_port..(base_port + 10) {
        let b = StdTcpListener::bind(("127.0.0.1", p)).expect("Bind blocker");
        blockers.push(b);
    }

    let mut server = WsServer::default();
    let err = server.start(base_port).await;
    assert!(err.is_err());
    match err {
        Err(liveaudio_network::NetworkError::PortExhaustion { base, top }) => {
            assert_eq!(base, base_port);
            assert_eq!(top, base_port + 9);
        }
        other => panic!("Expected PortExhaustion, got {:?}", other),
    }

    drop(blockers);
}

#[tokio::test]
async fn test_hello_frame_and_subtitle_broadcast() {
    let mut server = WsServer::default();
    let port = server.start(19900).await.expect("Start server");

    let url = format!("ws://127.0.0.1:{}", port);
    let (ws_stream, _resp) = connect_async(&url).await.expect("Client connect");
    let (mut _write, mut read) = ws_stream.split();

    // 1. Frame #1 must be the hello frame
    let first_msg = tokio::time::timeout(Duration::from_secs(2), read.next())
        .await
        .expect("Timeout waiting for hello")
        .expect("Stream closed")
        .expect("WebSocket error");

    let text = match first_msg {
        Message::Text(t) => t,
        other => panic!("Expected text message, got {:?}", other),
    };

    let hello: HelloMessage = serde_json::from_str(&text).expect("Parse hello");
    assert_eq!(hello.message_type, "hello");
    assert_eq!(hello.app, "liveaudio");
    assert_eq!(hello.proto, 1);
    assert_eq!(hello.port, port);

    // Give server time to register client
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(server.client_count(), 1);

    // 2. Broadcast a live subtitle message
    let sub = SubtitleWireMessage::new(
        "sub-1",
        "Hello from test broadcast",
        "default",
        100.0,
        101.0,
        0.1,
        1.1,
        1.0,
        false,
        0.0,
    );
    server.broadcast_subtitle(&sub).expect("Broadcast subtitle");

    let sub_msg = tokio::time::timeout(Duration::from_secs(2), read.next())
        .await
        .expect("Timeout waiting for subtitle")
        .expect("Stream closed")
        .expect("WebSocket error");

    let sub_text = match sub_msg {
        Message::Text(t) => t,
        other => panic!("Expected text subtitle, got {:?}", other),
    };

    let received_sub: SubtitleWireMessage = serde_json::from_str(&sub_text).expect("Parse sub");
    assert_eq!(received_sub, sub);

    // 3. Broadcast theme update
    let mut tokens = HashMap::new();
    tokens.insert("--sub-color".to_string(), "#ff0000".to_string());
    server.broadcast_theme(tokens).expect("Broadcast theme");

    let theme_msg = tokio::time::timeout(Duration::from_secs(2), read.next())
        .await
        .expect("Timeout waiting for theme")
        .expect("Stream closed")
        .expect("WebSocket error");

    let theme_text = match theme_msg {
        Message::Text(t) => t,
        other => panic!("Expected text theme, got {:?}", other),
    };
    let theme_val: serde_json::Value = serde_json::from_str(&theme_text).expect("Parse theme");
    assert_eq!(theme_val["type"], "theme");
    assert_eq!(theme_val["tokens"]["--sub-color"], "#ff0000");

    drop(read);
    server.stop().await.expect("Stop server");
}

#[tokio::test]
async fn test_handshake_origin_rejection_and_acceptance() {
    let mut server = WsServer::default();
    let port = server.start(19910).await.expect("Start server");
    let ws_url = format!("ws://127.0.0.1:{}", port);

    // 1. Foreign origin must be rejected with HTTP 403
    let mut bad_req = ws_url.clone().into_client_request().unwrap();
    bad_req
        .headers_mut()
        .insert("Origin", "https://malicious.com".parse().unwrap());
    let bad_conn = connect_async(bad_req).await;
    assert!(bad_conn.is_err(), "Foreign origin must be rejected");

    // 2. OBS CEF origin (http://absolute) must be accepted
    let mut obs_req = ws_url.clone().into_client_request().unwrap();
    obs_req
        .headers_mut()
        .insert("Origin", "http://absolute".parse().unwrap());
    let obs_conn = connect_async(obs_req).await;
    assert!(obs_conn.is_ok(), "http://absolute must be accepted");

    // 3. Localhost integration origin (http://localhost:1420) must be accepted
    let mut local_req = ws_url.clone().into_client_request().unwrap();
    local_req
        .headers_mut()
        .insert("Origin", "http://localhost:1420".parse().unwrap());
    let local_conn = connect_async(local_req).await;
    assert!(local_conn.is_ok(), "http://localhost:1420 must be accepted");

    // Metrics check
    assert!(
        server
            .metrics()
            .rejected_clients
            .load(std::sync::atomic::Ordering::SeqCst)
            >= 1
    );

    server.stop().await.expect("Stop server");
}

#[tokio::test]
async fn test_late_joining_client_replay() {
    let mut config = WsServerConfig::default();
    config.backlog_policy = BacklogPolicy::SendAll;
    let mut server = WsServer::new(config);
    let port = server.start(19920).await.expect("Start server");

    // Broadcast messages before any client connects
    let sub1 = SubtitleWireMessage::new(
        "msg-1",
        "First cue",
        "default",
        10.0,
        11.0,
        0.1,
        1.0,
        0.9,
        true,
        1.5,
    );
    let sub2 = SubtitleWireMessage::new(
        "msg-2",
        "Second cue",
        "default",
        12.0,
        13.0,
        0.1,
        1.0,
        0.9,
        true,
        1.5,
    );
    server.broadcast_subtitle(&sub1).unwrap();
    server.broadcast_subtitle(&sub2).unwrap();

    // Late joining client connects
    let url = format!("ws://127.0.0.1:{}", port);
    let (ws_stream, _) = connect_async(&url).await.expect("Late client connect");
    let (_write, mut read) = ws_stream.split();

    // 1. Hello frame
    let msg1 = read.next().await.unwrap().unwrap();
    let hello: HelloMessage = serde_json::from_str(&msg1.to_text().unwrap()).unwrap();
    assert_eq!(hello.message_type, "hello");

    // 2. Replayed cue 1
    let msg2 = read.next().await.unwrap().unwrap();
    let replayed1: SubtitleWireMessage = serde_json::from_str(&msg2.to_text().unwrap()).unwrap();
    assert_eq!(replayed1.id, "msg-1");

    // 3. Replayed cue 2
    let msg3 = read.next().await.unwrap().unwrap();
    let replayed2: SubtitleWireMessage = serde_json::from_str(&msg3.to_text().unwrap()).unwrap();
    assert_eq!(replayed2.id, "msg-2");

    server.stop().await.expect("Stop server");
}

#[tokio::test]
async fn test_late_joining_client_replay_is_paced() {
    let mut config = WsServerConfig::default();
    config.backlog_policy = BacklogPolicy::SendAll;
    let mut server = WsServer::new(config);
    let port = server.start(19950).await.expect("Start server");

    let first = SubtitleWireMessage::new(
        "paced-1", "one", "default", 10.0, 11.0, 1.1, 1.2, 0.1, true, 0.15,
    );
    let second = SubtitleWireMessage::new(
        "paced-2", "two", "default", 12.0, 13.0, 1.1, 1.2, 0.1, true, 0.15,
    );
    server
        .broadcast_subtitle(&first)
        .expect("Queue first replay cue");
    server
        .broadcast_subtitle(&second)
        .expect("Queue second replay cue");

    let (stream, _) = connect_async(format!("ws://127.0.0.1:{port}"))
        .await
        .expect("Connect client");
    let (_write, mut read) = stream.split();
    let _hello = tokio::time::timeout(Duration::from_secs(2), read.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let first_frame = tokio::time::timeout(Duration::from_secs(2), read.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<SubtitleWireMessage>(&first_frame.to_text().unwrap())
            .unwrap()
            .id,
        "paced-1"
    );

    let first_received = tokio::time::Instant::now();
    let second_frame = tokio::time::timeout(Duration::from_secs(2), read.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let spacing = first_received.elapsed();
    assert_eq!(
        serde_json::from_str::<SubtitleWireMessage>(&second_frame.to_text().unwrap())
            .unwrap()
            .id,
        "paced-2"
    );
    assert!(
        spacing >= Duration::from_millis(100),
        "catch-up spacing was only {spacing:?}"
    );

    server.stop().await.expect("Stop server");
}

#[tokio::test]
async fn test_reconnecting_client_gets_send_all_replay_without_catchup_interval() {
    let mut config = WsServerConfig::default();
    config.backlog_policy = BacklogPolicy::SendAll;
    let mut server = WsServer::new(config);
    let port = server.start(19970).await.expect("Start server");
    let message = SubtitleWireMessage::new(
        "send-all", "retained", "default", 1.0, 2.0, 2.0, 2.1, 0.1, true, 0.0,
    );
    let (initial_stream, _) = connect_async(format!("ws://127.0.0.1:{port}"))
        .await
        .expect("Initial client connect");
    let (initial_write, mut initial_read) = initial_stream.split();
    let _hello = tokio::time::timeout(Duration::from_secs(2), initial_read.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    server
        .broadcast_subtitle(&message)
        .expect("Broadcast send_all replay");
    let initial_frame = tokio::time::timeout(Duration::from_secs(2), initial_read.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<SubtitleWireMessage>(&initial_frame.to_text().unwrap())
            .unwrap()
            .id,
        "send-all"
    );
    drop(initial_read);
    drop(initial_write);
    tokio::time::timeout(Duration::from_secs(2), async {
        while server.client_count() != 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("initial client must be disconnected before reconnect");

    let (stream, _) = connect_async(format!("ws://127.0.0.1:{port}"))
        .await
        .expect("Reconnect client");
    let (_write, mut read) = stream.split();
    let _hello = tokio::time::timeout(Duration::from_secs(2), read.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let frame = tokio::time::timeout(Duration::from_secs(2), read.next()).await;
    assert!(
        frame.is_ok(),
        "send_all replay should be retained for reconnects"
    );
    let replayed: SubtitleWireMessage =
        serde_json::from_str(&frame.unwrap().unwrap().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(replayed.id, "send-all");
    server.stop().await.expect("Stop server");
}

#[tokio::test]
async fn test_connected_client_receives_catchup_at_interval_and_stop_cancels_pending() {
    let mut config = WsServerConfig::default();
    config.backlog_policy = BacklogPolicy::SendAll;
    let mut server = WsServer::new(config);
    let port = server.start(19960).await.expect("Start server");
    let (stream, _) = connect_async(format!("ws://127.0.0.1:{port}"))
        .await
        .expect("Connect client");
    let (_write, mut read) = stream.split();
    let _hello = tokio::time::timeout(Duration::from_secs(2), read.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    for (id, text) in [
        ("live-paced-1", "one"),
        ("live-paced-2", "two"),
        ("live-paced-3", "three"),
    ] {
        let msg =
            SubtitleWireMessage::new(id, text, "default", 10.0, 11.0, 1.1, 1.2, 0.1, true, 0.2);
        server.broadcast_subtitle(&msg).expect("Queue replay cue");
    }

    let first_frame = tokio::time::timeout(Duration::from_secs(2), read.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<SubtitleWireMessage>(&first_frame.to_text().unwrap())
            .unwrap()
            .id,
        "live-paced-1"
    );
    let second_started = tokio::time::Instant::now();
    let second_frame = tokio::time::timeout(Duration::from_secs(2), read.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<SubtitleWireMessage>(&second_frame.to_text().unwrap())
            .unwrap()
            .id,
        "live-paced-2"
    );
    assert!(
        second_started.elapsed() >= Duration::from_millis(100),
        "connected-client catch-up was not paced: {:?}",
        second_started.elapsed()
    );
    server.stop().await.expect("Stop server");
    assert_eq!(
        server.pending_replay_len(),
        0,
        "shutdown clears queued replay"
    );

    let next = tokio::time::timeout(Duration::from_secs(1), read.next())
        .await
        .unwrap();
    if let Some(Ok(Message::Text(text))) = next {
        let message: SubtitleWireMessage = serde_json::from_str(&text).expect("Parse frame");
        assert_ne!(
            message.id, "live-paced-3",
            "shutdown must cancel pending catch-up"
        );
    }
}

#[tokio::test]
async fn test_health_file_atomic_write() {
    let tmp_dir =
        std::env::temp_dir().join(format!("liveaudio_test_health_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp_dir);
    let health_path = tmp_dir.join("network_health.json");

    let mut config = WsServerConfig::default();
    config.health_file_path = Some(health_path.clone());
    let mut server = WsServer::new(config);

    let port = server.start(19930).await.expect("Start server");
    assert!(health_path.exists(), "Health file must be written on start");

    let content = std::fs::read_to_string(&health_path).expect("Read health file");
    let json: serde_json::Value = serde_json::from_str(&content).expect("Parse health JSON");
    assert_eq!(json["bound_port"], port);
    assert_eq!(json["client_count"], 0);

    server.stop().await.expect("Stop server");
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[tokio::test]
async fn test_sink_adapter_bridges_cue_to_ws_client() {
    use liveaudio_core::asr::AsrTranscriptionResponse;
    use liveaudio_core::{TranscriptionCue, TranscriptionSink};
    use liveaudio_network::sink_adapter::WebSocketTranscriptionSink;
    use std::sync::Arc;

    let server = Arc::new(WsServer::default());
    let mut server_mut = (*server).clone();
    let port = server_mut.start(19940).await.expect("Start server");

    let sink = WebSocketTranscriptionSink::from_server(Arc::clone(&server));

    let url = format!("ws://127.0.0.1:{}", port);
    let (ws_stream, _) = connect_async(&url).await.expect("Connect client");
    let (_write, mut read) = ws_stream.split();

    // Consume hello frame
    let _hello = read.next().await.unwrap().unwrap();

    let dummy_asr = AsrTranscriptionResponse {
        request_id: "req-sink".to_string(),
        text: "Sink pipeline test".to_string(),
        language: "es".to_string(),
        duration_sec: 1.5,
        latency_sec: 0.2,
        segments: vec![],
    };
    let cue = TranscriptionCue::new(10, 50.0, 51.5, "Sink pipeline test".to_string(), dummy_asr);

    sink.emit(&cue).await.expect("Emit cue to sink");
    let stats = sink.stats();
    assert_eq!(stats.saved, 1);
    assert_eq!(stats.failed, 0);

    let sub_frame = read.next().await.unwrap().unwrap();
    let sub: SubtitleWireMessage = serde_json::from_str(&sub_frame.to_text().unwrap()).unwrap();
    assert_eq!(sub.text, "Sink pipeline test");

    server_mut.stop().await.expect("Stop server");
}
