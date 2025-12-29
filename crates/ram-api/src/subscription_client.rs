//! Subscription client for cross-node VBAN subscription requests.
//!
//! This module handles outgoing subscription requests to remote nodes
//! when a cross-node route is created.

use std::net::SocketAddr;
use std::time::Duration;

use crate::models::{NodeInfo, SubscriptionRequest, SubscriptionResponse};

/// Client for sending subscription requests to remote nodes.
#[derive(Clone)]
pub struct SubscriptionClient {
    client: reqwest::Client,
    local_vban_port: u16,
}

impl SubscriptionClient {
    /// Creates a new subscription client.
    pub fn new(local_vban_port: u16) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_default();

        Self {
            client,
            local_vban_port,
        }
    }

    /// Sends a subscription request to a remote source node.
    ///
    /// This is called when we (the destination) want to receive audio
    /// from a remote source node via VBAN.
    ///
    /// # Arguments
    ///
    /// * `source_node` - The source node info (must have address and API port)
    /// * `source_device` - Device ID on the source node
    /// * `source_channels` - Channel numbers to subscribe to (1-based)
    /// * `local_addr` - Our local address that the source should send VBAN to
    /// * `sample_rate` - Requested sample rate
    ///
    /// # Returns
    ///
    /// The subscription response from the source node.
    pub async fn subscribe(
        &self,
        source_node: &NodeInfo,
        source_device: &str,
        source_channels: Vec<u16>,
        local_addr: SocketAddr,
        sample_rate: u32,
    ) -> Result<SubscriptionResponse, String> {
        // Get source node API address
        let source_addr = source_node
            .addresses
            .first()
            .ok_or_else(|| format!("No address for source node: {}", source_node.name))?;

        let api_url = format!(
            "http://{}:{}/api/v1/subscriptions",
            source_addr, source_node.api_port
        );

        // Generate a unique stream name
        let stream_name = format!(
            "AM_{}_{}",
            source_node.name.chars().take(8).collect::<String>(),
            &uuid::Uuid::new_v4().to_string()[..8]
        );

        let request = SubscriptionRequest {
            stream_name,
            source_device: source_device.to_string(),
            source_channels,
            destination_node: local_addr.ip().to_string(),
            destination_addr: format!("{}:{}", local_addr.ip(), self.local_vban_port),
            sample_rate,
        };

        tracing::info!(
            "Sending subscription request to {}: stream='{}' device='{}' channels={:?}",
            api_url,
            request.stream_name,
            request.source_device,
            request.source_channels
        );

        let response = self
            .client
            .post(&api_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Failed to send subscription request: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "no body".to_string());
            return Err(format!("Subscription request failed: {} - {}", status, body));
        }

        let sub_response: SubscriptionResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse subscription response: {e}"))?;

        if sub_response.success {
            tracing::info!(
                "Subscription accepted: id={:?} stream='{}'",
                sub_response.subscription_id,
                sub_response.vban_stream_name.as_deref().unwrap_or("?")
            );
        } else {
            tracing::warn!(
                "Subscription rejected: {}",
                sub_response.error.as_deref().unwrap_or("unknown error")
            );
        }

        Ok(sub_response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscription_client_creation() {
        let client = SubscriptionClient::new(6980);
        assert_eq!(client.local_vban_port, 6980);
    }
}
