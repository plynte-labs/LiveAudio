// SPDX-License-Identifier: MIT

use futures_util::StreamExt;
use liveaudio_core::acoustic_events::AcousticSpeechEvent;
use liveaudio_network::{NetworkServer, WsServer};
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};

async fn next_text(
    read: &mut (impl futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
              + Unpin),
) -> String {
    loop {
        match tokio::time::timeout(Duration::from_secs(2), read.next())
            .await
            .expect("timed out waiting for WebSocket frame")
            .expect("WebSocket closed")
            .expect("WebSocket receive failed")
        {
            Message::Text(text) => return text.to_string(),
            _ => continue,
        }
    }
}

#[tokio::test]
async fn acoustic_events_are_text_frames_live_only_and_bypass_retry_and_replay() {
    let mut server = WsServer::default();
    let port = server.start(19_900).await.expect("server starts");
    let url = format!("ws://127.0.0.1:{port}");

    let (first, _) = connect_async(&url).await.expect("first client connects");
    let (_, mut first_read) = first.split();
    let hello: serde_json::Value = serde_json::from_str(&next_text(&mut first_read).await).unwrap();
    assert_eq!(hello["type"], "hello");
    tokio::time::timeout(Duration::from_secs(2), async {
        while server.client_count() != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("first client registered");
    tokio::time::sleep(Duration::from_millis(20)).await;

    let start = AcousticSpeechEvent::SpeechActivityStart {
        event_version: 1,
        run_id: "run".into(),
        attempt_id: 1,
        utterance_id: "utterance".into(),
        seq: 1,
        monotonic_ms: 0,
    };
    server
        .broadcast_acoustic_event(&start)
        .expect("event publishes");
    let frame = next_text(&mut first_read).await;
    let parsed: serde_json::Value = serde_json::from_str(&frame).unwrap();
    assert_eq!(parsed["type"], "speech_activity_start");
    assert!(parsed.get("text").is_none());

    // The already-published start is not replayed to a later connection.
    let (second, _) = connect_async(&url).await.expect("second client connects");
    let (_, mut second_read) = second.split();
    let hello: serde_json::Value =
        serde_json::from_str(&next_text(&mut second_read).await).unwrap();
    assert_eq!(hello["type"], "hello");
    assert!(
        tokio::time::timeout(Duration::from_millis(150), second_read.next())
            .await
            .is_err()
    );

    for _ in 0..80 {
        server
            .broadcast_acoustic_event(&start)
            .expect("event publishes");
    }
    assert_eq!(
        server
            .metrics()
            .retry_buffer_size
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    assert_eq!(server.pending_replay_len(), 0);

    NetworkServer::stop(&mut server)
        .await
        .expect("server stops");
}
