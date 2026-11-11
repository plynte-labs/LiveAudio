// SPDX-License-Identifier: MIT

use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use liveaudio_network::protocol::HelloMessage;
use liveaudio_network::server::WsServer;
use liveaudio_network::traits::NetworkServer;

async fn next_frame<S>(read: &mut S) -> Message
where
    S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    tokio::time::timeout(Duration::from_secs(2), read.next())
        .await
        .expect("timed out waiting for frame")
        .expect("stream closed")
        .expect("websocket error")
}

fn tokens(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[tokio::test]
async fn late_joining_client_receives_the_stored_theme_right_after_hello() {
    let mut server = WsServer::default();
    let port = server.start(19940).await.expect("Start server");

    server
        .broadcast_theme(tokens(&[("--sub-color", "#ff0000")]))
        .expect("Store theme with no clients connected");

    let (ws, _) = connect_async(format!("ws://127.0.0.1:{port}"))
        .await
        .expect("Late client connects");
    let (_write, mut read) = ws.split();

    let hello_text = next_frame(&mut read).await;
    let hello: HelloMessage = serde_json::from_str(hello_text.to_text().unwrap()).unwrap();
    assert_eq!(hello.message_type, "hello");

    let theme_text = next_frame(&mut read).await;
    let theme: serde_json::Value = serde_json::from_str(theme_text.to_text().unwrap()).unwrap();
    assert_eq!(theme["type"], "theme");
    assert_eq!(theme["tokens"]["--sub-color"], "#ff0000");

    server.stop().await.expect("Stop server");
}

#[tokio::test]
async fn late_joiner_gets_no_theme_when_none_was_ever_set() {
    let mut server = WsServer::default();
    let port = server.start(19941).await.expect("Start server");

    let (ws, _) = connect_async(format!("ws://127.0.0.1:{port}"))
        .await
        .expect("Client connects");
    let (_write, mut read) = ws.split();

    let hello_text = next_frame(&mut read).await;
    assert!(hello_text.to_text().unwrap().contains("\"hello\""));

    let next = tokio::time::timeout(Duration::from_millis(300), read.next()).await;
    assert!(next.is_err(), "unexpected frame without a stored theme");

    server.stop().await.expect("Stop server");
}

#[tokio::test]
async fn late_joiner_gets_the_latest_theme_not_an_earlier_one() {
    let mut server = WsServer::default();
    let port = server.start(19942).await.expect("Start server");

    server
        .broadcast_theme(tokens(&[("--sub-color", "#111111")]))
        .unwrap();
    server
        .broadcast_theme(tokens(&[("--sub-color", "#222222")]))
        .unwrap();

    let (ws, _) = connect_async(format!("ws://127.0.0.1:{port}"))
        .await
        .expect("Late client connects");
    let (_write, mut read) = ws.split();
    let _hello = next_frame(&mut read).await;

    let theme_text = next_frame(&mut read).await;
    let theme: serde_json::Value = serde_json::from_str(theme_text.to_text().unwrap()).unwrap();
    assert_eq!(theme["tokens"]["--sub-color"], "#222222");

    server.stop().await.expect("Stop server");
}

#[tokio::test]
async fn theme_precedes_replayed_subtitles_for_a_late_joiner() {
    use liveaudio_network::protocol::SubtitleWireMessage;

    let mut server = WsServer::default();
    let port = server.start(19943).await.expect("Start server");
    server
        .broadcast_theme(tokens(&[("--sub-color", "#ff0000")]))
        .expect("Store theme");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let cue = SubtitleWireMessage::new(
        "cue-1", "hola", "default", now, now, 0.0, 0.0, 0.0, false, 0.0,
    );
    server.broadcast_subtitle(&cue).expect("Buffer subtitle");

    let (ws, _) = connect_async(format!("ws://127.0.0.1:{port}"))
        .await
        .expect("Late client connects");
    let (_write, mut read) = ws.split();

    let _hello = next_frame(&mut read).await;
    let theme_text = next_frame(&mut read).await;
    let theme: serde_json::Value = serde_json::from_str(theme_text.to_text().unwrap()).unwrap();
    assert_eq!(
        theme["type"], "theme",
        "the theme must come before any replayed subtitle"
    );
    let cue_text = next_frame(&mut read).await;
    let replayed: serde_json::Value = serde_json::from_str(cue_text.to_text().unwrap()).unwrap();
    assert_eq!(replayed["text"], "hola");

    server.stop().await.expect("Stop server");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn late_joiner_never_keeps_an_older_theme_than_the_last_one_stored() {
    use std::sync::Arc;

    let mut server = WsServer::default();
    let port = server.start(19944).await.expect("Start server");
    let server = Arc::new(server);
    for round in 0..25u32 {
        let writer = {
            let server = Arc::clone(&server);
            tokio::spawn(async move {
                for i in 0..50u32 {
                    let color = format!("#{:06x}", round * 100 + i);
                    server
                        .broadcast_theme(tokens(&[("--sub-color", &color)]))
                        .expect("Store theme");
                    tokio::task::yield_now().await;
                }
            })
        };
        let (ws, _) = connect_async(format!("ws://127.0.0.1:{port}"))
            .await
            .expect("Late client connects");
        let (_write, mut read) = ws.split();
        writer.await.expect("Writer finished");

        let mut last_theme = None;
        while let Ok(Some(Ok(frame))) =
            tokio::time::timeout(Duration::from_millis(300), read.next()).await
        {
            let value: serde_json::Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
            if value["type"] == "theme" {
                last_theme = Some(value["tokens"]["--sub-color"].as_str().unwrap().to_string());
            }
        }
        let expected = format!("#{:06x}", round * 100 + 49);
        assert_eq!(
            last_theme.as_deref(),
            Some(expected.as_str()),
            "round {round}"
        );
    }

    Arc::try_unwrap(server)
        .ok()
        .expect("No other handles")
        .stop()
        .await
        .expect("Stop server");
}
