//! Routing matrix component.
//!
//! The main view showing inputs on the left, outputs on top,
//! and route cells at intersections. Users select source and
//! destination devices to work with a manageable matrix.

use leptos::prelude::*;
use ram_api::models::DeviceInfo;

use super::RouteCell;
use crate::state::AppState;

/// Channel info tuple: (device_id, device_name, channel_number)
type ChannelInfo = (String, String, u8);

/// Main routing matrix grid with device selection.
#[component]
pub fn RoutingMatrix() -> impl IntoView {
    let app_state = expect_context::<AppState>();

    // Selected devices for the matrix
    let selected_input = RwSignal::new(None::<String>);
    let selected_output = RwSignal::new(None::<String>);

    // Clones for different closures
    let app_state_inputs = app_state.clone();
    let app_state_outputs = app_state.clone();
    let app_state_matrix = app_state.clone();

    // Get available devices
    let input_devices = Signal::derive(move || app_state_inputs.input_devices());
    let output_devices = Signal::derive(move || app_state_outputs.output_devices());

    // Auto-select first device if none selected
    Effect::new(move || {
        if selected_input.get().is_none() {
            if let Some(first) = app_state.input_devices().first() {
                selected_input.set(Some(first.id.clone()));
            }
        }
        if selected_output.get().is_none() {
            if let Some(first) = app_state.output_devices().first() {
                selected_output.set(Some(first.id.clone()));
            }
        }
    });

    // Get channels for selected input device
    let input_channels: Signal<Vec<ChannelInfo>> = {
        let app_state = app_state_matrix.clone();
        Signal::derive(move || {
            let sel_id = selected_input.get();
            app_state
                .input_devices()
                .into_iter()
                .filter(|d| sel_id.as_ref() == Some(&d.id))
                .flat_map(|d| {
                    let id = d.id.clone();
                    let name = d.name.clone();
                    (1..=d.channels).map(move |ch| (id.clone(), name.clone(), ch))
                })
                .collect()
        })
    };

    // Get channels for selected output device
    let output_channels: Signal<Vec<ChannelInfo>> = {
        let app_state = app_state_matrix.clone();
        Signal::derive(move || {
            let sel_id = selected_output.get();
            app_state
                .output_devices()
                .into_iter()
                .filter(|d| sel_id.as_ref() == Some(&d.id))
                .flat_map(|d| {
                    let id = d.id.clone();
                    let name = d.name.clone();
                    (1..=d.channels).map(move |ch| (id.clone(), name.clone(), ch))
                })
                .collect()
        })
    };

    // Clone for header
    let output_channels_header = Signal::derive(move || output_channels.get());

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

    view! {
        <div class="routing-matrix-container">
            // Device selection bar
            <div class="device-selectors">
                <div class="device-selector">
                    <label>"Source:"</label>
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
                <div class="device-selector">
                    <label>"Destination:"</label>
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

            // Matrix info bar
            <div class="matrix-info">
                {move || {
                    let in_ch = input_channels.get().len();
                    let out_ch = output_channels.get().len();
                    format!("Matrix: {} inputs × {} outputs", in_ch, out_ch)
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
                                                    source_device=src_id.clone()
                                                    source_channel=src_ch as u16
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
