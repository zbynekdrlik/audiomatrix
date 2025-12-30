//! Virtual device creation dialog component.
//!
//! Allows creating new virtual ASIO devices.

use leptos::prelude::*;
use ram_api::models::CreateVirtualDeviceRequest;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::state::AppState;

/// Virtual device creation dialog.
#[component]
pub fn VirtualDeviceDialog(
    /// Node ID to create the device on.
    node_id: String,
    /// Callback when dialog is closed.
    on_close: Callback<()>,
) -> impl IntoView {
    let app_state = expect_context::<AppState>();

    // Form state
    let name = RwSignal::new(String::new());
    let input_channels = RwSignal::new(2u16);
    let output_channels = RwSignal::new(2u16);
    let sample_rate = RwSignal::new(48000u32);
    let buffer_size = RwSignal::new(256u32);
    let creating = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    // Validation
    let is_valid = move || {
        let n = name.get();
        !n.is_empty()
            && n.len() <= 31
            && (input_channels.get() > 0 || output_channels.get() > 0)
            && input_channels.get() <= 256
            && output_channels.get() <= 256
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

    // Input handlers
    let on_name_input = move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let input: web_sys::HtmlInputElement = target.dyn_into().unwrap();
        name.set(input.value());
    };

    let on_input_channels_input = move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let input: web_sys::HtmlInputElement = target.dyn_into().unwrap();
        if let Ok(v) = input.value().parse::<u16>() {
            input_channels.set(v.min(256));
        }
    };

    let on_output_channels_input = move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let input: web_sys::HtmlInputElement = target.dyn_into().unwrap();
        if let Ok(v) = input.value().parse::<u16>() {
            output_channels.set(v.min(256));
        }
    };

    let on_sample_rate_change = move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let select: web_sys::HtmlSelectElement = target.dyn_into().unwrap();
        if let Ok(v) = select.value().parse::<u32>() {
            sample_rate.set(v);
        }
    };

    let on_buffer_size_change = move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let select: web_sys::HtmlSelectElement = target.dyn_into().unwrap();
        if let Ok(v) = select.value().parse::<u32>() {
            buffer_size.set(v);
        }
    };

    // Create handler
    let on_create = {
        let node_id = node_id.clone();
        let on_close = on_close.clone();
        let app_state = app_state.clone();
        move |_| {
            if !is_valid() || creating.get() {
                return;
            }

            creating.set(true);
            error.set(None);

            let request = CreateVirtualDeviceRequest {
                name: name.get(),
                input_channels: input_channels.get(),
                output_channels: output_channels.get(),
                sample_rate: sample_rate.get(),
                buffer_size: buffer_size.get(),
            };

            let node_id = node_id.clone();
            let on_close = on_close.clone();
            let app_state = app_state.clone();

            spawn_local(async move {
                match api::create_virtual_device(&node_id, &request).await {
                    Ok(response) => {
                        if response.success {
                            // Refresh devices
                            if let Ok(devices) = api::get_devices(&node_id).await {
                                app_state.devices.set(devices);
                            }
                            on_close.run(());
                        } else {
                            let err_msg = response
                                .error
                                .unwrap_or_else(|| "Unknown error".to_string());
                            error.set(Some(format!("Failed to create device: {}", err_msg)));
                            creating.set(false);
                        }
                    },
                    Err(e) => {
                        error.set(Some(format!("Failed to create device: {}", e)));
                        creating.set(false);
                    },
                }
            });
        }
    };

    view! {
        <div class="virtual-device-backdrop" on:click=on_backdrop_click></div>
        <div class="virtual-device-dialog">
            <div class="virtual-device-header">
                <h2>"Create Virtual Device"</h2>
                <button class="virtual-device-close" on:click=on_close_click>"×"</button>
            </div>

            <div class="virtual-device-content">
                {move || {
                    error.get().map(|e| view! {
                        <div class="virtual-device-error">{e}</div>
                    })
                }}

                <div class="form-group">
                    <label for="device-name">"Device Name"</label>
                    <input
                        type="text"
                        id="device-name"
                        class="form-input"
                        placeholder="My Virtual Device"
                        maxlength="31"
                        prop:value=move || name.get()
                        on:input=on_name_input
                    />
                    <span class="form-hint">"Max 31 characters"</span>
                </div>

                <div class="form-row">
                    <div class="form-group">
                        <label for="input-channels">"Input Channels"</label>
                        <input
                            type="number"
                            id="input-channels"
                            class="form-input"
                            min="0"
                            max="256"
                            prop:value=move || input_channels.get()
                            on:input=on_input_channels_input
                        />
                    </div>

                    <div class="form-group">
                        <label for="output-channels">"Output Channels"</label>
                        <input
                            type="number"
                            id="output-channels"
                            class="form-input"
                            min="0"
                            max="256"
                            prop:value=move || output_channels.get()
                            on:input=on_output_channels_input
                        />
                    </div>
                </div>

                <div class="form-row">
                    <div class="form-group">
                        <label for="sample-rate">"Sample Rate"</label>
                        <select
                            id="sample-rate"
                            class="form-select"
                            on:change=on_sample_rate_change
                        >
                            <option value="44100" selected=move || sample_rate.get() == 44100>"44100 Hz"</option>
                            <option value="48000" selected=move || sample_rate.get() == 48000>"48000 Hz"</option>
                            <option value="96000" selected=move || sample_rate.get() == 96000>"96000 Hz"</option>
                        </select>
                    </div>

                    <div class="form-group">
                        <label for="buffer-size">"Buffer Size"</label>
                        <select
                            id="buffer-size"
                            class="form-select"
                            on:change=on_buffer_size_change
                        >
                            <option value="64" selected=move || buffer_size.get() == 64>"64 samples"</option>
                            <option value="128" selected=move || buffer_size.get() == 128>"128 samples"</option>
                            <option value="256" selected=move || buffer_size.get() == 256>"256 samples"</option>
                            <option value="512" selected=move || buffer_size.get() == 512>"512 samples"</option>
                            <option value="1024" selected=move || buffer_size.get() == 1024>"1024 samples"</option>
                        </select>
                    </div>
                </div>
            </div>

            <div class="virtual-device-footer">
                <button
                    class="btn-cancel"
                    on:click=on_close_click.clone()
                    disabled=move || creating.get()
                >
                    "Cancel"
                </button>
                <button
                    class="btn-create"
                    on:click=on_create
                    disabled=move || !is_valid() || creating.get()
                >
                    {move || if creating.get() { "Creating..." } else { "Create Device" }}
                </button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn virtual_device_dialog_compiles() {
        // Compilation test
    }
}
