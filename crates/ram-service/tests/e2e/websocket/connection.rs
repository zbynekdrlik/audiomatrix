#![allow(clippy::doc_markdown)]
//! WebSocket connection E2E tests.

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// WebSocket URL for tests.
fn ws_url() -> String {
    "ws://localhost:8080/api/v1/ws".to_string()
}

/// Test WebSocket connection can be established.
#[tokio::test]
#[ignore = "Requires running server"]
async fn test_websocket_connection() {
    let connect_result = timeout(Duration::from_secs(5), connect_async(&ws_url())).await;

    assert!(
        connect_result.is_ok(),
        "Connection should complete within timeout"
    );

    let (ws_stream, response) = connect_result.unwrap().expect("Failed to connect");
    assert_eq!(
        response.status().as_u16(),
        101,
        "Should receive 101 Switching Protocols"
    );

    // Clean close
    let (mut write, _read) = ws_stream.split();
    let _ = write.close().await;
}

/// Test WebSocket ping-pong keepalive.
#[tokio::test]
#[ignore = "Requires running server"]
async fn test_websocket_ping_pong() {
    let (ws_stream, _) = connect_async(&ws_url()).await.expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Send a ping
    write
        .send(Message::Ping(Bytes::from_static(&[1, 2, 3])))
        .await
        .expect("Failed to send ping");

    // Wait for pong response
    let pong_result = timeout(Duration::from_secs(5), async {
        while let Some(msg_result) = read.next().await {
            if let Ok(Message::Pong(data)) = msg_result {
                return Some(data);
            }
        }
        None
    })
    .await;

    assert!(pong_result.is_ok(), "Should receive pong within timeout");
    assert_eq!(
        pong_result.unwrap(),
        Some(Bytes::from_static(&[1, 2, 3])),
        "Pong should echo ping data"
    );

    let _ = write.close().await;
}

/// Test WebSocket handles invalid messages gracefully.
#[tokio::test]
#[ignore = "Requires running server"]
async fn test_websocket_invalid_message() {
    let (ws_stream, _) = connect_async(&ws_url()).await.expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Send invalid JSON
    write
        .send(Message::Text("not valid json {{{".into()))
        .await
        .expect("Failed to send message");

    // Should receive error response or stay connected (not crash)
    let response = timeout(Duration::from_secs(2), read.next()).await;

    // Either we get an error message back or connection stays open
    // Server should not crash on invalid input
    if let Ok(Some(Ok(msg))) = response {
        // If we get a response, it should be text (error message)
        assert!(
            matches!(msg, Message::Text(_)),
            "Response to invalid message should be text error"
        );
    }

    let _ = write.close().await;
}

/// Test WebSocket close is handled properly.
#[tokio::test]
#[ignore = "Requires running server"]
async fn test_websocket_close() {
    let (ws_stream, _) = connect_async(&ws_url()).await.expect("Failed to connect");

    let (mut write, _read) = ws_stream.split();

    // Send close frame
    let close_result = write.close().await;
    assert!(close_result.is_ok(), "Close should succeed");
}

/// Test multiple WebSocket connections can coexist.
#[tokio::test]
#[ignore = "Requires running server"]
async fn test_multiple_websocket_connections() {
    // Connect 3 clients
    let (ws1, _) = connect_async(&ws_url())
        .await
        .expect("Failed to connect client 1");
    let (ws2, _) = connect_async(&ws_url())
        .await
        .expect("Failed to connect client 2");
    let (ws3, _) = connect_async(&ws_url())
        .await
        .expect("Failed to connect client 3");

    let (mut write1, _) = ws1.split();
    let (mut write2, _) = ws2.split();
    let (mut write3, _) = ws3.split();

    // All should be able to send messages
    write1
        .send(Message::Ping(Bytes::from_static(&[1])))
        .await
        .expect("Client 1 should send");
    write2
        .send(Message::Ping(Bytes::from_static(&[2])))
        .await
        .expect("Client 2 should send");
    write3
        .send(Message::Ping(Bytes::from_static(&[3])))
        .await
        .expect("Client 3 should send");

    // Clean close all
    let _ = write1.close().await;
    let _ = write2.close().await;
    let _ = write3.close().await;
}
