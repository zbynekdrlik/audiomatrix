//! Device card component.

use leptos::prelude::*;
use ram_api::models::{DeviceInfo, DeviceStatus};
use wasm_bindgen_futures::spawn_local;

use super::Meter;
use crate::api;
use crate::state::{AppState, ChannelLevel};

/// Card displaying a single audio device with meters.
#[component]
pub fn DeviceCard(
    /// The device to display.
    device: DeviceInfo,
    /// Whether this is an input device.
    #[prop(default = true)]
    is_input: bool,
    /// Whether to show attach/detach controls.
    #[prop(default = true)]
    show_controls: bool,
) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let device_id = device.id.clone();
    let device_id_attach = device.id.clone();
    let device_id_detach = device.id.clone();
    let device_name = device
        .display_name
        .clone()
        .unwrap_or_else(|| device.name.clone());
    let channel_count = if is_input {
        device.input_channels
    } else {
        device.output_channels
    };
    let sample_rate = device.sample_rate;
    let device_status = device.status;

    // Get levels for this device
    let levels = {
        let app_state = app_state.clone();
        let device_id = device_id.clone();
        Signal::derive(move || {
            let levels_map = if is_input {
                app_state.input_levels.get()
            } else {
                app_state.output_levels.get()
            };
            levels_map
                .get(&device_id)
                .cloned()
                .unwrap_or_else(|| vec![ChannelLevel::default(); channel_count as usize])
        })
    };

    let device_type_class = if is_input {
        "device-input"
    } else {
        "device-output"
    };

    // Status class and text
    let status_class = match device_status {
        DeviceStatus::Available => "status-available",
        DeviceStatus::Attached => "status-attached",
        DeviceStatus::Active => "status-active",
        DeviceStatus::Detached => "status-available",
        DeviceStatus::Error => "status-error",
    };

    let status_text = match device_status {
        DeviceStatus::Available => "Available",
        DeviceStatus::Attached => "Attached",
        DeviceStatus::Active => "Active",
        DeviceStatus::Detached => "Detached",
        DeviceStatus::Error => "Error",
    };

    // Handle attach click
    let on_attach = {
        let app_state = app_state.clone();
        move |_| {
            let device_id = device_id_attach.clone();
            let app_state = app_state.clone();

            // Get current node ID
            let node_id = app_state
                .current_node
                .get()
                .map(|n| n.id)
                .unwrap_or_else(|| "LOCAL".to_string());

            spawn_local(async move {
                match api::attach_device(&node_id, &device_id, None).await {
                    Ok(updated_device) => {
                        // Update device in state
                        app_state.devices.update(|devices| {
                            if let Some(d) = devices.iter_mut().find(|d| d.id == updated_device.id)
                            {
                                *d = updated_device;
                            }
                        });
                    },
                    Err(e) => {
                        log::error!("Failed to attach device: {}", e);
                        app_state
                            .error
                            .set(Some(format!("Failed to attach device: {}", e)));
                    },
                }
            });
        }
    };

    // Handle detach click
    let on_detach = {
        let app_state = app_state.clone();
        move |_| {
            let device_id = device_id_detach.clone();
            let app_state = app_state.clone();

            // Get current node ID
            let node_id = app_state
                .current_node
                .get()
                .map(|n| n.id)
                .unwrap_or_else(|| "LOCAL".to_string());

            spawn_local(async move {
                match api::detach_device(&node_id, &device_id).await {
                    Ok(_) => {
                        // Update device status in state
                        app_state.devices.update(|devices| {
                            if let Some(d) = devices.iter_mut().find(|d| d.id == device_id) {
                                d.status = DeviceStatus::Available;
                            }
                        });
                    },
                    Err(e) => {
                        log::error!("Failed to detach device: {}", e);
                        app_state
                            .error
                            .set(Some(format!("Failed to detach device: {}", e)));
                    },
                }
            });
        }
    };

    // Determine which buttons to show
    let show_attach =
        device_status == DeviceStatus::Available || device_status == DeviceStatus::Detached;
    let show_detach =
        device_status == DeviceStatus::Attached || device_status == DeviceStatus::Active;

    view! {
        <div class=format!("device-card {}", device_type_class)>
            <div class="device-header">
                <span class="device-name">{device_name}</span>
                <span class=format!("device-status {}", status_class)>{status_text}</span>
            </div>

            <div class="device-meters">
                <For
                    each=move || {
                        (0..channel_count as usize).collect::<Vec<_>>()
                    }
                    key=|idx| *idx
                    children=move |idx| {
                        let level_signal = {
                            let levels = levels.clone();
                            Signal::derive(move || {
                                levels.get().get(idx).map_or(-60.0, |l| l.level_db)
                            })
                        };
                        let peak_signal = {
                            let levels = levels.clone();
                            Signal::derive(move || {
                                levels.get().get(idx).map_or(-60.0, |l| l.peak_db)
                            })
                        };
                        view! {
                            <div class="channel-meter">
                                <span class="channel-label">{idx + 1}</span>
                                <Meter
                                    level=level_signal
                                    peak=peak_signal
                                    orientation="vertical"
                                />
                            </div>
                        }
                    }
                />
            </div>

            <div class="device-info">
                <span class="device-channels">{channel_count}" ch"</span>
                <span class="device-sample-rate">" @ "{sample_rate}" Hz"</span>
            </div>

            {show_controls.then(|| view! {
                <div class="device-actions">
                    {show_attach.then(|| view! {
                        <button class="device-btn attach-btn" on:click=on_attach.clone()>
                            "Attach"
                        </button>
                    })}
                    {show_detach.then(|| view! {
                        <button class="device-btn detach-btn" on:click=on_detach.clone()>
                            "Detach"
                        </button>
                    })}
                </div>
            })}
        </div>
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn device_card_compiles() {
        // Compilation test
    }
}
