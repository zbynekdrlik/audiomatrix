//! Application header component.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::state::AppState;

/// Application header with navigation.
#[component]
pub fn Header() -> impl IntoView {
    let app_state = expect_context::<AppState>();

    let node_name = move || {
        app_state
            .current_node
            .get()
            .map_or_else(|| "Connecting...".to_string(), |n| n.name)
    };

    let connection_class = move || {
        if app_state.connected.get() {
            "connection-status status-connected"
        } else {
            "connection-status status-disconnected"
        }
    };

    view! {
        <header class="app-header">
            <div class="header-brand">
                <h1 class="header-title">"AudioMatrix"</h1>
                <span class="header-version">"v0.1.0-dev"</span>
            </div>

            <nav class="header-nav">
                <A href="/" attr:class="nav-link">"Matrix"</A>
                <A href="/devices" attr:class="nav-link">"Devices"</A>
                <A href="/routes" attr:class="nav-link">"Routes"</A>
                <A href="/settings" attr:class="nav-link">"Settings"</A>
            </nav>

            <div class="header-status">
                <span class=connection_class>
                    {node_name}
                </span>
            </div>
        </header>
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn header_compiles() {
        // Compilation test
    }
}
