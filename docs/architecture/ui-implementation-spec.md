# AudioMatrix - Web UI Implementation Specification

> **Status**: Implementation spec for missing 70% of Web UI functionality
> **Created**: 2025-12-29
> **Priority**: Critical - Core user-facing features

---

## Executive Summary

The Web UI is ~30% complete. This spec defines the remaining 70% needed for a production-ready audio routing interface.

### Missing Features (Priority Order)

| Priority | Feature | Complexity | Est. LOC |
|----------|---------|------------|----------|
| P0 | Real-Time Metering WebSocket | High | 300 |
| P0 | Route Volume/Mute Controls | Medium | 250 |
| P1 | Device Attachment Workflow | High | 400 |
| P1 | Channel Label Editor | Medium | 350 |
| P2 | Virtual Device Creation | High | 450 |
| P2 | Settings Page | Medium | 400 |
| P3 | Generator Controls | Medium | 300 |
| P3 | Stream/Subscription Monitor | Low | 200 |

---

## P0: Real-Time Metering WebSocket

### Current State
- `AppState.input_levels` and `output_levels` signals exist
- `DeviceCard` component reads levels but shows -60dB (no data)
- WebSocket endpoint `/api/v1/ws` is fully functional
- No WebSocket connection established by UI

### Implementation

#### 1. WebSocket Service Module (`src/services/websocket.rs`)

```rust
//! WebSocket service for real-time events.

use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use web_sys::{MessageEvent, WebSocket, CloseEvent};
use serde::{Deserialize, Serialize};

/// WebSocket event types from server.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    Metering(MeteringUpdate),
    RouteChanged(RouteUpdate),
    NodeStatus(NodeStatusUpdate),
    DeviceStatus(DeviceStatusUpdate),
    Pong,
    Error { message: String },
}

#[derive(Debug, Clone, Deserialize)]
pub struct MeteringUpdate {
    pub node: String,
    pub device: String,
    pub direction: String,
    pub levels: Vec<f32>,
    pub peaks: Vec<f32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RouteUpdate {
    pub action: String,
    pub route_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodeStatusUpdate {
    pub node: String,
    pub online: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceStatusUpdate {
    pub device: String,
    pub state: String,
}

/// WebSocket commands to server.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsCommand {
    SubscribeMetering { node: String, device: String },
    UnsubscribeMetering { node: String, device: String },
    Ping,
}

/// WebSocket connection state.
#[derive(Clone)]
pub struct WsConnection {
    socket: StoredValue<Option<WebSocket>>,
    connected: RwSignal<bool>,
}

impl WsConnection {
    pub fn new() -> Self {
        Self {
            socket: StoredValue::new(None),
            connected: RwSignal::new(false),
        }
    }

    pub fn connect(&self, app_state: AppState) {
        // Build WebSocket URL from current location
        let location = web_sys::window().unwrap().location();
        let protocol = if location.protocol().unwrap() == "https:" { "wss" } else { "ws" };
        let host = location.host().unwrap();
        let url = format!("{}://{}/api/v1/ws", protocol, host);

        match WebSocket::new(&url) {
            Ok(ws) => {
                // Setup event handlers
                let state = app_state.clone();
                let on_message = Closure::wrap(Box::new(move |e: MessageEvent| {
                    if let Ok(text) = e.data().dyn_into::<js_sys::JsString>() {
                        let text: String = text.into();
                        if let Ok(event) = serde_json::from_str::<WsEvent>(&text) {
                            handle_ws_event(&state, event);
                        }
                    }
                }) as Box<dyn FnMut(MessageEvent)>);

                ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
                on_message.forget();

                // Store connection
                self.socket.set_value(Some(ws));
                self.connected.set(true);
            }
            Err(e) => {
                log::error!("WebSocket connection failed: {:?}", e);
            }
        }
    }

    pub fn send(&self, cmd: WsCommand) {
        if let Some(ws) = self.socket.get_value() {
            if let Ok(json) = serde_json::to_string(&cmd) {
                let _ = ws.send_with_str(&json);
            }
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected.get()
    }
}

fn handle_ws_event(state: &AppState, event: WsEvent) {
    match event {
        WsEvent::Metering(m) => {
            let levels: Vec<ChannelLevel> = m.levels.iter()
                .zip(m.peaks.iter())
                .map(|(&level, &peak)| ChannelLevel { level_db: level, peak_db: peak })
                .collect();

            state.update_levels(&m.device, m.direction == "input", levels);
        }
        WsEvent::RouteChanged(r) => {
            // Trigger route refresh
            log::info!("Route changed: {} - {}", r.action, r.route_id);
        }
        WsEvent::NodeStatus(n) => {
            log::info!("Node {} online: {}", n.node, n.online);
        }
        WsEvent::DeviceStatus(d) => {
            log::info!("Device {} status: {}", d.device, d.state);
        }
        WsEvent::Pong => {}
        WsEvent::Error { message } => {
            log::error!("WebSocket error: {}", message);
        }
    }
}
```

#### 2. Integration in App Component

Add to `app.rs` initialization:
```rust
// After fetching initial data
let ws = WsConnection::new();
provide_context(ws.clone());
ws.connect(app_state.clone());
```

---

## P0: Route Volume/Mute Controls

### Current State
- RouteCell shows volume percentage as text
- No way to adjust volume or toggle mute
- API `PUT /api/v1/routes/{id}` supports volume/mute

### Implementation

#### 1. Route Control Popover Component (`src/components/route_control.rs`)

```rust
//! Route control popover with volume slider and mute toggle.

use leptos::prelude::*;
use web_sys::HtmlInputElement;

use crate::api;
use crate::state::{AppState, RouteWithId};

/// Props for route control popover.
#[derive(Clone)]
pub struct RouteControlProps {
    pub route: RouteWithId,
    pub on_close: Callback<()>,
}

/// Route control popover component.
#[component]
pub fn RouteControl(props: RouteControlProps) -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let route = props.route.clone();

    let volume = RwSignal::new(route.volume);
    let muted = RwSignal::new(route.muted);
    let saving = RwSignal::new(false);

    // Convert volume (0.0-1.0) to dB display
    let volume_db = move || {
        let v = volume.get();
        if v <= 0.0 { "-inf".to_string() }
        else { format!("{:.1}", 20.0 * v.log10()) }
    };

    let route_id = route.id.clone();
    let on_volume_change = move |ev: web_sys::Event| {
        let target = event_target::<HtmlInputElement>(&ev);
        let value: f32 = target.value().parse().unwrap_or(1.0);
        volume.set(value);
    };

    let route_for_save = route.clone();
    let on_save = move |_| {
        let route_id = route_for_save.id.clone();
        saving.set(true);

        spawn_local(async move {
            let update = ram_api::models::RouteDefinition {
                source_node: route_for_save.source_node.clone(),
                source_device: route_for_save.source_device.clone(),
                source_channel: route_for_save.source_channel,
                destination_node: route_for_save.destination_node.clone(),
                destination_device: route_for_save.destination_device.clone(),
                destination_channel: route_for_save.destination_channel,
                volume: volume.get_untracked(),
                muted: muted.get_untracked(),
            };

            match api::update_route(&route_id, &update).await {
                Ok(_) => {
                    // Update local state
                    app_state.upsert_route(update);
                }
                Err(e) => {
                    log::error!("Failed to update route: {}", e);
                }
            }
            saving.set(false);
        });
    };

    let on_mute_toggle = move |_| {
        muted.update(|m| *m = !*m);
    };

    view! {
        <div class="route-control-popover">
            <div class="route-control-header">
                <span class="route-path">
                    {route.source_device.clone()}":"
                    {route.source_channel}
                    " → "
                    {route.destination_device.clone()}":"
                    {route.destination_channel}
                </span>
                <button class="close-btn" on:click=move |_| props.on_close.call(())>"×"</button>
            </div>

            <div class="route-control-body">
                <div class="volume-control">
                    <label>"Volume"</label>
                    <input
                        type="range"
                        min="0"
                        max="1"
                        step="0.01"
                        prop:value=move || volume.get()
                        on:input=on_volume_change
                    />
                    <span class="volume-value">{volume_db}" dB"</span>
                </div>

                <div class="mute-control">
                    <button
                        class="mute-btn"
                        class:muted=move || muted.get()
                        on:click=on_mute_toggle
                    >
                        {move || if muted.get() { "Unmute" } else { "Mute" }}
                    </button>
                </div>
            </div>

            <div class="route-control-footer">
                <button class="save-btn" on:click=on_save disabled=move || saving.get()>
                    {move || if saving.get() { "Saving..." } else { "Apply" }}
                </button>
            </div>
        </div>
    }
}
```

#### 2. Update RouteCell to Open Popover on Right-Click

Add context menu handling to `route_cell.rs`:
```rust
let on_context_menu = move |ev: web_sys::MouseEvent| {
    ev.prevent_default();
    if has_route {
        show_route_control.set(true);
    }
};
```

---

## P1: Device Attachment Workflow

### Current State
- DeviceList shows all devices
- No way to attach/detach devices
- API endpoints `POST /attach` and `POST /detach` exist

### Implementation

#### 1. Enhanced DeviceCard with Attach/Detach (`src/components/device_card.rs`)

Add attachment controls:
```rust
let on_attach = move |_| {
    let device_id = device.id.clone();
    let node_id = current_node_id.clone();

    spawn_local(async move {
        match api::attach_device(&node_id, &device_id, None).await {
            Ok(updated) => {
                app_state.update_device(updated);
            }
            Err(e) => {
                app_state.error.set(Some(format!("Failed to attach: {}", e)));
            }
        }
    });
};

let on_detach = move |_| {
    let device_id = device.id.clone();
    let node_id = current_node_id.clone();

    spawn_local(async move {
        match api::detach_device(&node_id, &device_id).await {
            Ok(_) => {
                app_state.mark_device_detached(&device_id);
            }
            Err(e) => {
                app_state.error.set(Some(format!("Failed to detach: {}", e)));
            }
        }
    });
};
```

#### 2. New API Functions (`src/api/mod.rs`)

```rust
/// Attaches a device.
pub async fn attach_device(node_id: &str, device_id: &str, display_name: Option<&str>) -> ApiResult<DeviceInfo> {
    let url = format!("{}/nodes/{}/devices/{}/attach", api_base(), node_id, device_id);
    let body = serde_json::json!({ "display_name": display_name });

    let response = Request::post(&url)
        .header("Content-Type", "application/json")
        .body(body.to_string())?
        .send()
        .await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.text().await.unwrap_or_default(),
            status: Some(response.status()),
        });
    }

    response.json().await.map_err(Into::into)
}

/// Detaches a device.
pub async fn detach_device(node_id: &str, device_id: &str) -> ApiResult<()> {
    let url = format!("{}/nodes/{}/devices/{}/detach", api_base(), node_id, device_id);
    let response = Request::post(&url).send().await?;

    if !response.ok() {
        return Err(ApiError {
            message: response.text().await.unwrap_or_default(),
            status: Some(response.status()),
        });
    }

    Ok(())
}
```

#### 3. DeviceList Reorganization

Split into sections:
```
┌─ Available Devices ────────────────────┐
│  □ Focusrite USB ASIO    [Attach]     │
│  □ Realtek HD Audio      [Attach]     │
└─────────────────────────────────────────┘

┌─ Attached Devices ─────────────────────┐
│  ● RME Fireface          [Detach]     │
│    Status: Active (5 routes)          │
└─────────────────────────────────────────┘
```

---

## P1: Channel Label Editor

### Current State
- Channels display numbers only (1, 2, 3...)
- API supports per-channel labels
- No UI for editing labels

### Implementation

#### 1. Channel Labels Page Component (`src/pages/channel_labels.rs`)

```rust
//! Channel label editor page.

use leptos::prelude::*;
use ram_api::models::ChannelInfo;

#[component]
pub fn ChannelLabelsPage() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let devices = app_state.devices;

    let selected_device = RwSignal::new(None::<String>);
    let channels = RwSignal::new(Vec::<ChannelInfo>::new());
    let loading = RwSignal::new(false);

    // Fetch channels when device selected
    let fetch_channels = move |device_id: String| {
        let node_id = app_state.source_node_id();
        loading.set(true);

        spawn_local(async move {
            match api::get_device_channels(&node_id, &device_id).await {
                Ok(ch) => channels.set(ch),
                Err(e) => log::error!("Failed to fetch channels: {}", e),
            }
            loading.set(false);
        });
    };

    view! {
        <div class="page channel-labels-page">
            <h1>"Channel Labels"</h1>

            <div class="device-selector">
                <label>"Device: "</label>
                <select on:change=move |ev| {
                    let value = event_target_value(&ev);
                    if !value.is_empty() {
                        selected_device.set(Some(value.clone()));
                        fetch_channels(value);
                    }
                }>
                    <option value="">"Select device..."</option>
                    <For
                        each=move || devices.get()
                        key=|d| d.id.clone()
                        children=|device| {
                            view! { <option value=device.id.clone()>{device.name.clone()}</option> }
                        }
                    />
                </select>
            </div>

            <Show when=move || selected_device.get().is_some()>
                <div class="channel-list">
                    <table>
                        <thead>
                            <tr>
                                <th>"Ch"</th>
                                <th>"Label"</th>
                                <th>"Level"</th>
                                <th></th>
                            </tr>
                        </thead>
                        <tbody>
                            <For
                                each=move || channels.get()
                                key=|ch| ch.number
                                children=move |channel| {
                                    view! { <ChannelRow channel=channel device_id=selected_device.get().unwrap() /> }
                                }
                            />
                        </tbody>
                    </table>
                </div>
            </Show>
        </div>
    }
}

#[component]
fn ChannelRow(channel: ChannelInfo, device_id: String) -> impl IntoView {
    let label = RwSignal::new(channel.label.clone().unwrap_or_default());
    let editing = RwSignal::new(false);
    let saving = RwSignal::new(false);

    let save_label = move |_| {
        let new_label = label.get_untracked();
        let ch = channel.number;
        let dev = device_id.clone();
        saving.set(true);

        spawn_local(async move {
            match api::update_channel_label("local", &dev, ch, &new_label).await {
                Ok(_) => editing.set(false),
                Err(e) => log::error!("Failed to save label: {}", e),
            }
            saving.set(false);
        });
    };

    view! {
        <tr>
            <td class="channel-number">{channel.number}</td>
            <td class="channel-label">
                <Show
                    when=move || editing.get()
                    fallback=move || view! {
                        <span on:dblclick=move |_| editing.set(true)>
                            {move || label.get().clone()}
                        </span>
                    }
                >
                    <input
                        type="text"
                        maxlength="31"
                        prop:value=move || label.get()
                        on:input=move |ev| label.set(event_target_value(&ev))
                        on:keydown=move |ev| {
                            if ev.key() == "Enter" { save_label(()); }
                            if ev.key() == "Escape" { editing.set(false); }
                        }
                    />
                    <button on:click=save_label disabled=move || saving.get()>"Save"</button>
                </Show>
            </td>
            <td class="channel-level">
                <Meter level_db=channel.level_dbfs.unwrap_or(-60.0) />
            </td>
        </tr>
    }
}
```

---

## P2: Virtual Device Creation

### Current State
- Virtual devices can be created via API
- No UI for creation/management
- DAW users need virtual ASIO devices

### Implementation

#### 1. Virtual Device Dialog Component

```rust
#[component]
pub fn CreateVirtualDeviceDialog(on_close: Callback<()>, on_created: Callback<DeviceInfo>) -> impl IntoView {
    let name = RwSignal::new(String::new());
    let input_channels = RwSignal::new(8u16);
    let output_channels = RwSignal::new(8u16);
    let sample_rate = RwSignal::new(48000u32);
    let buffer_size = RwSignal::new(64u32);
    let auto_attach = RwSignal::new(true);
    let creating = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);

    let on_create = move |_| {
        let device_name = name.get_untracked();
        if device_name.is_empty() || device_name.len() > 31 {
            error.set(Some("Name must be 1-31 characters".into()));
            return;
        }

        creating.set(true);
        error.set(None);

        spawn_local(async move {
            let req = CreateVirtualDeviceRequest {
                name: device_name,
                input_channels: input_channels.get_untracked(),
                output_channels: output_channels.get_untracked(),
                sample_rate: sample_rate.get_untracked(),
                buffer_size: buffer_size.get_untracked(),
                auto_attach: auto_attach.get_untracked(),
            };

            match api::create_virtual_device("local", &req).await {
                Ok(device) => {
                    on_created.call(device);
                    on_close.call(());
                }
                Err(e) => {
                    error.set(Some(e.message));
                }
            }
            creating.set(false);
        });
    };

    view! {
        <div class="modal-overlay">
            <div class="modal virtual-device-dialog">
                <h2>"Create Virtual ASIO Device"</h2>

                <Show when=move || error.get().is_some()>
                    <div class="error-banner">{move || error.get()}</div>
                </Show>

                <div class="form-group">
                    <label>"Device Name"</label>
                    <input
                        type="text"
                        placeholder="VASIO-DAW"
                        maxlength="31"
                        prop:value=move || name.get()
                        on:input=move |ev| name.set(event_target_value(&ev))
                    />
                    <span class="hint">"Appears in DAW device list (1-31 chars)"</span>
                </div>

                <div class="form-row">
                    <div class="form-group">
                        <label>"Input Channels"</label>
                        <select on:change=move |ev| {
                            input_channels.set(event_target_value(&ev).parse().unwrap_or(8));
                        }>
                            {[2, 4, 8, 16, 32, 64, 128, 256].iter().map(|&n| {
                                view! { <option value=n selected=move || input_channels.get() == n>{n}</option> }
                            }).collect_view()}
                        </select>
                    </div>
                    <div class="form-group">
                        <label>"Output Channels"</label>
                        <select on:change=move |ev| {
                            output_channels.set(event_target_value(&ev).parse().unwrap_or(8));
                        }>
                            {[2, 4, 8, 16, 32, 64, 128, 256].iter().map(|&n| {
                                view! { <option value=n selected=move || output_channels.get() == n>{n}</option> }
                            }).collect_view()}
                        </select>
                    </div>
                </div>

                <div class="form-row">
                    <div class="form-group">
                        <label>"Sample Rate"</label>
                        <select on:change=move |ev| {
                            sample_rate.set(event_target_value(&ev).parse().unwrap_or(48000));
                        }>
                            <option value="44100">"44.1 kHz"</option>
                            <option value="48000" selected>"48 kHz"</option>
                            <option value="96000">"96 kHz"</option>
                        </select>
                    </div>
                    <div class="form-group">
                        <label>"Buffer Size"</label>
                        <select on:change=move |ev| {
                            buffer_size.set(event_target_value(&ev).parse().unwrap_or(64));
                        }>
                            {[32, 64, 128, 256, 512, 1024, 2048].iter().map(|&n| {
                                view! { <option value=n selected=move || buffer_size.get() == n>{n}" samples"</option> }
                            }).collect_view()}
                        </select>
                    </div>
                </div>

                <div class="form-group checkbox">
                    <label>
                        <input
                            type="checkbox"
                            prop:checked=move || auto_attach.get()
                            on:change=move |ev| auto_attach.set(event_target_checked(&ev))
                        />
                        " Auto-attach after creation"
                    </label>
                </div>

                <div class="modal-actions">
                    <button class="btn-secondary" on:click=move |_| on_close.call(())>"Cancel"</button>
                    <button class="btn-primary" on:click=on_create disabled=move || creating.get()>
                        {move || if creating.get() { "Creating..." } else { "Create" }}
                    </button>
                </div>
            </div>
        </div>
    }
}
```

---

## P2: Settings Page

### Implementation

```rust
#[component]
pub fn SettingsPage() -> impl IntoView {
    let app_state = expect_context::<AppState>();

    view! {
        <div class="page settings-page">
            <h1>"Settings"</h1>

            <section class="settings-section">
                <h2>"Node Configuration"</h2>
                <div class="setting-item">
                    <label>"Node Name"</label>
                    <input type="text" value=move || {
                        app_state.current_node.get().map(|n| n.name).unwrap_or_default()
                    } disabled />
                    <span class="hint">"Configure in service config file"</span>
                </div>
            </section>

            <section class="settings-section">
                <h2>"Audio Settings"</h2>
                <div class="setting-item">
                    <label>"Default Sample Rate"</label>
                    <select>
                        <option>"44100 Hz"</option>
                        <option selected>"48000 Hz"</option>
                        <option>"96000 Hz"</option>
                    </select>
                </div>
                <div class="setting-item">
                    <label>"Default Buffer Size"</label>
                    <select>
                        <option>"64 samples"</option>
                        <option>"128 samples"</option>
                        <option selected>"256 samples"</option>
                    </select>
                </div>
            </section>

            <section class="settings-section">
                <h2>"Network"</h2>
                <div class="setting-item">
                    <label>"API Port"</label>
                    <input type="number" value="8080" disabled />
                </div>
                <div class="setting-item">
                    <label>"VBAN Port"</label>
                    <input type="number" value="6980" disabled />
                </div>
            </section>

            <section class="settings-section">
                <h2>"About"</h2>
                <div class="about-info">
                    <p><strong>"AudioMatrix"</strong></p>
                    <p>"Version: " {move || app_state.current_node.get().map(|_| "0.1.0-dev.9").unwrap_or("Unknown")}</p>
                    <p><a href="https://github.com/zbynekdrlik/audiomatrix" target="_blank">"GitHub Repository"</a></p>
                </div>
            </section>
        </div>
    }
}
```

---

## P3: Generator Controls

### Implementation in DeviceCard

Add generator toggle per output channel:
```rust
#[component]
fn ChannelGeneratorControl(device_id: String, channel: u16) -> impl IntoView {
    let enabled = RwSignal::new(false);
    let waveform = RwSignal::new("sine".to_string());
    let frequency = RwSignal::new(1000u32);
    let level_db = RwSignal::new(-20.0f32);

    let toggle_generator = move |_| {
        let new_state = !enabled.get_untracked();
        enabled.set(new_state);

        spawn_local(async move {
            let req = SetGeneratorRequest {
                enabled: new_state,
                waveform: waveform.get_untracked(),
                frequency: frequency.get_untracked(),
                level_db: level_db.get_untracked(),
            };
            let _ = api::set_channel_generator("local", &device_id, channel, &req).await;
        });
    };

    view! {
        <div class="generator-control">
            <select on:change=move |ev| waveform.set(event_target_value(&ev))>
                <option value="off">"Off"</option>
                <option value="sine">"Sine"</option>
                <option value="pink_noise">"Pink Noise"</option>
                <option value="channel_id">"Channel ID"</option>
                <option value="sweep">"Sweep"</option>
                <option value="click">"Click"</option>
            </select>
            <button class="generator-btn" class:active=move || enabled.get() on:click=toggle_generator>
                {move || if enabled.get() { "■" } else { "▶" }}
            </button>
        </div>
    }
}
```

---

## CSS Additions Required

Add to `style.css`:
```css
/* Route Control Popover */
.route-control-popover {
    position: absolute;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1rem;
    box-shadow: 0 4px 12px rgba(0,0,0,0.3);
    z-index: 100;
    min-width: 280px;
}

.route-control-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1rem;
    padding-bottom: 0.5rem;
    border-bottom: 1px solid var(--border);
}

.volume-control {
    display: flex;
    align-items: center;
    gap: 0.5rem;
}

.volume-control input[type="range"] {
    flex: 1;
    accent-color: var(--primary);
}

.mute-btn.muted {
    background: var(--warning);
    color: #000;
}

/* Modal */
.modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0,0,0,0.7);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
}

.modal {
    background: var(--surface);
    border-radius: 8px;
    padding: 1.5rem;
    max-width: 500px;
    width: 90%;
}

.form-group {
    margin-bottom: 1rem;
}

.form-group label {
    display: block;
    margin-bottom: 0.25rem;
    font-weight: 500;
}

.form-row {
    display: flex;
    gap: 1rem;
}

.form-row .form-group {
    flex: 1;
}

.modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 1.5rem;
}

/* Generator Control */
.generator-control {
    display: flex;
    gap: 0.25rem;
}

.generator-btn {
    width: 24px;
    height: 24px;
    padding: 0;
    border-radius: 4px;
}

.generator-btn.active {
    background: var(--error);
}

/* Settings Page */
.settings-section {
    margin-bottom: 2rem;
    padding: 1rem;
    background: var(--surface);
    border-radius: 8px;
}

.settings-section h2 {
    margin-bottom: 1rem;
    font-size: 1.1rem;
}

.setting-item {
    display: flex;
    align-items: center;
    gap: 1rem;
    margin-bottom: 0.75rem;
}

.setting-item label {
    min-width: 150px;
}

.setting-item .hint {
    color: var(--text-secondary);
    font-size: 0.85rem;
}
```

---

## New Routes Required

Add to `app.rs`:
```rust
<Route path=path!("/channels") view=ChannelLabelsPage/>
```

Add to Header navigation:
```rust
<a href="/channels">"Channels"</a>
```

---

## API Functions to Add

```rust
// Get channel info
pub async fn get_device_channels(node_id: &str, device_id: &str) -> ApiResult<Vec<ChannelInfo>>

// Update channel label
pub async fn update_channel_label(node_id: &str, device_id: &str, channel: u16, label: &str) -> ApiResult<ChannelInfo>

// Bulk update labels
pub async fn bulk_update_channel_labels(node_id: &str, device_id: &str, labels: &[ChannelLabelUpdate]) -> ApiResult<Vec<ChannelInfo>>

// Create virtual device
pub async fn create_virtual_device(node_id: &str, req: &CreateVirtualDeviceRequest) -> ApiResult<DeviceInfo>

// Delete virtual device
pub async fn delete_virtual_device(node_id: &str, device_id: &str) -> ApiResult<()>

// Set generator
pub async fn set_channel_generator(node_id: &str, device_id: &str, channel: u16, req: &SetGeneratorRequest) -> ApiResult<()>

// Get generator status
pub async fn get_device_generator(node_id: &str, device_id: &str) -> ApiResult<DeviceGeneratorResponse>
```

---

## Implementation Order

1. **WebSocket Connection** (enables real-time metering)
2. **Route Volume/Mute** (most requested feature)
3. **Device Attachment** (core workflow)
4. **Channel Labels** (professional feature)
5. **Virtual Devices** (DAW workflow)
6. **Settings Page** (configuration)
7. **Generator Controls** (testing feature)

---

## Testing Requirements

Each feature requires:
1. Component unit tests (Leptos test utils)
2. API integration tests (mock server)
3. E2E tests (browser automation)

See `e2e-test-spec.md` for comprehensive E2E test plan.
