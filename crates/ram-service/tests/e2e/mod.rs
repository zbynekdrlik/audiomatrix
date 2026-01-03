#![allow(clippy::doc_markdown)]
//! E2E Tests for AudioMatrix Service.
//!
//! These tests verify the full system behavior by running against
//! a real AudioMatrix service instance.
//!
//! # Running E2E Tests
//!
//! ```bash
//! # Run all E2E tests (requires running service on localhost:8080)
//! cargo test -p ram-service -- --ignored
//! ```

mod api;
mod websocket;

use reqwest::Client;
use std::time::Duration;

/// Test server client for E2E tests.
pub struct TestClient {
    /// HTTP client.
    pub client: Client,
    /// Base URL of the server.
    pub base_url: String,
}

impl TestClient {
    /// Creates a new test client.
    ///
    /// Uses `TEST_SERVER_URL` environment variable if set,
    /// otherwise defaults to localhost:8080.
    pub fn new() -> Self {
        let base_url = std::env::var("TEST_SERVER_URL")
            .map(|url| {
                if url.starts_with("http://") || url.starts_with("https://") {
                    url
                } else {
                    format!("http://{url}")
                }
            })
            .unwrap_or_else(|_| "http://localhost:8080".to_string());
        Self::with_url(&base_url)
    }

    /// Creates a new test client with a custom URL.
    pub fn with_url(base_url: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            base_url: base_url.to_string(),
        }
    }

    /// Returns the API URL for a path.
    pub fn api_url(&self, path: &str) -> String {
        format!("{}/api/v1{}", self.base_url, path)
    }

    /// Returns the WebSocket URL.
    pub fn ws_url(&self) -> String {
        format!("{}/api/v1/ws", self.base_url.replace("http", "ws"))
    }

    /// Makes a GET request and returns the response.
    pub async fn get(&self, path: &str) -> Result<reqwest::Response, reqwest::Error> {
        self.client.get(self.api_url(path)).send().await
    }

    /// Makes a GET request and returns JSON.
    pub async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<T, reqwest::Error> {
        self.client
            .get(self.api_url(path))
            .send()
            .await?
            .json()
            .await
    }

    /// Makes a POST request with JSON body.
    pub async fn post_json<B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<reqwest::Response, reqwest::Error> {
        self.client.post(self.api_url(path)).json(body).send().await
    }

    /// Makes a PUT request with JSON body.
    #[allow(dead_code)]
    pub async fn put_json<B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<reqwest::Response, reqwest::Error> {
        self.client.put(self.api_url(path)).json(body).send().await
    }

    /// Makes a DELETE request.
    pub async fn delete(&self, path: &str) -> Result<reqwest::Response, reqwest::Error> {
        self.client.delete(self.api_url(path)).send().await
    }
}

impl Default for TestClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_url() {
        let client = TestClient::new();
        assert_eq!(
            client.api_url("/health"),
            "http://localhost:8080/api/v1/health"
        );
    }

    #[test]
    fn test_ws_url() {
        let client = TestClient::new();
        assert_eq!(client.ws_url(), "ws://localhost:8080/api/v1/ws");
    }
}
