//! Cross-node routing operations.
//!
//! This module handles the VBAN subscription and route forwarding
//! logic for cross-node audio routing.

use std::sync::Arc;

use ram_core::RouteController;

use crate::models::{NodeInfo, RouteDefinition};
use crate::subscription_client::SubscriptionClient;

/// Initiates a VBAN subscription to a remote source node.
///
/// This is called when we receive a route where the source is on a remote node.
/// We need to subscribe to that node to receive the VBAN audio stream.
pub fn initiate_subscription(
    client: SubscriptionClient,
    local_node: NodeInfo,
    route: RouteDefinition,
    source_node: NodeInfo,
    route_controller: Option<Arc<dyn RouteController>>,
) {
    tokio::spawn(async move {
        // Determine our local address that the source should send VBAN to.
        // We detect this by creating a UDP socket and connecting to the source's address.
        // The socket's local address tells us which interface/IP is used to reach that destination.
        let local_ip = detect_local_ip_for_target(source_node.addresses.first());

        tracing::debug!("Detected local IP for VBAN destination: {}", local_ip);

        let local_addr = format!("{}:{}", local_ip, local_node.vban_port)
            .parse()
            .unwrap_or_else(|_| "0.0.0.0:6980".parse().unwrap());

        match client
            .subscribe(
                &source_node,
                &route.source_device,
                vec![route.source_channel],
                local_addr,
                48000, // Default sample rate
            )
            .await
        {
            Ok(response) => {
                if response.success {
                    tracing::info!(
                        "Cross-node subscription established: {} -> {} (stream: {:?})",
                        route.source_node,
                        route.destination_node,
                        response.vban_stream_name
                    );

                    // Register VBAN stream buffers if we have a route controller
                    if let (Some(controller), Some(stream_name)) =
                        (&route_controller, &response.vban_stream_name)
                    {
                        register_vban_stream(controller, &route, stream_name);
                    }
                } else {
                    tracing::error!(
                        "Cross-node subscription failed: {}",
                        response.error.unwrap_or_else(|| "unknown".to_string())
                    );
                }
            },
            Err(e) => {
                tracing::error!("Failed to initiate subscription: {}", e);
            },
        }
    });
}

/// Registers VBAN stream buffers and starts the output stream.
fn register_vban_stream(
    controller: &Arc<dyn RouteController>,
    route: &RouteDefinition,
    stream_name: &str,
) {
    // Allocate receive buffers for incoming VBAN audio
    match controller
        .allocate_receive_buffers(&route.destination_device, &[route.destination_channel])
    {
        Ok(buffer_indices) => {
            tracing::info!(
                "Allocated receive buffers {:?} for VBAN stream '{}'",
                buffer_indices,
                stream_name
            );

            // Register the stream name to buffer mapping so received
            // VBAN packets will be written to these buffers
            controller.register_vban_stream_buffers(stream_name, buffer_indices);

            tracing::info!(
                "VBAN stream '{}' registered for receive routing",
                stream_name
            );

            // Start the output stream so audio can play through the destination device
            if let Err(e) = controller.ensure_output_stream(&route.destination_device) {
                tracing::error!(
                    "Failed to start output stream for {}: {}",
                    route.destination_device,
                    e
                );
            }
        },
        Err(e) => {
            tracing::error!(
                "Failed to allocate receive buffers for VBAN stream '{}': {}",
                stream_name,
                e
            );
        },
    }
}

/// Forwards a route to the destination node for outgoing cross-node routes.
///
/// When we create a local→remote route, we need the remote node to:
/// 1. Store the route (from its perspective, source is remote)
/// 2. Initiate a subscription back to us
pub fn forward_route_to_destination(
    local_node: NodeInfo,
    route: RouteDefinition,
    dest_node: NodeInfo,
) {
    // Transform the route so the destination node sees it correctly:
    // - Replace "LOCAL" source with our actual node ID
    // - The destination node will treat this as remote→local from its perspective
    let transformed_route = RouteDefinition {
        source_node: local_node.id.clone(),
        source_device: route.source_device.clone(),
        source_channel: route.source_channel,
        destination_node: "LOCAL".to_string(), // Destination is local from their perspective
        destination_device: route.destination_device.clone(),
        destination_channel: route.destination_channel,
        volume: route.volume,
        muted: route.muted,
    };

    // Spawn async task to send route to destination
    tokio::spawn(async move {
        // Get destination node's API address
        let Some(dest_ip) = dest_node.addresses.first() else {
            tracing::warn!(
                "Cannot forward route to {}: no addresses available",
                dest_node.name
            );
            return;
        };

        let dest_api_url = format!("http://{}:{}/api/v1/routes", dest_ip, dest_node.api_port);

        tracing::info!(
            "Forwarding route to {} ({}): {}:{} -> {}:{}",
            dest_node.name,
            dest_api_url,
            transformed_route.source_device,
            transformed_route.source_channel,
            transformed_route.destination_device,
            transformed_route.destination_channel,
        );

        // Send the route to the destination node
        let client = reqwest::Client::new();
        match client
            .post(&dest_api_url)
            .json(&transformed_route)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    tracing::info!("Route forwarded successfully to {}", dest_node.name);
                } else {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    tracing::error!(
                        "Failed to forward route to {}: {} - {}",
                        dest_node.name,
                        status,
                        body
                    );
                }
            },
            Err(e) => {
                tracing::error!("Failed to forward route to {}: {}", dest_node.name, e);
            },
        }
    });
}

/// Detects the local IP address to use for reaching a target IP.
fn detect_local_ip_for_target(target_ip: Option<&String>) -> String {
    match target_ip {
        Some(source_ip) => {
            // Create a UDP socket and "connect" to the source (doesn't send anything)
            match std::net::UdpSocket::bind("0.0.0.0:0") {
                Ok(socket) => {
                    // Connect to the source on any port to determine our local IP
                    match socket.connect(format!("{}:8080", source_ip)) {
                        Ok(()) => {
                            // Get our local address for this connection
                            socket
                                .local_addr()
                                .map(|addr| addr.ip().to_string())
                                .unwrap_or_else(|_| "0.0.0.0".to_string())
                        },
                        Err(_) => "0.0.0.0".to_string(),
                    }
                },
                Err(_) => "0.0.0.0".to_string(),
            }
        },
        None => "0.0.0.0".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_local_ip_no_target() {
        let ip = detect_local_ip_for_target(None);
        assert_eq!(ip, "0.0.0.0");
    }
}
