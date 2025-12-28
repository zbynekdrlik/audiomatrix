//! Node selector component.

use leptos::prelude::*;
use web_sys::HtmlSelectElement;

use crate::state::AppState;

/// Dropdown for selecting which node to view/control.
#[component]
pub fn NodeSelector() -> impl IntoView {
    let app_state = expect_context::<AppState>();

    let nodes = app_state.nodes;
    let current_node = app_state.current_node;

    let app_state_for_select = app_state.clone();
    let on_select = move |ev: web_sys::Event| {
        let target = event_target::<HtmlSelectElement>(&ev);
        let node_id = target.value();

        if let Some(node) = nodes.get().into_iter().find(|n| n.id == node_id) {
            app_state_for_select.set_current_node(node);
        }
    };

    view! {
        <div class="node-selector">
            <label for="node-select">"Node: "</label>
            <select id="node-select" on:change=on_select>
                <For
                    each=move || nodes.get()
                    key=|node| node.id.clone()
                    children=move |node| {
                        let node_id = node.id.clone();
                        let node_name = node.name.clone();
                        let is_online = node.online;
                        let is_selected = {
                            let node_id = node_id.clone();
                            move || current_node.get().map_or(false, |n| n.id == node_id)
                        };
                        let display = if is_online {
                            node_name
                        } else {
                            format!("{} (offline)", node_name)
                        };
                        view! {
                            <option value=node_id selected=is_selected>
                                {display}
                            </option>
                        }
                    }
                />
            </select>
        </div>
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn node_selector_compiles() {
        // Compilation test
    }
}
