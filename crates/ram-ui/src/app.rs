//! Main application component and routing.

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::components::{DeviceList, Header, NodeSelector, RoutingMatrix};
use crate::state::AppState;

/// Root application component.
#[component]
pub fn App() -> impl IntoView {
    // Provide meta context for title/description
    provide_meta_context();

    // Create and provide global app state
    let app_state = AppState::new();
    provide_context(app_state);

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

/// Settings page.
#[component]
fn SettingsPage() -> impl IntoView {
    view! {
        <div class="page settings-page">
            <h1>"Settings"</h1>
            <p>"Settings coming soon..."</p>
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
    use super::*;

    #[test]
    fn app_compiles() {
        // Basic compilation test - actual rendering tests need wasm-bindgen-test
    }
}
