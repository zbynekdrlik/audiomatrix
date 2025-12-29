//! Route cell component for the routing matrix.

use leptos::prelude::*;

use crate::api;
use crate::state::AppState;

/// A single cell in the routing matrix representing a potential route.
#[component]
pub fn RouteCell(
    /// Source node ID ("LOCAL" for local node).
    source_node: String,
    /// Source device ID.
    source_device: String,
    /// Source channel (1-based).
    source_channel: u16,
    /// Destination node ID ("LOCAL" for local node).
    dest_node: String,
    /// Destination device ID.
    dest_device: String,
    /// Destination channel (1-based).
    dest_channel: u16,
) -> impl IntoView {
    let app_state = expect_context::<AppState>();

    // Clone for different closures
    let src_node_1 = source_node.clone();
    let src_node_2 = source_node.clone();
    let src_node_3 = source_node.clone();
    let src_node_view = source_node.clone();
    let src_dev_1 = source_device.clone();
    let src_dev_2 = source_device.clone();
    let src_dev_3 = source_device.clone();
    let src_dev_view = source_device.clone();
    let dst_node_1 = dest_node.clone();
    let dst_node_2 = dest_node.clone();
    let dst_node_3 = dest_node.clone();
    let dst_node_view = dest_node.clone();
    let dst_dev_1 = dest_device.clone();
    let dst_dev_2 = dest_device.clone();
    let dst_dev_3 = dest_device.clone();
    let dst_dev_view = dest_device.clone();

    // Create derived signals for reactive updates
    let has_route = {
        let app_state = app_state.clone();
        Signal::derive(move || {
            app_state.has_route_with_nodes(
                &src_node_1,
                &src_dev_1,
                source_channel,
                &dst_node_1,
                &dst_dev_1,
                dest_channel,
            )
        })
    };

    let route = {
        let app_state = app_state.clone();
        Signal::derive(move || {
            app_state.get_route_with_nodes(
                &src_node_2,
                &src_dev_2,
                source_channel,
                &dst_node_2,
                &dst_dev_2,
                dest_channel,
            )
        })
    };

    let cell_class = move || {
        let mut classes = vec!["route-cell"];
        if has_route.get() {
            classes.push("route-active");
            if let Some(r) = route.get() {
                if r.muted {
                    classes.push("route-muted");
                }
            }
        }
        classes.join(" ")
    };

    // Handle click to toggle route
    let on_click = {
        let app_state = app_state.clone();
        move |_| {
            let src_node = src_node_3.clone();
            let src_dev = src_dev_3.clone();
            let dst_node = dst_node_3.clone();
            let dst_dev = dst_dev_3.clone();
            let app_state = app_state.clone();

            if has_route.get() {
                // Delete route
                let route_id = format!(
                    "{}:{}:{}->{}:{}:{}",
                    src_node, src_dev, source_channel, dst_node, dst_dev, dest_channel
                );
                wasm_bindgen_futures::spawn_local(async move {
                    if let Err(e) = api::delete_route(&route_id).await {
                        log::error!("Failed to delete route: {}", e);
                        app_state
                            .error
                            .set(Some(format!("Failed to delete route: {}", e)));
                    } else {
                        app_state.remove_route(&route_id);
                    }
                });
            } else {
                // Create route
                let route = ram_api::models::RouteDefinition {
                    source_node: src_node,
                    source_device: src_dev,
                    source_channel,
                    destination_node: dst_node,
                    destination_device: dst_dev,
                    destination_channel: dest_channel,
                    volume: 1.0,
                    muted: false,
                };
                wasm_bindgen_futures::spawn_local(async move {
                    match api::create_route(&route).await {
                        Ok(_) => {
                            app_state.upsert_route(route);
                        },
                        Err(e) => {
                            log::error!("Failed to create route: {}", e);
                            app_state
                                .error
                                .set(Some(format!("Failed to create route: {}", e)));
                        },
                    }
                });
            }
        }
    };

    let cell_content = move || {
        if let Some(r) = route.get() {
            if r.muted {
                "M".to_string()
            } else {
                format!("{:.0}", r.volume * 100.0)
            }
        } else {
            String::new()
        }
    };

    view! {
        <div
            class=cell_class
            on:click=on_click
            data-source-node=src_node_view
            data-source-device=src_dev_view
            data-source-channel=source_channel
            data-dest-node=dst_node_view
            data-dest-device=dst_dev_view
            data-dest-channel=dest_channel
        >
            {cell_content}
        </div>
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn route_cell_compiles() {
        // Compilation test
    }
}
