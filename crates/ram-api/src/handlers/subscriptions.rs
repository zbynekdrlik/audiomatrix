//! Subscription request handlers.

use axum::extract::State;
use axum::Json;

use crate::models::{SubscriptionRequest, SubscriptionResponse};
use crate::state::AppState;
use crate::Result;

/// Create a subscription (called by destination node on source node).
///
/// This endpoint is called when a destination node wants to subscribe
/// to audio from this node. We validate the device exists and start
/// sending VBAN audio to the destination.
///
/// # Errors
///
/// Returns an error if the device doesn't exist or subscription fails.
pub async fn create_subscription(
    State(state): State<AppState>,
    Json(request): Json<SubscriptionRequest>,
) -> Result<Json<SubscriptionResponse>> {
    // Validate that the source device exists
    let device = state.get_device(&request.source_device);
    if device.is_none() {
        return Ok(Json(SubscriptionResponse {
            success: false,
            subscription_id: None,
            vban_stream_name: None,
            sample_rate: None,
            error: Some(format!("Device not found: {}", request.source_device)),
        }));
    }

    // If we have a route controller, create the subscription
    if let Some(controller) = state.route_controller() {
        let manager = controller.subscription_manager();

        // Convert the request to a core subscription request
        let dest_addr = request
            .destination_addr
            .parse()
            .map_err(|e| crate::Error::BadRequest(format!("Invalid address: {e}")))?;

        let core_request = ram_core::subscription::SubscribeRequest {
            request_id: 0, // Will be assigned by manager
            stream_name: request.stream_name.clone(),
            source_device: request.source_device.clone(),
            source_channels: request.source_channels.clone(),
            destination_node: request.destination_node.clone(),
            destination_addr: dest_addr,
            sample_rate: request.sample_rate,
        };

        // Handle the subscription request
        let ack = manager.handle_subscribe_request(core_request, |_device, _channels| {
            // For now, accept all devices that exist
            true
        });

        if ack.result.is_success() {
            tracing::info!(
                "Subscription created: stream='{}' -> {}",
                ack.vban_stream_name,
                request.destination_addr
            );

            // Start VBAN sender for this subscription
            tracing::info!(
                "Ensuring input stream for device '{}' channels {:?}",
                request.source_device,
                request.source_channels
            );
            let source_buffers = match controller
                .ensure_input_stream(&request.source_device, &request.source_channels)
            {
                Ok(buffers) => {
                    tracing::info!("Got buffer indices: {:?}", buffers);
                    buffers
                },
                Err(e) => {
                    tracing::error!("Failed to start input stream: {}", e);
                    return Ok(Json(SubscriptionResponse {
                        success: false,
                        subscription_id: None,
                        vban_stream_name: None,
                        sample_rate: None,
                        error: Some(format!("Failed to start input stream: {e}")),
                    }));
                },
            };

            // Start the VBAN sender
            tracing::info!(
                "Starting VBAN sender: sub_id={}, stream='{}', dest={}, buffers={:?}, channels={}",
                ack.subscription_id,
                ack.vban_stream_name,
                dest_addr,
                source_buffers,
                request.source_channels.len()
            );
            if let Err(e) = controller.start_vban_sender(
                ack.subscription_id,
                ack.vban_stream_name.clone(),
                dest_addr,
                source_buffers,
                request.source_channels.len() as u8,
            ) {
                tracing::error!("Failed to start VBAN sender: {}", e);
                return Ok(Json(SubscriptionResponse {
                    success: false,
                    subscription_id: None,
                    vban_stream_name: None,
                    sample_rate: None,
                    error: Some(format!("Failed to start VBAN sender: {e}")),
                }));
            }

            // Activate the subscription now that the VBAN sender is running
            manager.activate_incoming(ack.subscription_id);

            tracing::info!(
                "VBAN sender started for subscription {}: stream='{}'",
                ack.subscription_id,
                ack.vban_stream_name
            );

            Ok(Json(SubscriptionResponse {
                success: true,
                subscription_id: Some(ack.subscription_id),
                vban_stream_name: Some(ack.vban_stream_name),
                sample_rate: Some(ack.sample_rate),
                error: None,
            }))
        } else {
            Ok(Json(SubscriptionResponse {
                success: false,
                subscription_id: None,
                vban_stream_name: None,
                sample_rate: None,
                error: Some(format!("Subscription failed: {:?}", ack.result)),
            }))
        }
    } else {
        Ok(Json(SubscriptionResponse {
            success: false,
            subscription_id: None,
            vban_stream_name: None,
            sample_rate: None,
            error: Some("No route controller available".to_string()),
        }))
    }
}
