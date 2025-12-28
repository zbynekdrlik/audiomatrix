//! Device card component.

use leptos::prelude::*;
use ram_api::models::DeviceInfo;

use super::Meter;
use crate::state::{AppState, ChannelLevel};

/// Card displaying a single audio device with meters.
#[component]
pub fn DeviceCard(
    /// The device to display.
    device: DeviceInfo,
    /// Whether this is an input device.
    #[prop(default = true)]
    is_input: bool,
) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let device_id = device.id.clone();
    let device_name = device.name.clone();
    let channel_count = device.channels;
    let sample_rate = device.sample_rate;

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

    view! {
        <div class=format!("device-card {}", device_type_class)>
            <div class="device-header">
                <span class="device-name">{device_name}</span>
                <span class="device-channels">{channel_count}" ch"</span>
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
                <span class="device-sample-rate">{sample_rate}" Hz"</span>
            </div>
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
