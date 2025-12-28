//! Subscription protocol for cross-node audio routing.
//!
//! This module implements the destination-owned subscription model where:
//! - Destinations (receivers) subscribe to sources
//! - Subscriptions are stored at the destination node
//! - Sources send audio to all active subscribers
//!
//! # Protocol Flow
//!
//! ```text
//! Destination Node                    Source Node
//!       │                                  │
//!       │─────── SUBSCRIBE ───────────────►│
//!       │        (stream_name, channels)   │
//!       │                                  │
//!       │◄────── SUBSCRIBE_ACK ───────────│
//!       │        (stream_id, sample_rate)  │
//!       │                                  │
//!       │◄══════ VBAN Audio Stream ══════│
//!       │        (continuous)              │
//!       │                                  │
//!       │─────── UNSUBSCRIBE ─────────────►│
//!       │        (stream_id)               │
//!       │                                  │
//!       │◄────── UNSUBSCRIBE_ACK ─────────│
//! ```

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// Unique identifier for a subscription.
pub type SubscriptionId = u64;

/// Subscription request from destination to source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeRequest {
    /// Unique request ID for correlation.
    pub request_id: u64,
    /// Name of the stream to subscribe to.
    pub stream_name: String,
    /// Source device ID.
    pub source_device: String,
    /// Source channels (1-based, list of channels wanted).
    pub source_channels: Vec<u16>,
    /// Destination node name (for source to send to).
    pub destination_node: String,
    /// Destination address for VBAN stream.
    pub destination_addr: SocketAddr,
    /// Requested sample rate (0 = source default).
    pub sample_rate: u32,
}

/// Acknowledgment of subscription request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeAck {
    /// Correlates to request_id.
    pub request_id: u64,
    /// Assigned subscription ID.
    pub subscription_id: SubscriptionId,
    /// Actual sample rate of the stream.
    pub sample_rate: u32,
    /// Actual channels being sent.
    pub channels: Vec<u16>,
    /// Stream name that will appear in VBAN packets.
    pub vban_stream_name: String,
    /// Success or error.
    pub result: SubscribeResult,
}

/// Result of subscription attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SubscribeResult {
    /// Subscription successful.
    Success,
    /// Device not found.
    DeviceNotFound,
    /// Channel not available.
    ChannelNotAvailable,
    /// Stream limit reached.
    StreamLimitReached,
    /// Other error.
    Error(String),
}

impl SubscribeResult {
    /// Returns true if subscription was successful.
    #[must_use]
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }
}

/// Unsubscribe request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsubscribeRequest {
    /// Request ID for correlation.
    pub request_id: u64,
    /// Subscription to cancel.
    pub subscription_id: SubscriptionId,
}

/// Acknowledgment of unsubscribe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsubscribeAck {
    /// Correlates to request_id.
    pub request_id: u64,
    /// Subscription that was cancelled.
    pub subscription_id: SubscriptionId,
    /// Whether unsubscribe was successful.
    pub success: bool,
}

/// All subscription protocol messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SubscriptionMessage {
    /// Subscribe request (dest → source).
    Subscribe(SubscribeRequest),
    /// Subscribe acknowledgment (source → dest).
    SubscribeAck(SubscribeAck),
    /// Unsubscribe request (dest → source).
    Unsubscribe(UnsubscribeRequest),
    /// Unsubscribe acknowledgment (source → dest).
    UnsubscribeAck(UnsubscribeAck),
    /// Heartbeat to keep subscription alive.
    Heartbeat { subscription_id: SubscriptionId },
    /// Heartbeat acknowledgment.
    HeartbeatAck { subscription_id: SubscriptionId },
}

impl SubscriptionMessage {
    /// Serializes the message to JSON bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Deserializes a message from JSON bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

/// State of an active subscription.
#[derive(Debug, Clone)]
pub struct Subscription {
    /// Unique subscription ID.
    pub id: SubscriptionId,
    /// Source node name.
    pub source_node: String,
    /// Source device ID.
    pub source_device: String,
    /// Source channels.
    pub source_channels: Vec<u16>,
    /// Destination device ID.
    pub dest_device: String,
    /// Destination channels.
    pub dest_channels: Vec<u16>,
    /// VBAN stream name.
    pub vban_stream_name: String,
    /// Sample rate.
    pub sample_rate: u32,
    /// When subscription was created.
    pub created_at: Instant,
    /// Last heartbeat received.
    pub last_heartbeat: Instant,
    /// State of the subscription.
    pub state: SubscriptionState,
}

/// State of a subscription.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionState {
    /// Subscription is being set up.
    Pending,
    /// Subscription is active and receiving audio.
    Active,
    /// Subscription is paused (no audio flowing).
    Paused,
    /// Subscription failed.
    Failed,
    /// Subscription is being torn down.
    Closing,
}

impl Subscription {
    /// Creates a new pending subscription.
    #[must_use]
    pub fn new(
        id: SubscriptionId,
        source_node: String,
        source_device: String,
        source_channels: Vec<u16>,
        dest_device: String,
        dest_channels: Vec<u16>,
        vban_stream_name: String,
        sample_rate: u32,
    ) -> Self {
        let now = Instant::now();
        Self {
            id,
            source_node,
            source_device,
            source_channels,
            dest_device,
            dest_channels,
            vban_stream_name,
            sample_rate,
            created_at: now,
            last_heartbeat: now,
            state: SubscriptionState::Pending,
        }
    }

    /// Checks if subscription has timed out.
    #[must_use]
    pub fn is_timed_out(&self, timeout: Duration) -> bool {
        self.last_heartbeat.elapsed() > timeout
    }

    /// Updates the last heartbeat time.
    pub fn heartbeat(&mut self) {
        self.last_heartbeat = Instant::now();
    }

    /// Marks subscription as active.
    pub fn activate(&mut self) {
        self.state = SubscriptionState::Active;
        self.last_heartbeat = Instant::now();
    }
}

/// Configuration for subscription management.
#[derive(Debug, Clone)]
pub struct SubscriptionConfig {
    /// Heartbeat interval.
    pub heartbeat_interval: Duration,
    /// Timeout before subscription is considered dead.
    pub timeout: Duration,
    /// Maximum subscriptions per source.
    pub max_subscriptions_per_source: usize,
    /// Maximum total subscriptions.
    pub max_total_subscriptions: usize,
}

impl Default for SubscriptionConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_secs(5),
            timeout: Duration::from_secs(15),
            max_subscriptions_per_source: 64,
            max_total_subscriptions: 256,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn subscribe_request_serialization() {
        let request = SubscribeRequest {
            request_id: 1,
            stream_name: "mic-1".to_string(),
            source_device: "ALSA:input:default".to_string(),
            source_channels: vec![1, 2],
            destination_node: "studio-pc".to_string(),
            destination_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 6980),
            sample_rate: 48000,
        };

        let msg = SubscriptionMessage::Subscribe(request);
        let bytes = msg.to_bytes().unwrap();
        let decoded = SubscriptionMessage::from_bytes(&bytes).unwrap();

        match decoded {
            SubscriptionMessage::Subscribe(req) => {
                assert_eq!(req.request_id, 1);
                assert_eq!(req.stream_name, "mic-1");
                assert_eq!(req.source_channels, vec![1, 2]);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn subscribe_ack_serialization() {
        let ack = SubscribeAck {
            request_id: 1,
            subscription_id: 42,
            sample_rate: 48000,
            channels: vec![1, 2],
            vban_stream_name: "MIC-1".to_string(),
            result: SubscribeResult::Success,
        };

        let msg = SubscriptionMessage::SubscribeAck(ack);
        let bytes = msg.to_bytes().unwrap();
        let decoded = SubscriptionMessage::from_bytes(&bytes).unwrap();

        match decoded {
            SubscriptionMessage::SubscribeAck(a) => {
                assert_eq!(a.subscription_id, 42);
                assert!(a.result.is_success());
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn subscription_timeout() {
        let sub = Subscription::new(
            1,
            "node1".to_string(),
            "device1".to_string(),
            vec![1],
            "device2".to_string(),
            vec![1],
            "STREAM1".to_string(),
            48000,
        );

        assert!(!sub.is_timed_out(Duration::from_secs(1)));
        // Can't easily test timeout without sleeping
    }

    #[test]
    fn subscription_state_transitions() {
        let mut sub = Subscription::new(
            1,
            "node1".to_string(),
            "device1".to_string(),
            vec![1],
            "device2".to_string(),
            vec![1],
            "STREAM1".to_string(),
            48000,
        );

        assert_eq!(sub.state, SubscriptionState::Pending);

        sub.activate();
        assert_eq!(sub.state, SubscriptionState::Active);
    }
}
