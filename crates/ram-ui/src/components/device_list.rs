//! Device list component.

use leptos::prelude::*;
use ram_api::models::DeviceStatus;

use super::{DeviceCard, VirtualDeviceDialog};
use crate::state::AppState;

/// List of all audio devices grouped by type and attachment status.
#[component]
pub fn DeviceList() -> impl IntoView {
    let app_state = expect_context::<AppState>();

    // Dialog state
    let show_virtual_dialog = RwSignal::new(false);

    // Get current node for virtual device creation
    let app_state_node = app_state.clone();
    let selected_node = move || {
        app_state_node
            .current_node
            .get()
            .map(|n| n.id)
            .unwrap_or_else(|| "local".to_string())
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

    view! {
        // Virtual Device Dialog
        {move || {
            if show_virtual_dialog.get() {
                let node = selected_node_for_dialog();
                Some(view! {
                    <VirtualDeviceDialog
                        node_id=node
                        on_close=Callback::new(move |()| show_virtual_dialog.set(false))
                    />
                })
            } else {
                None
            }
        }}

        <div class="device-list">
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
