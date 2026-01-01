//! Health endpoint E2E tests.

use serde::Deserialize;

use crate::e2e::TestClient;

/// Health check response.
#[derive(Debug, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Test that the health endpoint returns correct status.
#[tokio::test]
async fn test_health_returns_ok() {
    let client = TestClient::new();

    let response = client.get("/health").await.expect("Failed to send request");

    assert!(
        response.status().is_success(),
        "Health check failed with status: {}",
        response.status()
    );

    let health: HealthResponse = response.json().await.expect("Failed to parse response");
    assert_eq!(health.status, "ok");
    assert!(!health.version.is_empty());
}

/// Test that the health endpoint returns valid version format.
#[tokio::test]
async fn test_health_version_format() {
    let client = TestClient::new();

    let health: HealthResponse = client
        .get_json("/health")
        .await
        .expect("Failed to get health");

    // Version should be semver format: X.Y.Z or X.Y.Z-dev.N
    let parts: Vec<&str> = health.version.split('.').collect();
    assert!(
        parts.len() >= 3,
        "Version '{}' should have at least 3 parts",
        health.version
    );
}
