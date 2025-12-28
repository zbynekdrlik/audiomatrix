//! Routing matrix component.
//!
//! The main view showing inputs on the left, outputs on top,
//! and route cells at intersections.

use leptos::prelude::*;

use super::RouteCell;
use crate::state::AppState;

/// Channel info tuple: (device_id, device_name, channel_number)
type ChannelInfo = (String, String, u8);

/// Main routing matrix grid.
#[component]
pub fn RoutingMatrix() -> impl IntoView {
    let app_state = expect_context::<AppState>();

    let app_state_1 = app_state.clone();
    let app_state_2 = app_state.clone();
    let app_state_3 = app_state.clone();

    // Create signals for device lists
    let input_channels: Signal<Vec<ChannelInfo>> = Signal::derive(move || {
        app_state_1
            .input_devices()
            .into_iter()
            .flat_map(|d| {
                let id = d.id.clone();
                let name = d.name.clone();
                (1..=d.channels).map(move |ch| (id.clone(), name.clone(), ch))
            })
            .collect()
    });

    let output_channels: Signal<Vec<ChannelInfo>> = Signal::derive(move || {
        app_state_2
            .output_devices()
            .into_iter()
            .flat_map(|d| {
                let id = d.id.clone();
                let name = d.name.clone();
                (1..=d.channels).map(move |ch| (id.clone(), name.clone(), ch))
            })
            .collect()
    });

    // Clone for header
    let output_channels_header = Signal::derive(move || {
        app_state_3
            .output_devices()
            .into_iter()
            .flat_map(|d| {
                let id = d.id.clone();
                let name = d.name.clone();
                (1..=d.channels).map(move |ch| (id.clone(), name.clone(), ch))
            })
            .collect::<Vec<ChannelInfo>>()
    });

    view! {
        <div class="routing-matrix">
            // Header row with output channels
            <div class="matrix-header">
                <div class="matrix-corner"></div>
                <For
                    each=move || output_channels_header.get()
                    key=|(id, _, ch)| format!("hdr-{}:{}", id, ch)
                    children=move |(_, name, ch)| {
                        let title = format!("{} ch{}", name, ch);
                        view! {
                            <div class="matrix-col-header" title=title>
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
                    children=move |(src_id, src_name, src_ch)| {
                        let outputs = output_channels.get();
                        let title = format!("{} ch{}", src_name, src_ch);
                        let name_display = src_name.clone();
                        view! {
                            <div class="matrix-row">
                                <div class="matrix-row-header" title=title>
                                    <span class="row-device">{name_display}</span>
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
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn routing_matrix_compiles() {
        // Compilation test
    }
}
