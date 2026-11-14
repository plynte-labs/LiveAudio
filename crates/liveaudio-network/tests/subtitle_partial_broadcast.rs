// SPDX-License-Identifier: MIT

use futures_util::StreamExt;
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
async fn subtitle_partials_are_textless_live_only_and_bypass_retry_and_replay() {
    let mut server = WsServer::default();
    let port = server.start(19_950).await.expect("server starts");
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

    server
        .broadcast_subtitle_partial("utt_3", "hola que")
        .expect("partial publishes");
    let parsed: serde_json::Value =
        serde_json::from_str(&next_text(&mut first_read).await).unwrap();
    assert_eq!(parsed["type"], "subtitle_partial");
    assert_eq!(parsed["utterance_id"], "utt_3");
    assert_eq!(parsed["partial_text"], "hola que");
    assert!(parsed.get("text").is_none(), "old overlays key on `text`");

    // Not replayed to a later connection.
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
            .broadcast_subtitle_partial("utt_3", "hola que tal")
            .expect("partial publishes");
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
