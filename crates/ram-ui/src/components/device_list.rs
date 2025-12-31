//! Device list component.

use leptos::prelude::*;
use ram_api::models::DeviceStatus;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use super::{DeviceCard, VirtualDeviceDialog};
use crate::api;
use crate::state::AppState;

/// List of all audio devices grouped by attachment status.
/// Devices are shown as unified entities with both inputs and outputs.
#[component]
pub fn DeviceList() -> impl IntoView {
    let app_state = expect_context::<AppState>();

    // Dialog state
    let show_virtual_dialog = RwSignal::new(false);

    // Selected node for device management (can be different from current_node for multi-node management)
    let selected_node_id = RwSignal::new(String::new());

    // Initialize selected node from current_node
    {
        let state = app_state.clone();
        Effect::new(move || {
            if selected_node_id.get().is_empty() {
                if let Some(node) = state.current_node.get() {
                    selected_node_id.set(node.id.clone());
                }
            }
        });
    }

    // Get current node for virtual device creation
    let selected_node = move || {
        let id = selected_node_id.get();
        if id.is_empty() {
            app_state
                .current_node
                .get()
                .map(|n| n.id)
                .unwrap_or_else(|| "local".to_string())
        } else {
            id
        }
    };

    // Fetch devices when selected node changes
    {
        let state = app_state.clone();
        Effect::new(move || {
            let node_id = selected_node_id.get();
            if !node_id.is_empty() {
                let state = state.clone();
                let node_id_clone = node_id.clone();
                let node_id_encoded = urlencoding::encode(&node_id).to_string();
                spawn_local(async move {
                    log::info!("Fetching devices for node: {}", node_id_clone);
                    match api::get_devices(&node_id_encoded).await {
                        Ok(devices) => {
                            log::info!("Got {} devices from node {}", devices.len(), node_id_clone);
                            state.devices.set(devices);
                        },
                        Err(e) => {
                            log::error!(
                                "Failed to fetch devices for node {}: {}",
                                node_id_clone,
                                e
                            );
                            state
                                .error
                                .set(Some(format!("Failed to fetch devices: {}", e)));
                        },
                    }
                });
            }
        });
    }

    // Node change handler
    let on_node_change = move |ev: web_sys::Event| {
        if let Some(target) = ev.target() {
            if let Ok(select) = target.dyn_into::<web_sys::HtmlSelectElement>() {
                let new_node_id = select.value();
                log::info!("Node changed to: {}", new_node_id);
                selected_node_id.set(new_node_id);
            }
        }
    };

    // All attached devices (unified - not separated by input/output)
    let attached_devices = {
        let state = app_state.clone();
        move || {
            state
                .devices
                .get()
                .into_iter()
                .filter(|d| matches!(d.status, DeviceStatus::Attached | DeviceStatus::Active))
                .collect::<Vec<_>>()
        }
    };

    // All available devices (unified - not separated by input/output)
    let available_devices = {
        let state = app_state.clone();
        move || {
            state
                .devices
                .get()
                .into_iter()
                .filter(|d| matches!(d.status, DeviceStatus::Available | DeviceStatus::Detached))
                .collect::<Vec<_>>()
        }
    };

    // Clone closures for For and conditional use
    let attached_devices_for = attached_devices.clone();
    let available_devices_for = available_devices.clone();

    // Clone selected_node for the dialog
    let selected_node_for_dialog = selected_node.clone();

    // Get current node ID for comparison
    let current_node_id = {
        let state = app_state.clone();
        move || state.current_node.get().map(|n| n.id).unwrap_or_default()
    };

    // Get nodes list for selector with "(this device)" indicator
    let nodes_for_select = {
        let state = app_state.clone();
        move || state.nodes.get()
    };

    // Callback to refresh devices
    let refresh_devices = {
        let state = app_state.clone();
        move || {
            let node_id = selected_node_id.get();
            if !node_id.is_empty() {
                let state = state.clone();
                let node_id_clone = node_id.clone();
                let node_id_encoded = urlencoding::encode(&node_id).to_string();
                spawn_local(async move {
                    log::info!("Refreshing devices for node: {}", node_id_clone);
                    if let Ok(devices) = api::get_devices(&node_id_encoded).await {
                        log::info!("Refreshed {} devices", devices.len());
                        state.devices.set(devices);
                    }
                });
            }
        }
    };

    view! {
        // Virtual Device Dialog
        {move || {
            if show_virtual_dialog.get() {
                let node = selected_node_for_dialog();
                let refresh = refresh_devices.clone();
                Some(view! {
                    <VirtualDeviceDialog
                        node_id=node
                        on_close=Callback::new(move |()| {
                            show_virtual_dialog.set(false);
                            refresh();
                        })
                    />
                })
            } else {
                None
            }
        }}

        <div class="device-list">
            // Node Selector - manage devices on ANY node
            <div class="node-selector device-node-selector">
                <label>"Manage Node: "</label>
                <select on:change=on_node_change>
                    <For
                        each=nodes_for_select
                        key=|node| node.id.clone()
                        children=move |node| {
                            let node_id = node.id.clone();
                            let is_current = current_node_id() == node_id;
                            let display_name = if is_current {
                                format!("{} (this device)", node.name)
                            } else {
                                node.name.clone()
                            };
                            let is_selected = {
                                let selected = selected_node_id.get();
                                selected == node_id || (selected.is_empty() && is_current)
                            };
                            view! {
                                <option value={node_id.clone()} selected=is_selected>
                                    {display_name}
                                </option>
                            }
                        }
                    />
                </select>
            </div>

            // Create Virtual Device Button
            <div class="device-actions">
                <button
                    class="create-virtual-btn"
                    on:click=move |_| show_virtual_dialog.set(true)
                >
                    "+ Create Virtual Device"
                </button>
            </div>

            // Attached Devices Section - UNIFIED (not separated by type)
            <section class="device-section">
                <h2>"Attached Devices"</h2>
                <div class="device-grid">
                    <For
                        each=attached_devices_for
                        key=|d| d.id.clone()
                        children=move |device| {
                            view! { <DeviceCard device=device /> }
                        }
                    />
                </div>
                {move || {
                    if attached_devices().is_empty() {
                        Some(view! {
                            <p class="empty-message">"No devices attached. Attach a device from the Available section below."</p>
                        })
                    } else {
                        None
                    }
                }}
            </section>

            // Available Devices Section - UNIFIED (not separated by type)
            <section class="device-section available-section">
                <h2>"Available Devices"</h2>
                <div class="device-grid">
                    <For
                        each=available_devices_for
                        key=|d| d.id.clone()
                        children=move |device| {
                            view! { <DeviceCard device=device /> }
                        }
                    />
                </div>
                {move || {
                    if available_devices().is_empty() {
                        Some(view! {
                            <p class="empty-message">"No available devices."</p>
                        })
                    } else {
                        None
                    }
                }}
            </section>
        </div>
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn device_list_compiles() {
        // Compilation test
    }
}
