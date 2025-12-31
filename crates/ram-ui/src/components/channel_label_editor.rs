//! Channel label editor component.
//!
//! Allows editing channel labels for audio devices.

use leptos::prelude::*;
use ram_api::models::ChannelInfo;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::state::AppState;

/// A single channel row in the editor.
#[component]
fn ChannelRow(channel: ChannelInfo, node_id: String, device_id: String) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let channel_num = channel.number;
    let initial_label = channel.label.clone();

    // State
    let editing = RwSignal::new(false);
    let label_value = RwSignal::new(initial_label.clone().unwrap_or_default());
    let saved_label = RwSignal::new(initial_label.clone());

    // Display label
    let display_label = move || {
        saved_label
            .get()
            .unwrap_or_else(|| format!("Ch {}", channel_num))
    };

    // Start editing
    let start_edit = move |_| {
        label_value.set(saved_label.get().unwrap_or_default());
        editing.set(true);
    };

    // Cancel editing
    let cancel_edit = move |_| {
        editing.set(false);
    };

    // Save label
    let on_save = {
        let node_id = node_id.clone();
        let device_id = device_id.clone();
        let app_state = app_state.clone();
        move |_| {
            let label = label_value.get();
            let node_id = node_id.clone();
            let device_id = device_id.clone();
            let app_state = app_state.clone();

            spawn_local(async move {
                match api::update_channel_label(&node_id, &device_id, channel_num, &label).await {
                    Ok(updated) => {
                        saved_label.set(updated.label);
                        editing.set(false);
                    },
                    Err(e) => {
                        app_state
                            .error
                            .set(Some(format!("Failed to save label: {}", e)));
                    },
                }
            });
        }
    };

    // Input change handler
    let on_input = move |ev: web_sys::Event| {
        if let Some(target) = ev.target() {
            if let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() {
                label_value.set(input.value());
            }
        }
    };

    // Key handler
    let on_keydown = {
        let node_id = node_id.clone();
        let device_id = device_id.clone();
        let app_state = app_state.clone();
        move |ev: web_sys::KeyboardEvent| {
            if ev.key() == "Enter" {
                let label = label_value.get();
                let node_id = node_id.clone();
                let device_id = device_id.clone();
                let app_state = app_state.clone();

                spawn_local(async move {
                    match api::update_channel_label(&node_id, &device_id, channel_num, &label).await
                    {
                        Ok(updated) => {
                            saved_label.set(updated.label);
                            editing.set(false);
                        },
                        Err(e) => {
                            app_state
                                .error
                                .set(Some(format!("Failed to save label: {}", e)));
                        },
                    }
                });
            } else if ev.key() == "Escape" {
                editing.set(false);
            }
        }
    };

    view! {
        <tr>
            <td class="channel-number">{channel_num}</td>
            <td class="channel-label">
                {move || {
                    if editing.get() {
                        view! {
                            <input
                                type="text"
                                class="channel-label-input"
                                prop:value=move || label_value.get()
                                on:input=on_input.clone()
                                on:keydown=on_keydown.clone()
                            />
                        }.into_any()
                    } else {
                        view! {
                            <span class="label-text">{display_label}</span>
                        }.into_any()
                    }
                }}
            </td>
            <td class="channel-actions">
                {move || {
                    if editing.get() {
                        view! {
                            <button class="btn-save" on:click=on_save.clone()>"Save"</button>
                            <button class="btn-cancel" on:click=cancel_edit>"Cancel"</button>
                        }.into_any()
                    } else {
                        view! {
                            <button class="btn-edit" on:click=start_edit>"Edit"</button>
                        }.into_any()
                    }
                }}
            </td>
        </tr>
    }
}

/// Channel label editor component.
///
/// Shows a list of channels with editable labels.
#[component]
pub fn ChannelLabelEditor(
    /// Node ID for the device.
    node_id: String,
    /// Device ID.
    device_id: String,
    /// Device display name for the header.
    device_name: String,
    /// Callback when editor is closed.
    on_close: Callback<()>,
) -> impl IntoView {
    // State for channels
    let channels = RwSignal::new(Vec::<ChannelInfo>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);

    // Load channels on mount
    {
        let node_id = node_id.clone();
        let device_id = device_id.clone();
        spawn_local(async move {
            match api::get_device_channels(&node_id, &device_id).await {
                Ok(ch) => {
                    channels.set(ch);
                    loading.set(false);
                },
                Err(e) => {
                    error.set(Some(format!("Failed to load channels: {}", e)));
                    loading.set(false);
                },
            }
        });
    }

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

    view! {
        <div class="channel-editor-backdrop" on:click=on_backdrop_click></div>
        <div class="channel-editor-dialog">
            <div class="channel-editor-header">
                <h2>"Channel Labels: " {device_name}</h2>
                <button class="channel-editor-close" on:click=on_close_click>"×"</button>
            </div>

            <div class="channel-editor-content">
                {move || {
                    let node_id = node_id.clone();
                    let device_id = device_id.clone();
                    if loading.get() {
                        view! { <p class="loading">"Loading channels..."</p> }.into_any()
                    } else if let Some(err) = error.get() {
                        view! { <p class="error">{err}</p> }.into_any()
                    } else {
                        view! {
                            <table class="channel-table">
                                <thead>
                                    <tr>
                                        <th>"Ch"</th>
                                        <th>"Label"</th>
                                        <th>"Actions"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {channels.get().into_iter().map(|ch| {
                                        let node_id = node_id.clone();
                                        let device_id = device_id.clone();
                                        view! {
                                            <ChannelRow
                                                channel=ch
                                                node_id=node_id
                                                device_id=device_id
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
    fn channel_label_editor_compiles() {
        // Compilation test
    }
}
