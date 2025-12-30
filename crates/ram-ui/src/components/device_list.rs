//! Device list component.

use leptos::prelude::*;
use ram_api::models::DeviceStatus;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use super::{DeviceCard, VirtualDeviceDialog};
use crate::api;
use crate::state::AppState;

/// List of all audio devices grouped by type and attachment status.
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
                let node_id_encoded = urlencoding::encode(&node_id).to_string();
                spawn_local(async move {
                    match api::get_devices(&node_id_encoded).await {
                        Ok(devices) => {
                            state.devices.set(devices);
                        }
                        Err(e) => {
                            log::error!("Failed to fetch devices for node {}: {}", node_id_encoded, e);
                        }
                    }
                });
            }
        });
    }

    // Node change handler
    let on_node_change = move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let select: web_sys::HtmlSelectElement = target.dyn_into().unwrap();
        let new_node_id = select.value();
        selected_node_id.set(new_node_id);
    };

    // Attached input devices
    let app_state_1 = app_state.clone();
    let attached_inputs = move || {
        app_state_1
            .input_devices()
            .into_iter()
            .filter(|d| matches!(d.status, DeviceStatus::Attached | DeviceStatus::Active))
            .collect::<Vec<_>>()
    };

    // Available input devices
    let app_state_2 = app_state.clone();
    let available_inputs = move || {
        app_state_2
            .input_devices()
            .into_iter()
            .filter(|d| matches!(d.status, DeviceStatus::Available | DeviceStatus::Detached))
            .collect::<Vec<_>>()
    };

    // Attached output devices
    let app_state_3 = app_state.clone();
    let attached_outputs = move || {
        app_state_3
            .output_devices()
            .into_iter()
            .filter(|d| matches!(d.status, DeviceStatus::Attached | DeviceStatus::Active))
            .collect::<Vec<_>>()
    };

    // Available output devices
    let app_state_4 = app_state.clone();
    let available_outputs = move || {
        app_state_4
            .output_devices()
            .into_iter()
            .filter(|d| matches!(d.status, DeviceStatus::Available | DeviceStatus::Detached))
            .collect::<Vec<_>>()
    };

    // Clone closures for For and conditional use
    let attached_inputs_for = attached_inputs.clone();
    let attached_outputs_for = attached_outputs.clone();
    let available_inputs_for = available_inputs.clone();
    let available_outputs_for = available_outputs.clone();

    // Clone selected_node for the dialog
    let selected_node_for_dialog = selected_node.clone();

    // Get nodes list for selector
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
                let node_id_encoded = urlencoding::encode(&node_id).to_string();
                spawn_local(async move {
                    if let Ok(devices) = api::get_devices(&node_id_encoded).await {
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
                            let node_name = node.name.clone();
                            let is_selected = {
                                let selected = selected_node_id.get();
                                selected == node_id || (selected.is_empty() && app_state.current_node.get().map(|n| n.id == node_id).unwrap_or(false))
                            };
                            view! {
                                <option value={node_id.clone()} selected=is_selected>
                                    {node_name}
                                </option>
                            }
                        }
                    />
                </select>
                <span class="node-hint">" (Select node to manage its devices)"</span>
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

            // Attached Devices Section
            <section class="device-section">
                <h2>"Attached Input Devices"</h2>
                <div class="device-grid">
                    <For
                        each=attached_inputs_for
                        key=|d| d.id.clone()
                        children=move |device| {
                            view! { <DeviceCard device=device is_input=true/> }
                        }
                    />
                </div>
                {move || {
                    if attached_inputs().is_empty() {
                        Some(view! {
                            <p class="empty-message">"No input devices attached. Attach a device from the Available section below."</p>
                        })
                    } else {
                        None
                    }
                }}
            </section>

            <section class="device-section">
                <h2>"Attached Output Devices"</h2>
                <div class="device-grid">
                    <For
                        each=attached_outputs_for
                        key=|d| d.id.clone()
                        children=move |device| {
                            view! { <DeviceCard device=device is_input=false/> }
                        }
                    />
                </div>
                {move || {
                    if attached_outputs().is_empty() {
                        Some(view! {
                            <p class="empty-message">"No output devices attached. Attach a device from the Available section below."</p>
                        })
                    } else {
                        None
                    }
                }}
            </section>

            // Available Devices Section
            <section class="device-section available-section">
                <h2>"Available Input Devices"</h2>
                <div class="device-grid">
                    <For
                        each=available_inputs_for
                        key=|d| d.id.clone()
                        children=move |device| {
                            view! { <DeviceCard device=device is_input=true/> }
                        }
                    />
                </div>
                {move || {
                    if available_inputs().is_empty() {
                        Some(view! {
                            <p class="empty-message">"No available input devices."</p>
                        })
                    } else {
                        None
                    }
                }}
            </section>

            <section class="device-section available-section">
                <h2>"Available Output Devices"</h2>
                <div class="device-grid">
                    <For
                        each=available_outputs_for
                        key=|d| d.id.clone()
                        children=move |device| {
                            view! { <DeviceCard device=device is_input=false/> }
                        }
                    />
                </div>
                {move || {
                    if available_outputs().is_empty() {
                        Some(view! {
                            <p class="empty-message">"No available output devices."</p>
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
