//! Main application component and routing.

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};
use wasm_bindgen_futures::spawn_local;

use crate::api;
use crate::components::{DeviceList, Header, NodeSelector, RoutingMatrix};
use crate::services::websocket::WsService;
use crate::state::{AppState, RouteWithId};

/// Root application component.
#[component]
pub fn App() -> impl IntoView {
    // Provide meta context for title/description
    provide_meta_context();

    // Create and provide global app state
    let app_state = AppState::new();
    provide_context(app_state.clone());

    // Create and provide WebSocket service
    let ws_service = WsService::new();
    provide_context(ws_service.clone());

    // Connect WebSocket
    let ws = ws_service.clone();
    let state_for_ws = app_state.clone();
    spawn_local(async move {
        // Small delay to let the app mount
        gloo_timers::future::TimeoutFuture::new(100).await;
        ws.connect(state_for_ws);
    });

    // Fetch initial data on mount
    let state = app_state.clone();
    spawn_local(async move {
        // Fetch nodes
        match api::get_nodes().await {
            Ok(nodes) => {
                if let Some(first_node) = nodes.first().cloned() {
                    state.current_node.set(Some(first_node.clone()));
                    state.nodes.set(nodes);

                    // Fetch devices for current node
                    let node_id = urlencoding::encode(&first_node.id).to_string();
                    match api::get_devices(&node_id).await {
                        Ok(devices) => {
                            state.devices.set(devices);
                        },
                        Err(e) => {
                            log::error!("Failed to fetch devices: {}", e);
                            state
                                .error
                                .set(Some(format!("Failed to load devices: {}", e)));
                        },
                    }
                }
                state.connected.set(true);
            },
            Err(e) => {
                log::error!("Failed to fetch nodes: {}", e);
                state.error.set(Some(format!("Failed to connect: {}", e)));
            },
        }

        // Fetch routes
        match api::get_routes().await {
            Ok(routes) => {
                let routes_with_id: Vec<RouteWithId> =
                    routes.into_iter().map(RouteWithId::from).collect();
                state.routes.set(routes_with_id);
            },
            Err(e) => {
                log::error!("Failed to fetch routes: {}", e);
            },
        }

        state.loading.set(false);
    });

    view! {
        <Title text="AudioMatrix"/>
        <Meta name="description" content="Professional Audio Routing System"/>

        <Router>
            <Header/>
            <main class="app-main">
                <Routes fallback=|| view! { <NotFound/> }>
                    <Route path=path!("/") view=HomePage/>
                    <Route path=path!("/devices") view=DevicesPage/>
                    <Route path=path!("/routes") view=RoutesPage/>
                    <Route path=path!("/settings") view=SettingsPage/>
                </Routes>
            </main>
        </Router>
    }
}

/// Home page - main routing matrix view.
#[component]
fn HomePage() -> impl IntoView {
    view! {
        <div class="page home-page">
            <NodeSelector/>
            <RoutingMatrix/>
        </div>
    }
}

/// Devices page - list of all audio devices.
#[component]
fn DevicesPage() -> impl IntoView {
    view! {
        <div class="page devices-page">
            <h1>"Audio Devices"</h1>
            <DeviceList/>
        </div>
    }
}

/// Routes page - detailed route management.
#[component]
fn RoutesPage() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let routes = app_state.routes;

    view! {
        <div class="page routes-page">
            <h1>"Active Routes"</h1>
            <div class="routes-list">
                <For
                    each=move || routes.get()
                    key=|route| route.id.clone()
                    children=move |route| {
                        let source = format!("{}:{}", route.source_device, route.source_channel);
                        let dest = format!("{}:{}", route.destination_device, route.destination_channel);
                        let volume = format!("{:.0}%", route.volume * 100.0);
                        view! {
                            <div class="route-item">
                                <span class="route-source">{source}</span>
                                <span class="route-arrow">" → "</span>
                                <span class="route-dest">{dest}</span>
                                <span class="route-volume">{volume}</span>
                            </div>
                        }
                    }
                />
            </div>
        </div>
    }
}

/// Settings page - display-only node configuration.
#[component]
fn SettingsPage() -> impl IntoView {
    let app_state = expect_context::<AppState>();

    // Get node info
    let node_name = move || {
        app_state
            .current_node
            .get()
            .map(|n| n.name)
            .unwrap_or_else(|| "Unknown".to_string())
    };

    let node_id = move || {
        app_state
            .current_node
            .get()
            .map(|n| n.id)
            .unwrap_or_else(|| "Unknown".to_string())
    };

    let node_addresses = move || {
        app_state
            .current_node
            .get()
            .map(|n| n.addresses.join(", "))
            .unwrap_or_else(|| "Unknown".to_string())
    };

    let api_port = move || {
        app_state
            .current_node
            .get()
            .map(|n| n.api_port.to_string())
            .unwrap_or_else(|| "8080".to_string())
    };

    let vban_port = move || {
        app_state
            .current_node
            .get()
            .map(|n| n.vban_port.to_string())
            .unwrap_or_else(|| "6980".to_string())
    };

    let node_count = move || app_state.nodes.get().len();
    let device_count = move || app_state.devices.get().len();
    let route_count = move || app_state.routes.get().len();

    view! {
        <div class="page settings-page">
            <h1>"Settings"</h1>

            <section class="settings-section">
                <h2>"Node Information"</h2>
                <div class="settings-grid">
                    <div class="setting-row">
                        <span class="setting-label">"Node Name"</span>
                        <span class="setting-value">{node_name}</span>
                    </div>
                    <div class="setting-row">
                        <span class="setting-label">"Node ID"</span>
                        <span class="setting-value">{node_id}</span>
                    </div>
                    <div class="setting-row">
                        <span class="setting-label">"IP Addresses"</span>
                        <span class="setting-value">{node_addresses}</span>
                    </div>
                </div>
            </section>

            <section class="settings-section">
                <h2>"Network Configuration"</h2>
                <div class="settings-grid">
                    <div class="setting-row">
                        <span class="setting-label">"API Port"</span>
                        <span class="setting-value">{api_port}</span>
                    </div>
                    <div class="setting-row">
                        <span class="setting-label">"VBAN Port"</span>
                        <span class="setting-value">{vban_port}</span>
                    </div>
                </div>
            </section>

            <section class="settings-section">
                <h2>"System Status"</h2>
                <div class="settings-grid">
                    <div class="setting-row">
                        <span class="setting-label">"Discovered Nodes"</span>
                        <span class="setting-value">{node_count}</span>
                    </div>
                    <div class="setting-row">
                        <span class="setting-label">"Audio Devices"</span>
                        <span class="setting-value">{device_count}</span>
                    </div>
                    <div class="setting-row">
                        <span class="setting-label">"Active Routes"</span>
                        <span class="setting-value">{route_count}</span>
                    </div>
                </div>
            </section>

            <section class="settings-section">
                <h2>"About"</h2>
                <div class="settings-grid">
                    <div class="setting-row">
                        <span class="setting-label">"Application"</span>
                        <span class="setting-value">"AudioMatrix"</span>
                    </div>
                    <div class="setting-row">
                        <span class="setting-label">"Version"</span>
                        <span class="setting-value">"0.1.0-dev.10"</span>
                    </div>
                    <div class="setting-row">
                        <span class="setting-label">"Documentation"</span>
                        <a href="https://github.com/zbynekdrlik/audiomatrix" target="_blank" class="setting-link">
                            "GitHub Repository"
                        </a>
                    </div>
                </div>
            </section>
        </div>
    }
}

/// 404 Not Found page.
#[component]
fn NotFound() -> impl IntoView {
    view! {
        <div class="page not-found">
            <h1>"404"</h1>
            <p>"Page not found"</p>
        </div>
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn app_compiles() {
        // Basic compilation test - actual rendering tests need wasm-bindgen-test
    }
}
