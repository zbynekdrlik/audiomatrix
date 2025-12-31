//! Generator control component.
//!
//! Allows controlling test signal generators on output devices.

use leptos::prelude::*;
use ram_api::models::{GeneratorStatus, SetGeneratorRequest, WaveformType};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::state::AppState;

/// Generator control for a single channel.
#[component]
fn ChannelGenerator(
    /// Node ID.
    node_id: String,
    /// Device ID.
    device_id: String,
    /// Channel number (1-based).
    channel: u16,
    /// Initial generator status.
    initial_status: GeneratorStatus,
) -> impl IntoView {
    let app_state = expect_context::<AppState>();

    // State
    let enabled = RwSignal::new(initial_status.enabled);
    let waveform = RwSignal::new(initial_status.waveform);
    let frequency = RwSignal::new(initial_status.frequency);
    let level_db = RwSignal::new(initial_status.level_db);
    let updating = RwSignal::new(false);

    // Update generator
    let update_generator = {
        let node_id = node_id.clone();
        let device_id = device_id.clone();
        let app_state = app_state.clone();
        move || {
            if updating.get() {
                return;
            }
            updating.set(true);

            let request = SetGeneratorRequest {
                enabled: enabled.get(),
                waveform: waveform.get(),
                frequency: frequency.get(),
                level_db: level_db.get(),
            };

            let node_id = node_id.clone();
            let device_id = device_id.clone();
            let app_state = app_state.clone();

            spawn_local(async move {
                if let Err(e) =
                    api::set_channel_generator(&node_id, &device_id, channel, &request).await
                {
                    app_state
                        .error
                        .set(Some(format!("Failed to update generator: {}", e)));
                }
                updating.set(false);
            });
        }
    };

    // Toggle enabled
    let on_toggle = {
        let update_generator = update_generator.clone();
        move |_| {
            enabled.update(|e| *e = !*e);
            update_generator();
        }
    };

    // Waveform change
    let on_waveform_change = {
        let update_generator = update_generator.clone();
        move |ev: web_sys::Event| {
            if let Some(target) = ev.target() {
                if let Ok(select) = target.dyn_into::<web_sys::HtmlSelectElement>() {
                    let new_waveform = match select.value().as_str() {
                        "sine" => WaveformType::Sine,
                        "pink_noise" => WaveformType::PinkNoise,
                        "channel_id" => WaveformType::ChannelId,
                        "sweep" => WaveformType::Sweep,
                        "click" => WaveformType::Click,
                        _ => WaveformType::Sine,
                    };
                    waveform.set(new_waveform);
                    update_generator();
                }
            }
        }
    };

    // Frequency change
    let on_frequency_change = {
        let update_generator = update_generator.clone();
        move |ev: web_sys::Event| {
            if let Some(target) = ev.target() {
                if let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() {
                    if let Ok(v) = input.value().parse::<u32>() {
                        frequency.set(v.clamp(20, 20000));
                        update_generator();
                    }
                }
            }
        }
    };

    // Level change
    let on_level_change = {
        let update_generator = update_generator.clone();
        move |ev: web_sys::Event| {
            if let Some(target) = ev.target() {
                if let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() {
                    if let Ok(v) = input.value().parse::<f32>() {
                        level_db.set(v.clamp(-60.0, 0.0));
                        update_generator();
                    }
                }
            }
        }
    };

    // Waveform value for select
    let waveform_value = move || match waveform.get() {
        WaveformType::Sine => "sine",
        WaveformType::PinkNoise => "pink_noise",
        WaveformType::ChannelId => "channel_id",
        WaveformType::Sweep => "sweep",
        WaveformType::Click => "click",
    };

    view! {
        <tr class="generator-row" class:enabled=move || enabled.get()>
            <td class="channel-num">{channel}</td>
            <td class="channel-toggle">
                <button
                    class="toggle-btn"
                    class:active=move || enabled.get()
                    on:click=on_toggle
                    disabled=move || updating.get()
                >
                    {move || if enabled.get() { "ON" } else { "OFF" }}
                </button>
            </td>
            <td class="channel-waveform">
                <select
                    class="waveform-select"
                    prop:value=waveform_value
                    on:change=on_waveform_change
                    disabled=move || !enabled.get() || updating.get()
                >
                    <option value="sine">"Sine"</option>
                    <option value="pink_noise">"Pink Noise"</option>
                    <option value="channel_id">"Channel ID"</option>
                    <option value="sweep">"Sweep"</option>
                    <option value="click">"Click"</option>
                </select>
            </td>
            <td class="channel-frequency">
                <input
                    type="number"
                    class="frequency-input"
                    min="20"
                    max="20000"
                    prop:value=move || frequency.get()
                    on:change=on_frequency_change
                    disabled=move || !enabled.get() || waveform.get() != WaveformType::Sine || updating.get()
                />
                <span class="unit">"Hz"</span>
            </td>
            <td class="channel-level">
                <input
                    type="range"
                    class="level-slider"
                    min="-60"
                    max="0"
                    step="1"
                    prop:value=move || level_db.get()
                    on:input=on_level_change
                    disabled=move || !enabled.get() || updating.get()
                />
                <span class="level-value">{move || format!("{:.0} dB", level_db.get())}</span>
            </td>
        </tr>
    }
}

/// Generator control panel for a device.
#[component]
pub fn GeneratorControl(
    /// Node ID.
    node_id: String,
    /// Device ID.
    device_id: String,
    /// Device display name.
    device_name: String,
    /// Callback when panel is closed.
    on_close: Callback<()>,
) -> impl IntoView {
    let app_state = expect_context::<AppState>();

    // State
    let generators = RwSignal::new(Vec::<GeneratorStatus>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);

    // Load generators on mount
    {
        let node_id = node_id.clone();
        let device_id = device_id.clone();
        spawn_local(async move {
            match api::get_device_generator(&node_id, &device_id).await {
                Ok(response) => {
                    generators.set(response.channels);
                    loading.set(false);
                },
                Err(e) => {
                    error.set(Some(format!("Failed to load generators: {}", e)));
                    loading.set(false);
                },
            }
        });
    }

    // Close handlers
    let on_close_click = {
        let on_close = on_close.clone();
        move |_| {
            on_close.run(());
        }
    };

    let on_backdrop_click = {
        let on_close = on_close.clone();
        move |_| {
            on_close.run(());
        }
    };

    // Enable all generators
    let on_enable_all = {
        let node_id = node_id.clone();
        let device_id = device_id.clone();
        let app_state = app_state.clone();
        move |_| {
            let node_id = node_id.clone();
            let device_id = device_id.clone();
            let app_state = app_state.clone();

            let request = SetGeneratorRequest {
                enabled: true,
                waveform: WaveformType::Sine,
                frequency: 1000,
                level_db: -18.0,
            };

            spawn_local(async move {
                match api::set_all_generators(&node_id, &device_id, &request).await {
                    Ok(()) => {
                        // Refresh
                        if let Ok(response) = api::get_device_generator(&node_id, &device_id).await
                        {
                            generators.set(response.channels);
                        }
                    },
                    Err(e) => {
                        app_state
                            .error
                            .set(Some(format!("Failed to enable generators: {}", e)));
                    },
                }
            });
        }
    };

    // Disable all generators
    let on_disable_all = {
        let node_id = node_id.clone();
        let device_id = device_id.clone();
        let app_state = app_state.clone();
        move |_| {
            let node_id = node_id.clone();
            let device_id = device_id.clone();
            let app_state = app_state.clone();

            let request = SetGeneratorRequest {
                enabled: false,
                waveform: WaveformType::Sine,
                frequency: 1000,
                level_db: -18.0,
            };

            spawn_local(async move {
                match api::set_all_generators(&node_id, &device_id, &request).await {
                    Ok(()) => {
                        // Refresh
                        if let Ok(response) = api::get_device_generator(&node_id, &device_id).await
                        {
                            generators.set(response.channels);
                        }
                    },
                    Err(e) => {
                        app_state
                            .error
                            .set(Some(format!("Failed to disable generators: {}", e)));
                    },
                }
            });
        }
    };

    view! {
        <div class="generator-backdrop" on:click=on_backdrop_click></div>
        <div class="generator-dialog">
            <div class="generator-header">
                <h2>"Generators: " {device_name}</h2>
                <button class="generator-close" on:click=on_close_click>"×"</button>
            </div>

            <div class="generator-actions">
                <button class="btn-enable-all" on:click=on_enable_all>"Enable All"</button>
                <button class="btn-disable-all" on:click=on_disable_all>"Disable All"</button>
            </div>

            <div class="generator-content">
                {move || {
                    let node_id = node_id.clone();
                    let device_id = device_id.clone();
                    if loading.get() {
                        view! { <p class="loading">"Loading generators..."</p> }.into_any()
                    } else if let Some(err) = error.get() {
                        view! { <p class="error">{err}</p> }.into_any()
                    } else {
                        view! {
                            <table class="generator-table">
                                <thead>
                                    <tr>
                                        <th>"Ch"</th>
                                        <th>"State"</th>
                                        <th>"Waveform"</th>
                                        <th>"Frequency"</th>
                                        <th>"Level"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {generators.get().into_iter().map(|gen| {
                                        let node_id = node_id.clone();
                                        let device_id = device_id.clone();
                                        view! {
                                            <ChannelGenerator
                                                node_id=node_id
                                                device_id=device_id
                                                channel=gen.channel
                                                initial_status=gen
                                            />
                                        }
                                    }).collect::<Vec<_>>()}
                                </tbody>
                            </table>
                        }.into_any()
                    }
                }}
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn generator_control_compiles() {
        // Compilation test
    }
}
