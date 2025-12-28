//! Device list component.

use leptos::prelude::*;

use super::DeviceCard;
use crate::state::AppState;

/// List of all audio devices grouped by type.
#[component]
pub fn DeviceList() -> impl IntoView {
    let app_state = expect_context::<AppState>();

    let app_state_1 = app_state.clone();
    let app_state_2 = app_state.clone();

    let input_devices = move || app_state_1.input_devices();
    let output_devices = move || app_state_2.output_devices();

    view! {
        <div class="device-list">
            <section class="device-section">
                <h2>"Input Devices"</h2>
                <div class="device-grid">
                    <For
                        each=input_devices
                        key=|d| d.id.clone()
                        children=move |device| {
                            view! { <DeviceCard device=device is_input=true/> }
                        }
                    />
                </div>
            </section>

            <section class="device-section">
                <h2>"Output Devices"</h2>
                <div class="device-grid">
                    <For
                        each=output_devices
                        key=|d| d.id.clone()
                        children=move |device| {
                            view! { <DeviceCard device=device is_input=false/> }
                        }
                    />
                </div>
            </section>
        </div>
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn device_list_compiles() {
        // Compilation test
    }
}
