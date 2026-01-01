#![allow(clippy::doc_markdown)]
//! Stream endpoint E2E tests.

use crate::e2e::TestClient;

/// Test listing active streams.
#[tokio::test]
async fn test_list_streams() {
    let client = TestClient::new();

    let response = client
        .get("/streams")
        .await
        .expect("Failed to list streams");

    assert!(
        response.status().is_success(),
        "List streams failed with status: {}",
        response.status()
    );

    // Streams endpoint returns array (may be empty if no active streams)
    let _streams: Vec<serde_json::Value> = response.json().await.expect("Failed to parse streams");
}

/// Test getting stream by ID returns 404 for nonexistent stream.
#[tokio::test]
async fn test_get_nonexistent_stream_returns_404() {
    let client = TestClient::new();

    let response = client
        .get("/streams/nonexistent-stream-12345")
        .await
        .expect("Failed to get stream");

    assert_eq!(
        response.status().as_u16(),
        404,
        "Nonexistent stream should return 404"
    );
}

/// Test stream statistics endpoint.
#[tokio::test]
async fn test_get_stream_stats() {
    let client = TestClient::new();

    // First list streams to find an active one
    let response = client
        .get("/streams")
        .await
        .expect("Failed to list streams");

    let streams: Vec<serde_json::Value> = response.json().await.expect("Failed to parse streams");

    if let Some(stream) = streams.first() {
        if let Some(stream_id) = stream.get("id").and_then(|v| v.as_str()) {
            let stats_response = client
                .get(&format!(
                    "/streams/{}/stats",
                    urlencoding::encode(stream_id)
                ))
                .await
                .expect("Failed to get stream stats");

            assert!(
                stats_response.status().is_success(),
                "Get stream stats failed with status: {}",
                stats_response.status()
            );
        }
    }
}
