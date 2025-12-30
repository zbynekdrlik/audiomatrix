//! Device configuration dialog component.
//!
//! Allows configuring device sample rate and buffer size.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::state::AppState;

/// Supported sample rates.
const SAMPLE_RATES: &[u32] = &[44100, 48000, 96000];

/// Supported buffer sizes.
const BUFFER_SIZES: &[u32] = &[32, 64, 128, 256, 512, 1024, 2048];

/// Device configuration dialog.
///
/// Allows editing sample rate and buffer size for an audio device.
#[component]
pub fn DeviceConfigDialog(
    /// Node ID for the device.
    node_id: String,
    /// Device ID.
    device_id: String,
    /// Device display name for the header.
    device_name: String,
    /// Current sample rate.
    current_sample_rate: u32,
    /// Current buffer size.
    current_buffer_size: u32,
    /// Callback when dialog is closed.
    on_close: Callback<()>,
) -> impl IntoView {
    let app_state = expect_context::<AppState>();

    // State
    let sample_rate = RwSignal::new(current_sample_rate);
    let buffer_size = RwSignal::new(current_buffer_size);
    let saving = RwSignal::new(false);

    // Save handler
    let on_save = {
        let node_id = node_id.clone();
        let device_id = device_id.clone();
        let app_state = app_state.clone();
        let on_close = on_close.clone();
        move |_| {
            let node_id = node_id.clone();
            let device_id = device_id.clone();
            let app_state = app_state.clone();
            let on_close = on_close.clone();
            let new_sample_rate = sample_rate.get();
            let new_buffer_size = buffer_size.get();

            saving.set(true);

            spawn_local(async move {
                match api::update_device(
                    &node_id,
                    &device_id,
                    None,
                    Some(new_sample_rate),
                    Some(new_buffer_size),
                )
                .await
                {
                    Ok(updated) => {
                        // Update device in state
                        app_state.devices.update(|devices| {
                            if let Some(d) = devices.iter_mut().find(|d| d.id == updated.id) {
                                *d = updated;
                            }
                        });
                        on_close.run(());
                    },
                    Err(e) => {
                        saving.set(false);
                        app_state
                            .error
                            .set(Some(format!("Failed to save config: {}", e)));
                    },
                }
            });
        }
    };

    // Close handler
    let on_close_click = {
        let on_close = on_close.clone();
        move |_| {
            on_close.run(());
        }
    };

    // Backdrop close
    let on_backdrop_click = {
        let on_close = on_close.clone();
        move |_| {
            on_close.run(());
        }
    };

    // Sample rate change handler
    let on_sample_rate_change = move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let select: web_sys::HtmlSelectElement = target.dyn_into().unwrap();
        if let Ok(value) = select.value().parse::<u32>() {
            sample_rate.set(value);
        }
    };

    // Buffer size change handler
    let on_buffer_size_change = move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let select: web_sys::HtmlSelectElement = target.dyn_into().unwrap();
        if let Ok(value) = select.value().parse::<u32>() {
            buffer_size.set(value);
        }
    };

    view! {
        <div class="config-dialog-backdrop" on:click=on_backdrop_click></div>
        <div class="config-dialog">
            <div class="config-dialog-header">
                <h2>"Device Configuration: " {device_name}</h2>
                <button class="config-dialog-close" on:click=on_close_click>"×"</button>
            </div>

            <div class="config-dialog-content">
                <div class="config-row">
                    <label for="sample-rate">"Sample Rate"</label>
                    <select id="sample-rate" on:change=on_sample_rate_change>
                        {SAMPLE_RATES.iter().map(|rate| {
                            let selected = *rate == current_sample_rate;
                            view! {
                                <option value=rate.to_string() selected=selected>
                                    {format!("{} Hz", rate)}
                                </option>
                            }
                        }).collect::<Vec<_>>()}
                    </select>
                </div>

                <div class="config-row">
                    <label for="buffer-size">"Buffer Size"</label>
                    <select id="buffer-size" on:change=on_buffer_size_change>
                        {BUFFER_SIZES.iter().map(|size| {
                            let selected = *size == current_buffer_size;
                            // Calculate latency in ms at 48kHz
                            let latency_ms = (*size as f32 / 48000.0) * 1000.0;
                            view! {
                                <option value=size.to_string() selected=selected>
                                    {format!("{} samples ({:.2} ms)", size, latency_ms)}
                                </option>
                            }
                        }).collect::<Vec<_>>()}
                    </select>
                </div>
            </div>

            <div class="config-dialog-footer">
                <button
                    class="config-btn cancel-btn"
                    on:click=move |_| on_close.run(())
                >
                    "Cancel"
                </button>
                <button
                    class="config-btn save-btn"
                    on:click=on_save.clone()
                    disabled=move || saving.get()
                >
                    {move || if saving.get() { "Saving..." } else { "Save" }}
                </button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn device_config_dialog_compiles() {
        // Compilation test
    }
}
