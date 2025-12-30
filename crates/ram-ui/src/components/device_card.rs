//! Device card component.

use leptos::prelude::*;
use ram_api::models::{DeviceInfo, DeviceStatus, DeviceType};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use super::{ChannelLabelEditor, Meter};
use crate::api;
use crate::services::websocket::WsService;
use crate::state::{AppState, ChannelLevel};

/// Card displaying a single audio device with meters.
/// Shows UNIFIED device with both inputs and outputs.
#[component]
pub fn DeviceCard(
    /// The device to display.
    device: DeviceInfo,
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
    let input_channels = device.input_channels;
    let output_channels = device.output_channels;
    let sample_rate = device.sample_rate;
    let device_status = device.status;
    let device_type = device.device_type;
    let device_backend = device.backend.clone().unwrap_or_default();

    // Get input levels for this device
    let input_levels = {
        let app_state = app_state.clone();
        let device_id = device_id.clone();
        Signal::derive(move || {
            let levels_map = app_state.input_levels.get();
            levels_map
                .get(&device_id)
                .cloned()
                .unwrap_or_else(|| vec![ChannelLevel::default(); input_channels as usize])
        })
    };

    // Get output levels for this device
    let output_levels = {
        let app_state = app_state.clone();
        let device_id = device_id.clone();
        Signal::derive(move || {
            let levels_map = app_state.output_levels.get();
            levels_map
                .get(&device_id)
                .cloned()
                .unwrap_or_else(|| vec![ChannelLevel::default(); output_channels as usize])
        })
    };

    // Subscribe to metering when device is attached/active
    {
        let ws_service = expect_context::<WsService>();
        let device_id_sub = device_id.clone();
        let node_id = app_state
            .current_node
            .get()
            .map(|n| n.id)
            .unwrap_or_else(|| "LOCAL".to_string());

        // Only subscribe if device is attached or active
        if matches!(device_status, DeviceStatus::Attached | DeviceStatus::Active) {
            log::debug!(
                "Subscribing to metering for device {} on node {}",
                device_id_sub,
                node_id
            );
            ws_service.subscribe_metering(&node_id, &device_id_sub);
        }
    }

    // Device type indicator
    let type_indicator = match device_type {
        DeviceType::Input => "IN",
        DeviceType::Output => "OUT",
        DeviceType::Duplex => "I/O",
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
                    Ok(response) => {
                        if response.success {
                            if let Some(updated_device) = response.device {
                                // Update device in state
                                app_state.devices.update(|devices| {
                                    if let Some(d) =
                                        devices.iter_mut().find(|d| d.id == updated_device.id)
                                    {
                                        *d = updated_device;
                                    }
                                });
                            }
                        } else {
                            let err_msg = response
                                .error
                                .unwrap_or_else(|| "Unknown error".to_string());
                            log::error!("Failed to attach device: {}", err_msg);
                            app_state
                                .error
                                .set(Some(format!("Failed to attach device: {}", err_msg)));
                        }
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
                    Ok(()) => {
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

    // Channel label editor state
    let show_label_editor = RwSignal::new(false);

    // Device rename state
    let is_renaming = RwSignal::new(false);
    let rename_value = RwSignal::new(device_name.clone());
    let current_display_name = RwSignal::new(device_name.clone());

    // Save rename
    let on_save_rename = {
        let device_id = device.id.clone();
        let app_state = app_state.clone();
        move |_| {
            let new_name = rename_value.get();
            let device_id = device_id.clone();
            let app_state = app_state.clone();

            let node_id = app_state
                .current_node
                .get()
                .map(|n| n.id)
                .unwrap_or_else(|| "LOCAL".to_string());

            spawn_local(async move {
                match api::update_device(&node_id, &device_id, Some(&new_name)).await {
                    Ok(updated) => {
                        let display = updated
                            .display_name
                            .clone()
                            .unwrap_or_else(|| updated.name.clone());
                        current_display_name.set(display);
                        // Update device in state
                        app_state.devices.update(|devices| {
                            if let Some(d) = devices.iter_mut().find(|d| d.id == updated.id) {
                                *d = updated;
                            }
                        });
                        is_renaming.set(false);
                    },
                    Err(e) => {
                        log::error!("Failed to rename device: {}", e);
                        app_state
                            .error
                            .set(Some(format!("Failed to rename: {}", e)));
                    },
                }
            });
        }
    };

    // Cancel rename
    let on_cancel_rename = move |_| {
        rename_value.set(current_display_name.get());
        is_renaming.set(false);
    };

    // Input change
    let on_rename_input = move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let input: web_sys::HtmlInputElement = target.dyn_into().unwrap();
        rename_value.set(input.value());
    };

    // IDs for label editor
    let device_id_for_editor = device.id.clone();
    let device_name_for_editor = current_display_name.clone();
    let node_id_for_editor = app_state
        .current_node
        .get()
        .map(|n| n.id)
        .unwrap_or_else(|| "LOCAL".to_string());

    // Format channel info string
    let channel_info = if input_channels > 0 && output_channels > 0 {
        format!("{} in / {} out", input_channels, output_channels)
    } else if input_channels > 0 {
        format!("{} in", input_channels)
    } else if output_channels > 0 {
        format!("{} out", output_channels)
    } else {
        "0 ch".to_string()
    };

    // Build input channels list
    let input_ch_list: Vec<usize> = (0..input_channels as usize).collect();
    let output_ch_list: Vec<usize> = (0..output_channels as usize).collect();

    view! {
        // Channel Label Editor Dialog
        {move || {
            if show_label_editor.get() {
                let node_id = node_id_for_editor.clone();
                let device_id = device_id_for_editor.clone();
                let device_name = device_name_for_editor.get();
                Some(view! {
                    <ChannelLabelEditor
                        node_id=node_id
                        device_id=device_id
                        device_name=device_name
                        on_close=Callback::new(move |()| show_label_editor.set(false))
                    />
                })
            } else {
                None
            }
        }}

        <div class="device-card">
            <div class="device-header">
                <div class="device-badges">
                    <span class="device-type-badge">{type_indicator}</span>
                    {if !device_backend.is_empty() {
                        Some(view! { <span class="device-backend-badge">{device_backend.clone()}</span> })
                    } else {
                        None
                    }}
                </div>
                {move || {
                    if is_renaming.get() {
                        view! {
                            <div class="device-rename-form">
                                <input
                                    type="text"
                                    class="device-rename-input"
                                    prop:value=move || rename_value.get()
                                    on:input=on_rename_input.clone()
                                    maxlength="31"
                                />
                                <button class="rename-save-btn" on:click=on_save_rename.clone()>"✓"</button>
                                <button class="rename-cancel-btn" on:click=on_cancel_rename>"✕"</button>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <span class="device-name" title="Click to rename" on:dblclick=move |_| is_renaming.set(true)>
                                {move || current_display_name.get()}
                            </span>
                        }.into_any()
                    }
                }}
                <span class=format!("device-status {}", status_class)>{status_text}</span>
            </div>

            // Input channels meters (if any)
            {move || {
                if input_channels > 0 {
                    let ch_list = input_ch_list.clone();
                    Some(view! {
                        <div class="device-meters-section">
                            <span class="meters-label">"IN"</span>
                            <div class="device-meters">
                                {ch_list.into_iter().map(|idx| {
                                    let level_signal = {
                                        let levels = input_levels.clone();
                                        Signal::derive(move || {
                                            levels.get().get(idx).map_or(-60.0, |l| l.level_db)
                                        })
                                    };
                                    let peak_signal = {
                                        let levels = input_levels.clone();
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
                                }).collect_view()}
                            </div>
                        </div>
                    })
                } else {
                    None
                }
            }}

            // Output channels meters (if any)
            {move || {
                if output_channels > 0 {
                    let ch_list = output_ch_list.clone();
                    Some(view! {
                        <div class="device-meters-section">
                            <span class="meters-label">"OUT"</span>
                            <div class="device-meters">
                                {ch_list.into_iter().map(|idx| {
                                    let level_signal = {
                                        let levels = output_levels.clone();
                                        Signal::derive(move || {
                                            levels.get().get(idx).map_or(-60.0, |l| l.level_db)
                                        })
                                    };
                                    let peak_signal = {
                                        let levels = output_levels.clone();
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
                                }).collect_view()}
                            </div>
                        </div>
                    })
                } else {
                    None
                }
            }}

            <div class="device-info">
                <span class="device-channels">{channel_info}</span>
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
                <div class="device-config-actions">
                    <button
                        class="device-btn config-btn"
                        title="Rename device"
                        on:click=move |_| is_renaming.set(true)
                    >
                        "Rename"
                    </button>
                    <button
                        class="device-btn config-btn"
                        title="Edit channel labels"
                        on:click=move |_| show_label_editor.set(true)
                    >
                        "Labels"
                    </button>
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
