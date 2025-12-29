//! Subscription manager for cross-node audio routing.
//!
//! This module manages subscriptions between nodes, handling:
//! - Subscription lifecycle (create, maintain, teardown)
//! - VBAN stream creation for cross-node routes
//! - Heartbeat monitoring and timeout handling

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use parking_lot::RwLock;
use tracing::{debug, info, warn};

use crate::subscription::{
    SubscribeAck, SubscribeRequest, SubscribeResult, Subscription, SubscriptionConfig,
    SubscriptionId, SubscriptionState,
};

/// Manages subscriptions for a node.
#[derive(Debug)]
pub struct SubscriptionManager {
    /// Configuration.
    config: SubscriptionConfig,
    /// Local node name.
    node_name: String,
    /// Outgoing subscriptions (we are the destination).
    outgoing: RwLock<HashMap<SubscriptionId, Subscription>>,
    /// Incoming subscriptions (we are the source).
    incoming: RwLock<HashMap<SubscriptionId, Subscription>>,
    /// Next subscription ID.
    next_id: AtomicU64,
    /// Last cleanup time.
    last_cleanup: RwLock<Instant>,
}

impl SubscriptionManager {
    /// Creates a new subscription manager.
    #[must_use]
    pub fn new(node_name: String, config: SubscriptionConfig) -> Self {
        Self {
            config,
            node_name,
            outgoing: RwLock::new(HashMap::new()),
            incoming: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            last_cleanup: RwLock::new(Instant::now()),
        }
    }

    /// Creates with default configuration.
    #[must_use]
    pub fn with_defaults(node_name: String) -> Self {
        Self::new(node_name, SubscriptionConfig::default())
    }

    /// Returns the local node name.
    #[must_use]
    pub fn node_name(&self) -> &str {
        &self.node_name
    }

    // ========================================================================
    // Outgoing Subscriptions (we are the destination)
    // ========================================================================

    /// Creates a subscription request to a remote source.
    ///
    /// Returns the request to send to the source node.
    pub fn create_subscription_request(
        &self,
        source_node: &str,
        source_device: &str,
        source_channels: Vec<u16>,
        dest_device: &str,
        dest_channels: Vec<u16>,
        dest_addr: SocketAddr,
        sample_rate: u32,
    ) -> (SubscriptionId, SubscribeRequest) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        // Create stream name from source info
        let stream_name = format!(
            "{}-{}-{}",
            source_device.split(':').last().unwrap_or("stream"),
            source_channels.first().unwrap_or(&1),
            source_channels.len()
        );

        let request = SubscribeRequest {
            request_id: id,
            stream_name: stream_name.clone(),
            source_device: source_device.to_string(),
            source_channels: source_channels.clone(),
            destination_node: self.node_name.clone(),
            destination_addr: dest_addr,
            sample_rate,
        };

        // Create pending subscription
        let sub = Subscription::new(
            id,
            source_node.to_string(),
            source_device.to_string(),
            source_channels,
            dest_device.to_string(),
            dest_channels,
            stream_name,
            sample_rate,
        );

        self.outgoing.write().insert(id, sub);
        debug!("Created outgoing subscription request {id} to {source_node}");

        (id, request)
    }

    /// Handles acknowledgment of our subscription request.
    pub fn handle_subscribe_ack(&self, ack: SubscribeAck) -> bool {
        let mut outgoing = self.outgoing.write();

        if let Some(sub) = outgoing.get_mut(&ack.request_id) {
            if ack.result.is_success() {
                sub.activate();
                sub.vban_stream_name = ack.vban_stream_name;
                sub.sample_rate = ack.sample_rate;
                info!(
                    "Subscription {} activated: stream={}, rate={}Hz",
                    ack.subscription_id, sub.vban_stream_name, sub.sample_rate
                );
                true
            } else {
                sub.state = SubscriptionState::Failed;
                warn!("Subscription {} failed: {:?}", ack.request_id, ack.result);
                false
            }
        } else {
            warn!("Received ack for unknown subscription {}", ack.request_id);
            false
        }
    }

    /// Gets an outgoing subscription by ID.
    #[must_use]
    pub fn get_outgoing(&self, id: SubscriptionId) -> Option<Subscription> {
        self.outgoing.read().get(&id).cloned()
    }

    /// Removes an outgoing subscription.
    pub fn remove_outgoing(&self, id: SubscriptionId) -> Option<Subscription> {
        self.outgoing.write().remove(&id)
    }

    /// Returns all active outgoing subscriptions.
    #[must_use]
    pub fn active_outgoing(&self) -> Vec<Subscription> {
        self.outgoing
            .read()
            .values()
            .filter(|s| s.state == SubscriptionState::Active)
            .cloned()
            .collect()
    }

    /// Finds an outgoing subscription by VBAN stream name.
    #[must_use]
    pub fn find_outgoing_by_stream_name(&self, stream_name: &str) -> Option<Subscription> {
        self.outgoing
            .read()
            .values()
            .find(|s| s.vban_stream_name == stream_name)
            .cloned()
    }

    // ========================================================================
    // Incoming Subscriptions (we are the source)
    // ========================================================================

    /// Handles a subscription request from a remote destination.
    ///
    /// Returns the acknowledgment to send back.
    pub fn handle_subscribe_request(
        &self,
        request: SubscribeRequest,
        validate_device: impl Fn(&str, &[u16]) -> bool,
    ) -> SubscribeAck {
        // Validate the device and channels exist
        if !validate_device(&request.source_device, &request.source_channels) {
            return SubscribeAck {
                request_id: request.request_id,
                subscription_id: 0,
                sample_rate: 0,
                channels: vec![],
                vban_stream_name: String::new(),
                result: SubscribeResult::DeviceNotFound,
            };
        }

        // Check subscription limits
        let incoming = self.incoming.read();
        if incoming.len() >= self.config.max_total_subscriptions {
            return SubscribeAck {
                request_id: request.request_id,
                subscription_id: 0,
                sample_rate: 0,
                channels: vec![],
                vban_stream_name: String::new(),
                result: SubscribeResult::StreamLimitReached,
            };
        }
        drop(incoming);

        // Generate subscription ID and VBAN stream name
        let sub_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let vban_stream_name = format!(
            "{}-{}",
            request.stream_name.chars().take(12).collect::<String>(),
            sub_id % 1000
        )
        .to_uppercase();

        // Determine sample rate (use requested or default to 48000)
        let sample_rate = if request.sample_rate > 0 {
            request.sample_rate
        } else {
            48000
        };

        // Create incoming subscription record
        let sub = Subscription::new(
            sub_id,
            request.destination_node.clone(),
            request.source_device.clone(),
            request.source_channels.clone(),
            format!("{}:{}", request.destination_node, request.destination_addr),
            request.source_channels.clone(), // Dest channels = source channels for now
            vban_stream_name.clone(),
            sample_rate,
        );

        let mut incoming = self.incoming.write();
        incoming.insert(sub_id, sub);

        info!(
            "Accepted incoming subscription {} from {} for {}",
            sub_id, request.destination_node, request.source_device
        );

        SubscribeAck {
            request_id: request.request_id,
            subscription_id: sub_id,
            sample_rate,
            channels: request.source_channels,
            vban_stream_name,
            result: SubscribeResult::Success,
        }
    }

    /// Gets an incoming subscription by ID.
    #[must_use]
    pub fn get_incoming(&self, id: SubscriptionId) -> Option<Subscription> {
        self.incoming.read().get(&id).cloned()
    }

    /// Removes an incoming subscription.
    pub fn remove_incoming(&self, id: SubscriptionId) -> Option<Subscription> {
        self.incoming.write().remove(&id)
    }

    /// Activates an incoming subscription (marks it as active after VBAN sender starts).
    pub fn activate_incoming(&self, id: SubscriptionId) -> bool {
        if let Some(sub) = self.incoming.write().get_mut(&id) {
            sub.activate();
            true
        } else {
            false
        }
    }

    /// Returns all active incoming subscriptions.
    #[must_use]
    pub fn active_incoming(&self) -> Vec<Subscription> {
        self.incoming
            .read()
            .values()
            .filter(|s| s.state == SubscriptionState::Active)
            .cloned()
            .collect()
    }

    // ========================================================================
    // Heartbeat and Cleanup
    // ========================================================================

    /// Processes a heartbeat for a subscription.
    pub fn heartbeat(&self, id: SubscriptionId) -> bool {
        // Check outgoing first
        if let Some(sub) = self.outgoing.write().get_mut(&id) {
            sub.heartbeat();
            return true;
        }

        // Then incoming
        if let Some(sub) = self.incoming.write().get_mut(&id) {
            sub.heartbeat();
            return true;
        }

        false
    }

    /// Returns subscriptions that need heartbeats sent.
    #[must_use]
    pub fn subscriptions_needing_heartbeat(&self) -> Vec<SubscriptionId> {
        let cutoff = Instant::now() - self.config.heartbeat_interval;

        self.outgoing
            .read()
            .iter()
            .filter(|(_, sub)| {
                sub.state == SubscriptionState::Active && sub.last_heartbeat < cutoff
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// Cleans up timed-out subscriptions.
    ///
    /// Returns the IDs of removed subscriptions.
    pub fn cleanup_timed_out(&self) -> Vec<SubscriptionId> {
        let timeout = self.config.timeout;
        let mut removed = Vec::new();

        // Cleanup outgoing
        {
            let mut outgoing = self.outgoing.write();
            outgoing.retain(|id, sub| {
                if sub.is_timed_out(timeout) {
                    warn!("Outgoing subscription {} timed out", id);
                    removed.push(*id);
                    false
                } else {
                    true
                }
            });
        }

        // Cleanup incoming
        {
            let mut incoming = self.incoming.write();
            incoming.retain(|id, sub| {
                if sub.is_timed_out(timeout) {
                    warn!("Incoming subscription {} timed out", id);
                    removed.push(*id);
                    false
                } else {
                    true
                }
            });
        }

        *self.last_cleanup.write() = Instant::now();
        removed
    }

    // ========================================================================
    // Statistics
    // ========================================================================

    /// Returns subscription statistics.
    #[must_use]
    pub fn stats(&self) -> SubscriptionStats {
        let outgoing = self.outgoing.read();
        let incoming = self.incoming.read();

        SubscriptionStats {
            outgoing_total: outgoing.len(),
            outgoing_active: outgoing
                .values()
                .filter(|s| s.state == SubscriptionState::Active)
                .count(),
            outgoing_pending: outgoing
                .values()
                .filter(|s| s.state == SubscriptionState::Pending)
                .count(),
            incoming_total: incoming.len(),
            incoming_active: incoming
                .values()
                .filter(|s| s.state == SubscriptionState::Active)
                .count(),
        }
    }
}

/// Subscription statistics.
#[derive(Debug, Clone, Default)]
pub struct SubscriptionStats {
    /// Total outgoing subscriptions.
    pub outgoing_total: usize,
    /// Active outgoing subscriptions.
    pub outgoing_active: usize,
    /// Pending outgoing subscriptions.
    pub outgoing_pending: usize,
    /// Total incoming subscriptions.
    pub incoming_total: usize,
    /// Active incoming subscriptions.
    pub incoming_active: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn test_addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 6980)
    }

    #[test]
    fn create_subscription_request() {
        let mgr = SubscriptionManager::with_defaults("test-node".to_string());

        let (id, request) = mgr.create_subscription_request(
            "source-node",
            "ALSA:input:mic",
            vec![1, 2],
            "ALSA:output:speakers",
            vec![1, 2],
            test_addr(),
            48000,
        );

        assert_eq!(id, 1);
        assert_eq!(request.source_device, "ALSA:input:mic");
        assert_eq!(request.source_channels, vec![1, 2]);
        assert_eq!(request.destination_node, "test-node");

        // Should have pending subscription
        let sub = mgr.get_outgoing(id).unwrap();
        assert_eq!(sub.state, SubscriptionState::Pending);
    }

    #[test]
    fn handle_subscribe_ack_success() {
        let mgr = SubscriptionManager::with_defaults("test-node".to_string());

        let (id, _request) = mgr.create_subscription_request(
            "source-node",
            "ALSA:input:mic",
            vec![1],
            "ALSA:output:speakers",
            vec![1],
            test_addr(),
            48000,
        );

        let ack = SubscribeAck {
            request_id: id,
            subscription_id: 100,
            sample_rate: 48000,
            channels: vec![1],
            vban_stream_name: "MIC-100".to_string(),
            result: SubscribeResult::Success,
        };

        assert!(mgr.handle_subscribe_ack(ack));

        let sub = mgr.get_outgoing(id).unwrap();
        assert_eq!(sub.state, SubscriptionState::Active);
        assert_eq!(sub.vban_stream_name, "MIC-100");
    }

    #[test]
    fn handle_subscribe_request() {
        let mgr = SubscriptionManager::with_defaults("source-node".to_string());

        let request = SubscribeRequest {
            request_id: 1,
            stream_name: "mic-stream".to_string(),
            source_device: "ALSA:input:mic".to_string(),
            source_channels: vec![1, 2],
            destination_node: "dest-node".to_string(),
            destination_addr: test_addr(),
            sample_rate: 48000,
        };

        // Validator that always returns true
        let ack = mgr.handle_subscribe_request(request, |_dev, _ch| true);

        assert!(ack.result.is_success());
        assert_eq!(ack.sample_rate, 48000);
        assert!(!ack.vban_stream_name.is_empty());

        // Should have incoming subscription
        let sub = mgr.get_incoming(ack.subscription_id).unwrap();
        assert_eq!(sub.source_channels, vec![1, 2]);
    }

    #[test]
    fn handle_subscribe_request_device_not_found() {
        let mgr = SubscriptionManager::with_defaults("source-node".to_string());

        let request = SubscribeRequest {
            request_id: 1,
            stream_name: "mic-stream".to_string(),
            source_device: "ALSA:input:nonexistent".to_string(),
            source_channels: vec![1],
            destination_node: "dest-node".to_string(),
            destination_addr: test_addr(),
            sample_rate: 48000,
        };

        // Validator that always returns false
        let ack = mgr.handle_subscribe_request(request, |_dev, _ch| false);

        assert!(!ack.result.is_success());
        matches!(ack.result, SubscribeResult::DeviceNotFound);
    }

    #[test]
    fn stats() {
        let mgr = SubscriptionManager::with_defaults("test-node".to_string());

        let stats = mgr.stats();
        assert_eq!(stats.outgoing_total, 0);
        assert_eq!(stats.incoming_total, 0);

        // Create outgoing
        mgr.create_subscription_request(
            "source",
            "dev",
            vec![1],
            "dest",
            vec![1],
            test_addr(),
            48000,
        );

        let stats = mgr.stats();
        assert_eq!(stats.outgoing_total, 1);
        assert_eq!(stats.outgoing_pending, 1);
    }
}
