//! Routing matrix component.
//!
//! The main view showing inputs on the left, outputs on top,
//! and route cells at intersections. Users select source and
//! destination nodes and devices to work with a manageable matrix.

use leptos::prelude::*;
use ram_api::models::{DeviceInfo, NodeInfo};
use wasm_bindgen_futures::spawn_local;

use super::RouteCell;
use crate::api;
use crate::state::AppState;

/// Channel info tuple: (device_id, device_name, channel_number)
type ChannelInfo = (String, String, u8);

/// Main routing matrix grid with node and device selection.
#[component]
pub fn RoutingMatrix() -> impl IntoView {
    let app_state = expect_context::<AppState>();

    // Selected devices for the matrix
    let selected_input = RwSignal::new(None::<String>);
    let selected_output = RwSignal::new(None::<String>);

    // Clones for different closures
    let app_state_src_devices = app_state.clone();
    let app_state_dst_devices = app_state.clone();
    let app_state_nodes = app_state.clone();
    let app_state_matrix = app_state.clone();

    // Get available nodes
    let nodes = Signal::derive(move || app_state_nodes.nodes.get());

    // Get available devices based on selected source/dest nodes
    let input_devices = Signal::derive(move || {
        // If source_node is set, use source_devices, otherwise use local devices
        if app_state_src_devices.source_node.get().is_some() {
            app_state_src_devices.source_input_devices()
        } else {
            app_state_src_devices.input_devices()
        }
    });

    let output_devices = Signal::derive(move || {
        // If dest_node is set, use dest_devices, otherwise use local devices
        if app_state_dst_devices.dest_node.get().is_some() {
            app_state_dst_devices.dest_output_devices()
        } else {
            app_state_dst_devices.output_devices()
        }
    });

    // Auto-select first device when devices change
    let app_state_effect = app_state.clone();
    Effect::new(move || {
        let inputs = input_devices.get();
        let outputs = output_devices.get();

        if selected_input.get().is_none()
            || !inputs
                .iter()
                .any(|d| Some(&d.id) == selected_input.get().as_ref())
        {
            if let Some(first) = inputs.first() {
                selected_input.set(Some(first.id.clone()));
            }
        }
        if selected_output.get().is_none()
            || !outputs
                .iter()
                .any(|d| Some(&d.id) == selected_output.get().as_ref())
        {
            if let Some(first) = outputs.first() {
                selected_output.set(Some(first.id.clone()));
            }
        }
        // Silence unused warning
        let _ = &app_state_effect;
    });

    // Get channels for selected input device
    let input_channels: Signal<Vec<ChannelInfo>> = Signal::derive(move || {
        let sel_id = selected_input.get();
        input_devices
            .get()
            .into_iter()
            .filter(|d| sel_id.as_ref() == Some(&d.id))
            .flat_map(|d| {
                let id = d.id.clone();
                let name = d.name.clone();
                (1..=d.channels).map(move |ch| (id.clone(), name.clone(), ch))
            })
            .collect()
    });

    // Get channels for selected output device
    let output_channels: Signal<Vec<ChannelInfo>> = Signal::derive(move || {
        let sel_id = selected_output.get();
        output_devices
            .get()
            .into_iter()
            .filter(|d| sel_id.as_ref() == Some(&d.id))
            .flat_map(|d| {
                let id = d.id.clone();
                let name = d.name.clone();
                (1..=d.channels).map(move |ch| (id.clone(), name.clone(), ch))
            })
            .collect()
    });

    // Get node IDs for route creation
    let app_state_src_node = app_state_matrix.clone();
    let app_state_dst_node = app_state_matrix.clone();
    let source_node_id = Signal::derive(move || app_state_src_node.source_node_id());
    let dest_node_id = Signal::derive(move || app_state_dst_node.dest_node_id());

    // Clone for header
    let output_channels_header = Signal::derive(move || output_channels.get());

    // Node selector helper
    fn node_option(node: &NodeInfo, selected: Option<&String>, is_local: bool) -> impl IntoView {
        let is_selected = if is_local {
            selected.is_none()
        } else {
            selected == Some(&node.id)
        };
        let id = node.id.clone();
        let label = if is_local {
            format!("{} (local)", node.name)
        } else if node.online {
            node.name.clone()
        } else {
            format!("{} (offline)", node.name)
        };
        view! {
            <option value=id selected=is_selected>
                {label}
            </option>
        }
    }

    // Device selector helper
    fn device_option(device: &DeviceInfo, selected: Option<&String>) -> impl IntoView {
        let is_selected = selected == Some(&device.id);
        let id = device.id.clone();
        let label = format!("{} ({} ch)", device.name, device.channels);
        view! {
            <option value=id selected=is_selected>
                {label}
            </option>
        }
    }

    // Handlers for node selection
    let app_state_src_handler = app_state.clone();
    let on_source_node_change = move |ev: web_sys::Event| {
        let value = event_target_value(&ev);
        if value.is_empty() {
            // Selected local node
            app_state_src_handler.source_node.set(None);
            app_state_src_handler.source_devices.set(Vec::new());
        } else {
            // Selected remote node - fetch its devices
            if let Some(node) = app_state_src_handler
                .nodes
                .get()
                .into_iter()
                .find(|n| n.id == value)
            {
                app_state_src_handler.source_node.set(Some(node.clone()));
                let state = app_state_src_handler.clone();
                let node_id = urlencoding::encode(&node.id).to_string();
                spawn_local(async move {
                    match api::get_devices(&node_id).await {
                        Ok(devices) => {
                            state.source_devices.set(devices);
                        },
                        Err(e) => {
                            log::error!("Failed to fetch source devices: {}", e);
                            state
                                .error
                                .set(Some(format!("Failed to load devices: {}", e)));
                        },
                    }
                });
            }
        }
    };

    let app_state_dst_handler = app_state.clone();
    let on_dest_node_change = move |ev: web_sys::Event| {
        let value = event_target_value(&ev);
        if value.is_empty() {
            // Selected local node
            app_state_dst_handler.dest_node.set(None);
            app_state_dst_handler.dest_devices.set(Vec::new());
        } else {
            // Selected remote node - fetch its devices
            if let Some(node) = app_state_dst_handler
                .nodes
                .get()
                .into_iter()
                .find(|n| n.id == value)
            {
                app_state_dst_handler.dest_node.set(Some(node.clone()));
                let state = app_state_dst_handler.clone();
                let node_id = urlencoding::encode(&node.id).to_string();
                spawn_local(async move {
                    match api::get_devices(&node_id).await {
                        Ok(devices) => {
                            state.dest_devices.set(devices);
                        },
                        Err(e) => {
                            log::error!("Failed to fetch dest devices: {}", e);
                            state
                                .error
                                .set(Some(format!("Failed to load devices: {}", e)));
                        },
                    }
                });
            }
        }
    };

    view! {
        <div class="routing-matrix-container">
            // Node and device selection bar
            <div class="device-selectors">
                // Source node and device
                <div class="node-device-group">
                    <div class="node-selector">
                        <label>"Source Node:"</label>
                        <select on:change=on_source_node_change>
                            <option value="">"Local"</option>
                            {move || nodes.get().iter()
                                .filter(|n| Some(&n.id) != app_state.current_node.get().as_ref().map(|c| &c.id))
                                .map(|n| {
                                    node_option(n, app_state.source_node.get().as_ref().map(|s| &s.id), false)
                                }).collect_view()}
                        </select>
                    </div>
                    <div class="device-selector">
                        <label>"Source Device:"</label>
                        <select on:change=move |ev| {
                            let value = event_target_value(&ev);
                            selected_input.set(if value.is_empty() { None } else { Some(value) });
                        }>
                            <option value="">"-- Select Input --"</option>
                            {move || input_devices.get().iter().map(|d| {
                                device_option(d, selected_input.get().as_ref())
                            }).collect_view()}
                        </select>
                    </div>
                </div>

                // Destination node and device
                <div class="node-device-group">
                    <div class="node-selector">
                        <label>"Dest Node:"</label>
                        <select on:change=on_dest_node_change>
                            <option value="">"Local"</option>
                            {move || nodes.get().iter()
                                .filter(|n| Some(&n.id) != app_state.current_node.get().as_ref().map(|c| &c.id))
                                .map(|n| {
                                    node_option(n, app_state.dest_node.get().as_ref().map(|s| &s.id), false)
                                }).collect_view()}
                        </select>
                    </div>
                    <div class="device-selector">
                        <label>"Dest Device:"</label>
                        <select on:change=move |ev| {
                            let value = event_target_value(&ev);
                            selected_output.set(if value.is_empty() { None } else { Some(value) });
                        }>
                            <option value="">"-- Select Output --"</option>
                            {move || output_devices.get().iter().map(|d| {
                                device_option(d, selected_output.get().as_ref())
                            }).collect_view()}
                        </select>
                    </div>
                </div>
            </div>

            // Matrix info bar
            <div class="matrix-info">
                {move || {
                    let in_ch = input_channels.get().len();
                    let out_ch = output_channels.get().len();
                    let src = source_node_id.get();
                    let dst = dest_node_id.get();
                    format!("Matrix: {} inputs × {} outputs ({} → {})", in_ch, out_ch, src, dst)
                }}
            </div>

            // The routing matrix
            <div class="routing-matrix">
                // Header row with output channels
                <div class="matrix-header">
                    <div class="matrix-corner"></div>
                    <For
                        each=move || output_channels_header.get()
                        key=|(id, _, ch)| format!("hdr-{}:{}", id, ch)
                        children=move |(_, _, ch)| {
                            view! {
                                <div class="matrix-col-header">
                                    {ch}
                                </div>
                            }
                        }
                    />
                </div>

                // Matrix body with input rows
                <div class="matrix-body">
                    <For
                        each=move || input_channels.get()
                        key=|(id, _, ch)| format!("row-{}:{}", id, ch)
                        children=move |(src_id, _, src_ch)| {
                            let outputs = output_channels.get();
                            let src_node = source_node_id.get();
                            let dst_node = dest_node_id.get();
                            view! {
                                <div class="matrix-row">
                                    <div class="matrix-row-header">
                                        <span class="row-channel">{src_ch}</span>
                                    </div>
                                    {outputs
                                        .into_iter()
                                        .map(|(dst_id, _, dst_ch)| {
                                            view! {
                                                <RouteCell
                                                    source_node=src_node.clone()
                                                    source_device=src_id.clone()
                                                    source_channel=src_ch as u16
                                                    dest_node=dst_node.clone()
                                                    dest_device=dst_id
                                                    dest_channel=dst_ch as u16
                                                />
                                            }
                                        })
                                        .collect_view()}
                                </div>
                            }
                        }
                    />
                </div>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn routing_matrix_compiles() {
        // Compilation test
    }
}
