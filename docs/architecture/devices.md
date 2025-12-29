# AudioMatrix - Device Architecture

## Core Principle: Zero Auto-Connect

**CRITICAL**: AudioMatrix NEVER automatically connects to local audio devices.

| Behavior | Correct | Incorrect |
|----------|---------|-----------|
| Service startup | No devices attached | Auto-attach default device |
| Device detected | Listed as "available" | Automatically streaming |
| DAW opens virtual ASIO | Service notified | Auto-route to outputs |
| Network node discovered | Listed in UI | Auto-subscribe streams |

**Rationale**: Professional audio engineers require explicit control to:
- Prevent feedback loops
- Avoid resource conflicts with other software
- Maintain predictable behavior in live environments
- Control exactly which devices participate in routing

---

## Device Model

### Device Identification

Every audio endpoint is uniquely identified as `{node}:{device}`:

```
stagebox1:Focusrite 18i20      # Physical ASIO device
stagebox1:VASIO-DAW            # Virtual ASIO device
develbox:RME Fireface          # Physical on another computer
```

### Device Types

| Type | Description | Creation | Attachment |
|------|-------------|----------|------------|
| **Physical Input** | Hardware capture (mic, line in) | Auto-detected by OS | User explicitly attaches |
| **Physical Output** | Hardware playback (speakers) | Auto-detected by OS | User explicitly attaches |
| **Virtual ASIO** | Software device for DAW routing | User creates via API/UI | User explicitly attaches |
| **VBAN Input** | Network audio from remote node | Created on subscription | Automatic on cross-node route |

### Device States

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Device State Machine                         │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   ┌───────────┐    attach()     ┌───────────┐   start_stream()      │
│   │ Available │ ───────────────▶│ Attached  │──────────────────┐    │
│   │ (passive) │                 │ (ready)   │                  │    │
│   └───────────┘                 └───────────┘                  ▼    │
│         ▲                            │ ▲                 ┌─────────┐│
│         │                   detach() │ │ stop_stream()   │ Active  ││
│         │                            ▼ │                 │(running)││
│         │                       ┌───────────┐            └─────────┘│
│         │     device_removed()  │ Detached  │◀──────────────────────│
│         └───────────────────────│ (stopped) │        detach()       │
│                                 └───────────┘                       │
│                                      │                              │
│                     device_error()   ▼                              │
│                                 ┌───────────┐                       │
│                                 │   Error   │                       │
│                                 └───────────┘                       │
└─────────────────────────────────────────────────────────────────────┘
```

| State | Description | Streams | Routes Allowed |
|-------|-------------|---------|----------------|
| **Available** | Device exists in system, not attached to AudioMatrix | None | No |
| **Attached** | User attached device, ready for routing | Allocated, not started | Yes (pending) |
| **Active** | Device has routes, audio streaming | Running | Yes |
| **Detached** | User detached, was previously attached | Stopped, deallocated | No |
| **Error** | Hardware error, disconnected | Failed | No |

### Device Properties

```rust
/// Complete device information.
pub struct DeviceInfo {
    /// System-assigned unique identifier.
    pub id: String,

    /// Driver-reported device name.
    pub system_name: String,

    /// User-defined display name (alias).
    /// If None, display system_name.
    pub display_name: Option<String>,

    /// Device capabilities.
    pub device_type: DeviceType,

    /// Input channel count (0 if output-only).
    pub input_channels: u16,

    /// Output channel count (0 if input-only).
    pub output_channels: u16,

    /// Current sample rate.
    pub sample_rate: u32,

    /// Current buffer size in samples.
    pub buffer_size: u32,

    /// Whether this is a virtual device created by AudioMatrix.
    pub is_virtual: bool,

    /// Current device state.
    pub status: DeviceStatus,

    /// Audio backend (ASIO, WASAPI, ALSA, etc.).
    pub backend: AudioBackend,
}

/// Device attachment status.
pub enum DeviceStatus {
    Available,
    Attached,
    Active,
    Detached,
    Error(String),
}

/// Device type classification.
pub enum DeviceType {
    /// Capture-only device.
    Input,
    /// Playback-only device.
    Output,
    /// Full-duplex device.
    Duplex,
}

/// Audio backend type.
pub enum AudioBackend {
    Asio,
    Wasapi,
    Alsa,
    CoreAudio,
    PipeWire,
}
```

---

## Device Naming

### Naming Rules (Dante-Inspired)

| Property | Rule |
|----------|------|
| **System Name** | Read-only, from driver/OS |
| **Display Name** | User-editable alias, 1-31 characters |
| **Allowed Characters** | A-Z, a-z, 0-9, `-`, `_`, space |
| **Uniqueness** | Display name must be unique within node |
| **Case Sensitivity** | Case-insensitive for matching ("Guitar" = "guitar") |

### Display Priority

When showing device in UI:
1. If `display_name` is set, show it
2. Otherwise, show `system_name`
3. In detailed views, always show both

---

## Virtual ASIO Device Management

Virtual ASIO devices are software audio devices that:
- Appear in DAW ASIO device lists
- Route audio through AudioMatrix
- Allow DAWs to send/receive audio to/from the routing matrix

### Virtual Device Lifecycle

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Virtual Device Lifecycle                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  1. CREATE (via API or UI)                                          │
│     ├─ Generate unique CLSID for COM registration                   │
│     ├─ Register in Windows Registry (HKLM\SOFTWARE\ASIO)            │
│     ├─ Device appears in ASIO device lists                          │
│     └─ Status: Available (not attached)                             │
│                                                                      │
│  2. ATTACH (user action)                                            │
│     ├─ Allocate shared memory for IPC                               │
│     ├─ Initialize ring buffers                                      │
│     └─ Status: Attached (ready for routing)                         │
│                                                                      │
│  3. DAW CONNECT (ASIOInit called)                                   │
│     ├─ DAW calls ASIOInit() on the virtual driver                   │
│     ├─ Driver connects to AudioMatrix service via shared memory     │
│     ├─ Audio buffers allocated based on DAW's requested size        │
│     └─ ASIOStart() begins audio callback loop                       │
│                                                                      │
│  4. ACTIVE (audio flowing)                                          │
│     ├─ Callbacks running at configured rate                         │
│     ├─ Audio routed through matrix                                  │
│     └─ Status: Active                                               │
│                                                                      │
│  5. RESIZE (while connected - optional)                             │
│     ├─ User requests channel count change via API/UI                │
│     ├─ Service updates channel count in shared state                │
│     ├─ Driver signals ASIOResetRequest to DAW                       │
│     ├─ DAW re-queries channel count                                 │
│     └─ No disconnect required                                       │
│                                                                      │
│  6. DAW DISCONNECT (ASIOStop/Exit called)                           │
│     ├─ DAW calls ASIOStop(), ASIOExit()                             │
│     ├─ Shared memory remains allocated                              │
│     └─ Status: Attached (ready for reconnect)                       │
│                                                                      │
│  7. DETACH (user action)                                            │
│     ├─ Stop any streams                                             │
│     ├─ Release shared memory                                        │
│     └─ Status: Available                                            │
│                                                                      │
│  8. DELETE (device removal)                                         │
│     ├─ Detach if attached                                           │
│     ├─ Remove all routes using this device                          │
│     ├─ Unregister COM object                                        │
│     └─ Remove from Windows Registry                                 │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### Virtual Device Creation

```rust
/// Request to create a virtual ASIO device.
pub struct CreateVirtualDevice {
    /// Device name (appears in DAW ASIO list).
    /// Must be unique, 1-31 characters.
    pub name: String,

    /// Number of input channels (DAW receives from matrix).
    /// Range: 2-256, must be even.
    pub input_channels: u16,

    /// Number of output channels (DAW sends to matrix).
    /// Range: 2-256, must be even.
    pub output_channels: u16,

    /// Sample rate in Hz.
    /// Supported: 44100, 48000, 88200, 96000, 176400, 192000.
    pub sample_rate: u32,

    /// Buffer size in samples.
    /// Must be power of 2: 32, 64, 128, 256, 512, 1024, 2048.
    pub buffer_size: u32,

    /// Automatically attach after creation.
    pub auto_attach: bool,
}

/// Virtual device constraints.
pub const VIRTUAL_DEVICE_CONSTRAINTS: VirtualDeviceConstraints = VirtualDeviceConstraints {
    min_channels: 2,
    max_channels: 256,
    max_total_channels: 512,
    min_sample_rate: 44100,
    max_sample_rate: 192000,
    supported_sample_rates: &[44100, 48000, 88200, 96000, 176400, 192000],
    supported_buffer_sizes: &[32, 64, 128, 256, 512, 1024, 2048],
    max_name_length: 31,
};
```

### Virtual Device Modification

Users can modify virtual device properties without deleting:

| Property | Modifiable While Connected | Notes |
|----------|---------------------------|-------|
| Channel count | Yes | Triggers ASIOResetRequest |
| Sample rate | No | Must disconnect DAW first |
| Buffer size | No | Must disconnect DAW first |
| Name | No | Would break registry reference |

---

## Device Attachment API

### Attach Device

```http
POST /api/v1/nodes/{node_id}/devices/{device_id}/attach
Content-Type: application/json

{
    "display_name": "FOH Console"  // Optional alias
}
```

Response:
```json
{
    "id": "Focusrite USB ASIO",
    "system_name": "Focusrite USB ASIO",
    "display_name": "FOH Console",
    "status": "attached",
    "input_channels": 18,
    "output_channels": 20
}
```

### Detach Device

```http
POST /api/v1/nodes/{node_id}/devices/{device_id}/detach
```

Response:
```json
{
    "id": "Focusrite USB ASIO",
    "status": "detached",
    "routes_removed": 5
}
```

### Create Virtual Device

```http
POST /api/v1/nodes/{node_id}/virtual-devices
Content-Type: application/json

{
    "name": "VASIO-DAW",
    "input_channels": 16,
    "output_channels": 16,
    "sample_rate": 48000,
    "buffer_size": 64,
    "auto_attach": true
}
```

Response:
```json
{
    "id": "VASIO-DAW",
    "system_name": "VASIO-DAW",
    "display_name": null,
    "status": "attached",
    "is_virtual": true,
    "input_channels": 16,
    "output_channels": 16
}
```

---

## Platform Support

| Platform | Primary Backend | Virtual Device Support | Notes |
|----------|-----------------|----------------------|-------|
| **Windows** | ASIO | Yes (Virtual ASIO) | Primary target |
| **Linux** | ALSA | Future (Virtual ALSA) | PipeWire via ALSA |
| **macOS** | CoreAudio | Future | Lower priority |

---

## Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| Device enumeration | Complete | cpal-based, all platforms |
| Device state machine | Not Started | Available/Attached/Active/Error |
| Physical ASIO access | Complete | Via cpal ASIO backend |
| Device attachment API | Not Started | Explicit user control |
| Device detachment API | Not Started | Stop streams, release resources |
| Device naming/aliasing | Not Started | Persistent, user-editable |
| Virtual ASIO (Rust side) | Complete | Shared memory IPC ready |
| Virtual ASIO (C++ driver) | Complete | COM/ASIO driver with IClassFactory |
| Virtual ASIO creation API | Not Started | Create from Web UI |
| Virtual ASIO resize API | Not Started | Channel count changes |
| Virtual ASIO deletion API | Not Started | Registry cleanup |
| Auto-attach disabled | Not Started | CRITICAL: Zero auto-connect |
