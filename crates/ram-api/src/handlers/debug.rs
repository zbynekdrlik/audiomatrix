//! Debug handlers for inspecting service state.

use axum::extract::State;
use axum::Json;
use ram_core::route_controller::{AudioDiagnostics, DeviceDiagnostic};

use crate::state::AppState;

/// Debug information response.
#[derive(Debug, serde::Serialize)]
pub struct DebugInfo {
    /// Stream counts from stream registry.
    pub stream_registry: StreamRegistryDebug,
    /// Route controller status.
    pub route_controller_available: bool,
    /// Attached devices.
    pub attached_devices: Vec<String>,
    /// Audio system diagnostics (cpal devices).
    pub audio_diagnostics: Option<AudioDiagnostics>,
    /// Device-specific diagnostics for attached devices.
    pub device_diagnostics: Vec<DeviceDiagnostic>,
}

/// Stream registry debug info.
#[derive(Debug, serde::Serialize)]
pub struct StreamRegistryDebug {
    /// Number of input streams.
    pub input_count: usize,
    /// Number of output streams.
    pub output_count: usize,
}

/// Debug metering response - shows current meter levels.
#[derive(Debug, serde::Serialize)]
pub struct DebugMetering {
    /// Number of input devices with metering.
    pub input_device_count: usize,
    /// Number of output devices with metering.
    pub output_device_count: usize,
    /// Input meter data per device.
    pub input_meters: Vec<DeviceMeterDebug>,
    /// Output meter data per device.
    pub output_meters: Vec<DeviceMeterDebug>,
}

/// Per-device meter debug info.
#[derive(Debug, serde::Serialize)]
pub struct DeviceMeterDebug {
    /// Device ID.
    pub device_id: String,
    /// Number of channels.
    pub channel_count: usize,
    /// Max RMS level across all channels (dB).
    pub max_rms_db: f32,
    /// Max peak level across all channels (dB).
    pub max_peak_db: f32,
    /// Per-channel RMS levels (dB).
    pub rms_levels: Vec<f32>,
    /// Per-channel peak levels (dB).
    pub peak_levels: Vec<f32>,
}

/// Get debug metering information.
///
/// This endpoint returns the current meter levels for all devices,
/// useful for debugging metering issues without WebSocket.
pub async fn get_debug_metering(State(state): State<AppState>) -> Json<DebugMetering> {
    let mut input_meters = Vec::new();
    let mut output_meters = Vec::new();

    if let Some(controller) = state.route_controller() {
        // Get input meters
        for (device_id, levels) in controller.all_input_meters() {
            let rms_levels: Vec<f32> = levels.iter().map(|l| l.rms_db).collect();
            let peak_levels: Vec<f32> = levels.iter().map(|l| l.peak_db).collect();
            let max_rms = rms_levels.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let max_peak = peak_levels.iter().copied().fold(f32::NEG_INFINITY, f32::max);

            input_meters.push(DeviceMeterDebug {
                device_id,
                channel_count: levels.len(),
                max_rms_db: max_rms,
                max_peak_db: max_peak,
                rms_levels,
                peak_levels,
            });
        }

        // Get output meters
        for (device_id, levels) in controller.all_output_meters() {
            let rms_levels: Vec<f32> = levels.iter().map(|l| l.rms_db).collect();
            let peak_levels: Vec<f32> = levels.iter().map(|l| l.peak_db).collect();
            let max_rms = rms_levels.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let max_peak = peak_levels.iter().copied().fold(f32::NEG_INFINITY, f32::max);

            output_meters.push(DeviceMeterDebug {
                device_id,
                channel_count: levels.len(),
                max_rms_db: max_rms,
                max_peak_db: max_peak,
                rms_levels,
                peak_levels,
            });
        }
    }

    Json(DebugMetering {
        input_device_count: input_meters.len(),
        output_device_count: output_meters.len(),
        input_meters,
        output_meters,
    })
}

/// Get debug information about the service state.
pub async fn get_debug_info(State(state): State<AppState>) -> Json<DebugInfo> {
    let (input_count, output_count) = state.stream_counts();
    let route_controller_available = state.has_route_controller();

    let attached_devices: Vec<String> = state
        .all_devices()
        .into_iter()
        .filter(|d| {
            matches!(
                d.status,
                crate::models::DeviceStatus::Attached | crate::models::DeviceStatus::Active
            )
        })
        .map(|d| d.id)
        .collect();

    // Get audio diagnostics if route controller is available
    let (audio_diagnostics, device_diagnostics) = if let Some(controller) = state.route_controller()
    {
        let diags = controller.audio_diagnostics();
        let device_diags: Vec<DeviceDiagnostic> = attached_devices
            .iter()
            .map(|id| controller.diagnose_device(id))
            .collect();
        (Some(diags), device_diags)
    } else {
        (None, Vec::new())
    };

    Json(DebugInfo {
        stream_registry: StreamRegistryDebug {
            input_count,
            output_count,
        },
        route_controller_available,
        attached_devices,
        audio_diagnostics,
        device_diagnostics,
    })
}
