//! Route control popover for adjusting volume and mute.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::state::{AppState, RouteWithId};

/// Route control popover component.
///
/// Shows a popover with volume slider, mute toggle, and delete button.
#[component]
pub fn RouteControl(
    /// The route to control.
    route: RouteWithId,
    /// Position X (from mouse event).
    x: i32,
    /// Position Y (from mouse event).
    y: i32,
    /// Callback to close the popover.
    on_close: Callback<()>,
) -> impl IntoView {
    let app_state = expect_context::<AppState>();

    // Local state for volume and mute
    let volume = RwSignal::new(route.volume);
    let muted = RwSignal::new(route.muted);

    // Clone route data for closures
    let route_id = route.id.clone();
    let route_id_delete = route.id.clone();
    let source_node = route.source_node.clone();
    let source_device = route.source_device.clone();
    let source_channel = route.source_channel;
    let dest_node = route.destination_node.clone();
    let dest_device = route.destination_device.clone();
    let dest_channel = route.destination_channel;

    // Convert volume to dB for display
    let volume_db = move || {
        let v = volume.get();
        if v < 0.001 {
            "-inf".to_string()
        } else {
            format!("{:.1} dB", 20.0 * v.log10())
        }
    };

    // Handle volume change
    let on_volume_change = {
        let app_state = app_state.clone();
        let route_id = route_id.clone();
        let source_node = source_node.clone();
        let source_device = source_device.clone();
        let dest_node = dest_node.clone();
        let dest_device = dest_device.clone();
        move |ev: web_sys::Event| {
            let Some(target) = ev.target() else { return };
            let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() else { return };
            let new_volume: f32 = input.value().parse().unwrap_or(1.0);
            volume.set(new_volume);

            // Update route via API
            let route_id = route_id.clone();
            let route_def = ram_api::models::RouteDefinition {
                source_node: source_node.clone(),
                source_device: source_device.clone(),
                source_channel,
                destination_node: dest_node.clone(),
                destination_device: dest_device.clone(),
                destination_channel: dest_channel,
                volume: new_volume,
                muted: muted.get(),
            };
            let app_state = app_state.clone();
            spawn_local(async move {
                match api::update_route(&route_id, &route_def).await {
                    Ok(()) => {
                        app_state.upsert_route(route_def);
                    },
                    Err(e) => {
                        log::error!("Failed to update route: {}", e);
                        app_state
                            .error
                            .set(Some(format!("Failed to update volume: {}", e)));
                    },
                }
            });
        }
    };

    // Handle mute toggle
    let on_mute_toggle = {
        let app_state = app_state.clone();
        let route_id = route_id.clone();
        let source_node = source_node.clone();
        let source_device = source_device.clone();
        let dest_node = dest_node.clone();
        let dest_device = dest_device.clone();
        move |_| {
            let new_muted = !muted.get();
            muted.set(new_muted);

            // Update route via API
            let route_id = route_id.clone();
            let route_def = ram_api::models::RouteDefinition {
                source_node: source_node.clone(),
                source_device: source_device.clone(),
                source_channel,
                destination_node: dest_node.clone(),
                destination_device: dest_device.clone(),
                destination_channel: dest_channel,
                volume: volume.get(),
                muted: new_muted,
            };
            let app_state = app_state.clone();
            spawn_local(async move {
                match api::update_route(&route_id, &route_def).await {
                    Ok(()) => {
                        app_state.upsert_route(route_def);
                    },
                    Err(e) => {
                        log::error!("Failed to update route: {}", e);
                        app_state
                            .error
                            .set(Some(format!("Failed to update mute: {}", e)));
                    },
                }
            });
        }
    };

    // Handle delete
    let on_delete = {
        let app_state = app_state.clone();
        let on_close = on_close.clone();
        move |_| {
            let route_id = route_id_delete.clone();
            let app_state = app_state.clone();
            let on_close = on_close.clone();
            spawn_local(async move {
                if let Err(e) = api::delete_route(&route_id).await {
                    log::error!("Failed to delete route: {}", e);
                    app_state
                        .error
                        .set(Some(format!("Failed to delete route: {}", e)));
                } else {
                    app_state.remove_route(&route_id);
                    on_close.run(());
                }
            });
        }
    };

    // Handle close
    let on_close_click = {
        let on_close = on_close.clone();
        move |_| {
            on_close.run(());
        }
    };

    // Handle backdrop click (close)
    let on_backdrop_click = {
        let on_close = on_close.clone();
        move |_| {
            on_close.run(());
        }
    };

    // Mute button class
    let mute_btn_class = move || {
        if muted.get() {
            "route-control-btn mute-btn muted"
        } else {
            "route-control-btn mute-btn"
        }
    };

    // Position style
    let position_style = format!("left: {}px; top: {}px;", x, y);

    // Route title (short form)
    let title = format!(
        "{}:{} -> {}:{}",
        source_device, source_channel, dest_device, dest_channel
    );

    view! {
        <div class="route-control-backdrop" on:click=on_backdrop_click></div>
        <div class="route-control-popover" style=position_style>
            <div class="route-control-header">
                <span class="route-control-title">{title}</span>
                <button class="route-control-close" on:click=on_close_click>"×"</button>
            </div>

            <div class="route-control-section">
                <div class="route-control-label">
                    <span>"Volume"</span>
                    <span class="volume-value">{volume_db}</span>
                </div>
                <input
                    type="range"
                    class="volume-slider"
                    min="0"
                    max="1"
                    step="0.01"
                    prop:value=move || volume.get()
                    on:input=on_volume_change
                />
            </div>

            <div class="route-control-actions">
                <button
                    class=mute_btn_class
                    on:click=on_mute_toggle
                >
                    {move || if muted.get() { "Unmute" } else { "Mute" }}
                </button>
                <button
                    class="route-control-btn delete-btn"
                    on:click=on_delete
                >
                    "Delete"
                </button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn route_control_compiles() {
        // Compilation test
    }
}
