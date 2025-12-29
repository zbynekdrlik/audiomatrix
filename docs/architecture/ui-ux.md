# AudioMatrix - Web UI/UX Architecture

> **Design Philosophy**: Professional audio routing with zero auto-connect.
> User explicitly controls all device attachments and virtual device creation.
> Inspired by [Dante Controller](https://www.getdante.com/products/software-essentials/dante-controller/) and [VB-Audio Matrix](https://vb-audio.com/Matrix/).

---

## Visual Design System

### AudioMatrix Identity

AudioMatrix uses a **diamond-based** iconography to distinguish itself:

| State | Symbol | Color | Description |
|-------|--------|-------|-------------|
| **Connected** | ◆ | Green (#22C55E) | Active route, audio flowing |
| **Available** | ◇ | Gray (#6B7280) | No route, can be connected |
| **Muted** | ◈ | Amber (#F59E0B) | Route exists but muted |
| **Error** | ✕ | Red (#EF4444) | Route failed, needs attention |
| **Pending** | ◇̇ | Blue (#3B82F6) | Route being established |

### Device Status Indicators

| Status | Symbol | Color | Description |
|--------|--------|-------|-------------|
| **Attached** | ▣ | Green | Device enabled for routing |
| **Available** | ▢ | Gray | Device detected, not attached |
| **Active** | ▣▸ | Green pulse | Device streaming audio |
| **Disconnected** | ▢̸ | Orange | Was attached, now offline |
| **Error** | ▢✕ | Red | Device error |

### Color Palette

| Purpose | Light Mode | Dark Mode | Usage |
|---------|------------|-----------|-------|
| **Primary** | #2563EB | #3B82F6 | Buttons, links, selection |
| **Success** | #16A34A | #22C55E | Connected routes, active |
| **Warning** | #D97706 | #F59E0B | Muted, attention needed |
| **Error** | #DC2626 | #EF4444 | Errors, disconnected |
| **Surface** | #FFFFFF | #1F2937 | Card backgrounds |
| **Background** | #F3F4F6 | #111827 | Page background |
| **Border** | #E5E7EB | #374151 | Dividers, outlines |
| **Text Primary** | #111827 | #F9FAFB | Main text |
| **Text Secondary** | #6B7280 | #9CA3AF | Labels, hints |

### Meter Visualization

Level meters use a **gradient segmented** design:

```
Input Level:  ████████████░░░░░░░░  -6.2 dB
              ▓▓▓▓▓▓▓▓▓▓▓▓▒▒▒▒▒▒▒▒
              └── green ──┘└ yellow ┘

Clipping:     ████████████████████  0.0 dB
              ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓████
              └───── green ─────┘└red┘
```

| Range | Color | Meaning |
|-------|-------|---------|
| -60 to -18 dB | Green | Healthy level |
| -18 to -6 dB | Yellow | Approaching hot |
| -6 to 0 dB | Orange | Hot |
| 0+ dB | Red | Clipping |

### Typography

| Element | Font | Size | Weight |
|---------|------|------|--------|
| **Headers** | Inter | 18-24px | 600 |
| **Channel Labels** | JetBrains Mono | 12px | 400 |
| **Device Names** | Inter | 14px | 500 |
| **Matrix Cells** | JetBrains Mono | 11px | 400 |
| **Status Text** | Inter | 12px | 400 |

---

## Core Principles

### 1. Zero Auto-Connect Policy

**CRITICAL**: AudioMatrix NEVER automatically connects to or starts streaming from any local audio device.

| Action | Behavior |
|--------|----------|
| Service startup | No devices attached by default |
| Device detected | Listed as "available", not "attached" |
| DAW opens ASIO device | Service notified, but no routing until user action |
| Network node discovered | Listed, devices visible, but no subscriptions created |

**Rationale**: Professional audio engineers need full control. Auto-connecting could:
- Cause feedback loops
- Consume system resources unexpectedly
- Create unwanted noise in live environments
- Interfere with other software using the same devices

### 2. User-Controlled Device Lifecycle

```
┌─────────────────────────────────────────────────────────────────┐
│                      Device States                               │
├───────────────┬───────────────┬───────────────┬────────────────┤
│  Available    │   Attached    │    Active     │   Detached     │
│  (detected)   │   (enabled)   │  (streaming)  │   (stopped)    │
├───────────────┼───────────────┼───────────────┼────────────────┤
│ Device exists │ User clicked  │ Audio flowing │ User clicked   │
│ in system     │ "Attach"      │ via routes    │ "Detach"       │
│               │               │               │                │
│ No streams    │ Stream ready  │ Callbacks     │ Stream stopped │
│ allocated     │ (not started) │ active        │ Resources free │
└───────────────┴───────────────┴───────────────┴────────────────┘
```

### 3. Channel-First Routing

Every routing operation works at the **channel level**:
- Channels numbered from **1** (not 0)
- Each channel can have a custom **label** (user-assigned name)
- Routes connect: `SourceDevice:Channel → DestDevice:Channel`

---

## Device Management

### Device Types

| Type | Description | Creation | Attachment |
|------|-------------|----------|------------|
| **Physical Input** | Hardware capture (mic, line in) | Auto-detected | User attaches |
| **Physical Output** | Hardware playback (speakers, headphones) | Auto-detected | User attaches |
| **Virtual ASIO** | Software device for DAW routing | User creates | User attaches |
| **VBAN Stream** | Network audio from remote node | User subscribes | User attaches |

### Device Properties

```
┌─────────────────────────────────────────────────────────────────┐
│ Device: Focusrite Scarlett 18i20                                │
├─────────────────────────────────────────────────────────────────┤
│ System Name:  Focusrite USB ASIO                                │
│ Display Name: "FOH Console" (user-defined alias)                │
│ Type:         Physical Input/Output                             │
│ Backend:      ASIO                                              │
│ Status:       ● Attached  ○ Available  ○ Detached               │
├─────────────────────────────────────────────────────────────────┤
│ Input Channels: 18    │  Sample Rate: 48000 Hz                  │
│ Output Channels: 20   │  Buffer Size: 64 samples                │
├─────────────────────────────────────────────────────────────────┤
│ [Attach/Detach]  [Configure]  [Rename]                          │
└─────────────────────────────────────────────────────────────────┘
```

### Device Naming Rules (Dante-Inspired)

| Field | Max Length | Allowed Characters | Notes |
|-------|------------|-------------------|-------|
| **Display Name** | 31 chars | A-Z, a-z, 0-9, `-`, `_`, space | User-defined alias |
| **System Name** | N/A | Read-only | From OS/driver |

Display names:
- Must be unique within a node
- Case-insensitive for comparisons ("Guitar" = "guitar")
- Stored in persistent config
- Visible in routing matrix and all UI elements

### Device Attachment Workflow

```
┌─────────────────────────────────────────────────────────────────┐
│                     Devices Page                                 │
├─────────────────────────────────────────────────────────────────┤
│ Node: stagebox1                                                  │
│                                                                  │
│ ┌─ Available Devices ────────────────────────────────────────┐  │
│ │                                                             │  │
│ │  □ Focusrite USB ASIO (18 in / 20 out)           [Attach]  │  │
│ │  □ Realtek ASIO (2 in / 2 out)                   [Attach]  │  │
│ │  □ WASAPI: Speakers (0 in / 2 out)               [Attach]  │  │
│ │                                                             │  │
│ └─────────────────────────────────────────────────────────────┘  │
│                                                                  │
│ ┌─ Attached Devices ─────────────────────────────────────────┐  │
│ │                                                             │  │
│ │  ● RME Fireface UCX II (18 in / 20 out)                    │  │
│ │    Alias: "Stage Box Main"                                 │  │
│ │    Status: Active (8 routes)        [Detach] [Configure]   │  │
│ │                                                             │  │
│ │  ● VASIO-IEM (8 in / 8 out) [Virtual]                      │  │
│ │    Status: Idle                     [Detach] [Configure]   │  │
│ │                                                             │  │
│ └─────────────────────────────────────────────────────────────┘  │
│                                                                  │
│ ┌─ Virtual Devices ──────────────────────────────────────────┐  │
│ │                                                             │  │
│ │  [+ Create Virtual ASIO Device]                            │  │
│ │                                                             │  │
│ └─────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Virtual Device Management

### Virtual Device Creation

Users can create virtual ASIO devices on any node from the Web UI.

**Creation Dialog:**

```
┌─────────────────────────────────────────────────────────────────┐
│            Create Virtual ASIO Device                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Device Name:     [VASIO-DAW____________]                        │
│                   (appears in DAW device list)                   │
│                                                                  │
│  ┌─ Channel Configuration ───────────────────────────────────┐  │
│  │                                                            │  │
│  │  Input Channels:   [8___] ▼     (2-256)                   │  │
│  │  Output Channels:  [8___] ▼     (2-256)                   │  │
│  │                                                            │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                  │
│  ┌─ Audio Settings ──────────────────────────────────────────┐  │
│  │                                                            │  │
│  │  Sample Rate:      [48000 Hz___] ▼                        │  │
│  │                    44100 / 48000 / 88200 / 96000          │  │
│  │                                                            │  │
│  │  Buffer Size:      [64 samples_] ▼                        │  │
│  │                    32 / 64 / 128 / 256 / 512 / 1024       │  │
│  │                                                            │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                  │
│  □ Auto-attach after creation                                    │
│                                                                  │
│                              [Cancel]  [Create]                  │
└─────────────────────────────────────────────────────────────────┘
```

### Virtual Device Modification

After creation, users can modify channel count without deleting:

```
┌─────────────────────────────────────────────────────────────────┐
│          Configure: VASIO-DAW                                    │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Status: ● Connected (Ableton Live)                              │
│                                                                  │
│  ┌─ Channel Count ───────────────────────────────────────────┐  │
│  │                                                            │  │
│  │  Current: 8 in / 8 out                                    │  │
│  │                                                            │  │
│  │  New Input Channels:   [16__] ▼  (expand to 16)           │  │
│  │  New Output Channels:  [16__] ▼  (expand to 16)           │  │
│  │                                                            │  │
│  │  ⚠ DAW will receive ASIOResetRequest signal.              │  │
│  │    Audio may briefly interrupt during resize.             │  │
│  │                                                            │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                  │
│  ┌─ Audio Settings ──────────────────────────────────────────┐  │
│  │                                                            │  │
│  │  Sample Rate: 48000 Hz  (locked while DAW connected)      │  │
│  │  Buffer Size: 64 samples                                  │  │
│  │                                                            │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                  │
│                    [Delete Device]  [Cancel]  [Apply]            │
└─────────────────────────────────────────────────────────────────┘
```

### Virtual Device Constraints

| Property | Minimum | Maximum | Notes |
|----------|---------|---------|-------|
| Input Channels | 2 | 256 | Even numbers recommended |
| Output Channels | 2 | 256 | Even numbers recommended |
| Total Channels | 4 | 512 | Per device |
| Sample Rate | 44100 | 192000 | Standard rates |
| Buffer Size | 32 | 2048 | Power of 2 |
| Name Length | 1 | 31 | ASIO driver name limit |

---

## Channel Naming

### Channel Identification

Each channel has two identifiers:

| Field | Type | Example | Purpose |
|-------|------|---------|---------|
| **Number** | Integer (1-based) | `1`, `2`, `15` | Permanent, system-assigned |
| **Label** | String (user-defined) | `"Kick"`, `"Vocal L"` | Human-readable name |

### Channel Label Rules (Dante-Inspired)

| Constraint | Rule |
|------------|------|
| Max length | 31 characters |
| Forbidden chars | `=`, `.`, `@` (reserved for routing syntax) |
| Uniqueness | Must be unique within device |
| Case | Case-insensitive for matching |
| Default | If no label set, display channel number |

### Channel Naming UI

```
┌─────────────────────────────────────────────────────────────────┐
│      Channel Labels: FOH Console (Focusrite 18i20)               │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─ Input Channels ──────────────────────────────────────────┐  │
│  │                                                            │  │
│  │  Ch  │ Label              │ Level                         │  │
│  │ ─────┼────────────────────┼──────────────────────────────│  │
│  │   1  │ [Kick______________] │ ████████░░░░ -12dB          │  │
│  │   2  │ [Snare Top_________] │ ██████░░░░░░ -18dB          │  │
│  │   3  │ [Snare Bottom______] │ ████░░░░░░░░ -24dB          │  │
│  │   4  │ [Hi-Hat____________] │ ████████░░░░ -12dB          │  │
│  │   5  │ [Tom 1_____________] │ ░░░░░░░░░░░░ -∞             │  │
│  │   6  │ [Tom 2_____________] │ ░░░░░░░░░░░░ -∞             │  │
│  │   7  │ [Overhead L________] │ ██████████░░ -6dB           │  │
│  │   8  │ [Overhead R________] │ ██████████░░ -6dB           │  │
│  │  ...                                                       │  │
│  │  18  │ [___________________] │ ░░░░░░░░░░░░ -∞            │  │
│  │                                                            │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                  │
│  ┌─ Output Channels ─────────────────────────────────────────┐  │
│  │                                                            │  │
│  │  Ch  │ Label              │ Level                         │  │
│  │ ─────┼────────────────────┼──────────────────────────────│  │
│  │   1  │ [Main L____________] │ ████████████ 0dB            │  │
│  │   2  │ [Main R____________] │ ████████████ 0dB            │  │
│  │  ...                                                       │  │
│  │                                                            │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                  │
│  [Import from CSV]  [Export to CSV]  [Clear All]  [Save]         │
└─────────────────────────────────────────────────────────────────┘
```

### Channel Label Display in Matrix

The routing matrix shows both number and label:

```
                      │ 1      │ 2      │ 3      │ 4      │
                      │ Kick   │ Snare  │ Hi-Hat │ Tom 1  │
  ────────────────────┼────────┼────────┼────────┼────────┤
  1  IEM Mix L        │   ◆    │   ◆    │   ◆    │   ◇    │
  2  IEM Mix R        │   ◆    │   ◆    │   ◆    │   ◇    │
  3  Recording L      │   ◆    │   ◆    │   ◇    │   ◆    │
  4  Recording R      │   ◆    │   ◆    │   ◇    │   ◆    │
```

---

## Routing Matrix UX

### Matrix Layout

```
┌─────────────────────────────────────────────────────────────────┐
│                     Routing Matrix                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─ Source Selection ────────────────────────────────────────┐  │
│  │  Node:   [develbox_____] ▼   Device: [Focusrite 18i20] ▼  │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                  │
│  ┌─ Destination Selection ───────────────────────────────────┐  │
│  │  Node:   [stagebox1____] ▼   Device: [VASIO-IEM_______] ▼  │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                  │
│              │   SOURCE: Focusrite 18i20 @ develbox             │
│              │   1      │   2      │   3      │   4      │ ...  │
│              │  Kick    │  Snare   │  Hi-Hat  │  Tom 1   │      │
│  ────────────┼──────────┼──────────┼──────────┼──────────┼──────│
│  DEST: VASIO │          │          │          │          │      │
│   1  IEM L   │    ◆     │    ◆     │    ◆     │    ◇     │      │
│   2  IEM R   │    ◆     │    ◆     │    ◆     │    ◇     │      │
│   3  Click L │    ◇     │    ◇     │    ◇     │    ◇     │      │
│   4  Click R │    ◇     │    ◇     │    ◇     │    ◇     │      │
│   5          │    ◇     │    ◇     │    ◇     │    ◇     │      │
│   6          │    ◇     │    ◇     │    ◇     │    ◇     │      │
│   7          │    ◇     │    ◇     │    ◇     │    ◇     │      │
│   8          │    ◇     │    ◇     │    ◇     │    ◇     │      │
│  ────────────┴──────────┴──────────┴──────────┴──────────┴──────│
│                                                                  │
│  Legend: ◆ Connected   ◇ Available   ◈ Muted   ✕ Error          │
│                                                                  │
│  Matrix: 8 destinations × 18 sources = 144 crosspoints           │
└─────────────────────────────────────────────────────────────────┘
```

### Crosspoint Interactions

| Action | Result |
|--------|--------|
| **Single click on ◇** | Create route (turn to ◆) |
| **Single click on ◆** | Delete route (turn to ◇) |
| **Right-click on ◆** | Context menu: Adjust gain, Mute, Delete |
| **Hover on crosspoint** | Show tooltip with route info |
| **Drag across cells** | Create/delete multiple routes |

### Crosspoint Context Menu

```
┌────────────────────────────┐
│ Kick → IEM L               │
├────────────────────────────┤
│ Gain: [-3_____] dB  [slider] │
│ ☑ Enabled                  │
│ ☐ Muted                    │
├────────────────────────────┤
│ [Disconnect]               │
└────────────────────────────┘
```

### Matrix Filtering

| Filter | Description | Example |
|--------|-------------|---------|
| **Text search** | Filter devices/channels by name | `"kick"` shows only channels containing "kick" |
| **Node filter** | Show only specific node | `"stagebox1"` |
| **Connected only** | Hide unconnected crosspoints | Toggle button |
| **Presets** | Save/load filter combinations | "Drums Only", "IEM Mix" |

---

## Route Control Panel

When a route is selected, show detailed controls:

```
┌─────────────────────────────────────────────────────────────────┐
│              Route: Kick → IEM Mix L                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Source:      develbox : Focusrite 18i20 : Ch 1 (Kick)          │
│  Destination: stagebox1 : VASIO-IEM : Ch 1 (IEM Mix L)          │
│                                                                  │
│  ┌─ Level Control ───────────────────────────────────────────┐  │
│  │                                                            │  │
│  │  Gain: [========●===========] -3.0 dB    [Mute] [Solo]    │  │
│  │        -60dB            0dB           +12dB               │  │
│  │                                                            │  │
│  │  Input Level:  ████████████░░░░░░░░  -6.2 dB              │  │
│  │  Output Level: ██████████░░░░░░░░░░  -9.2 dB              │  │
│  │                                                            │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                  │
│  ┌─ Latency ─────────────────────────────────────────────────┐  │
│  │  Input Buffer:  1.33 ms                                   │  │
│  │  Ring Buffer:   0.50 ms                                   │  │
│  │  Network:       2.10 ms                                   │  │
│  │  Output Buffer: 1.33 ms                                   │  │
│  │  ──────────────────────                                   │  │
│  │  Total:         5.26 ms                                   │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                  │
│  Status: ● Active   Connection ID: develbox:Focusrite:1>...     │
│                                                                  │
│                                            [Delete Route]        │
└─────────────────────────────────────────────────────────────────┘
```

---

## API Requirements

### New Endpoints Required

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/api/v1/nodes/{id}/devices/{id}/attach` | Attach device to AudioMatrix |
| `POST` | `/api/v1/nodes/{id}/devices/{id}/detach` | Detach device |
| `PATCH` | `/api/v1/nodes/{id}/devices/{id}` | Update device (rename alias) |
| `POST` | `/api/v1/nodes/{id}/virtual-devices` | Create virtual ASIO device |
| `PATCH` | `/api/v1/nodes/{id}/virtual-devices/{id}` | Update virtual device config |
| `DELETE` | `/api/v1/nodes/{id}/virtual-devices/{id}` | Delete virtual device |
| `GET` | `/api/v1/nodes/{id}/devices/{id}/channels` | Get channel labels |
| `PATCH` | `/api/v1/nodes/{id}/devices/{id}/channels/{ch}` | Update channel label |
| `PUT` | `/api/v1/nodes/{id}/devices/{id}/channels` | Bulk update channel labels |

### Enhanced Models

```rust
/// Enhanced device information.
pub struct DeviceInfo {
    /// Device identifier (system-assigned).
    pub id: String,
    /// Device name (from driver).
    pub system_name: String,
    /// User-defined display name (alias).
    pub display_name: Option<String>,
    /// Device type (input/output/both).
    pub device_type: DeviceType,
    /// Input channel count.
    pub input_channels: u16,
    /// Output channel count.
    pub output_channels: u16,
    /// Sample rate.
    pub sample_rate: u32,
    /// Buffer size in samples.
    pub buffer_size: u32,
    /// Whether this is a virtual device.
    pub is_virtual: bool,
    /// Attachment status.
    pub status: DeviceStatus,
    /// Audio backend (ASIO, WASAPI, ALSA, etc.).
    pub backend: AudioBackend,
}

/// Device attachment status.
pub enum DeviceStatus {
    /// Device detected but not attached to AudioMatrix.
    Available,
    /// Device attached and ready for routing.
    Attached,
    /// Device attached and actively streaming.
    Active,
    /// Device was attached but is now disconnected.
    Disconnected,
    /// Device in error state.
    Error(String),
}

/// Channel information with label.
pub struct ChannelInfo {
    /// Channel number (1-based).
    pub number: u16,
    /// User-defined label (optional).
    pub label: Option<String>,
    /// Current level in dBFS.
    pub level_db: f32,
    /// Peak hold level in dBFS.
    pub peak_db: f32,
}

/// Virtual device creation request.
pub struct CreateVirtualDevice {
    /// Device name (appears in DAW).
    pub name: String,
    /// Number of input channels.
    pub input_channels: u16,
    /// Number of output channels.
    pub output_channels: u16,
    /// Sample rate.
    pub sample_rate: u32,
    /// Buffer size in samples.
    pub buffer_size: u32,
    /// Auto-attach after creation.
    pub auto_attach: bool,
}

/// Channel label update request.
pub struct UpdateChannelLabel {
    /// Channel number (1-based).
    pub channel: u16,
    /// New label (empty string to clear).
    pub label: String,
}
```

### WebSocket Events

```rust
/// Device status changed.
pub struct DeviceStatusUpdate {
    pub node: String,
    pub device_id: String,
    pub old_status: DeviceStatus,
    pub new_status: DeviceStatus,
}

/// Channel label changed.
pub struct ChannelLabelUpdate {
    pub node: String,
    pub device_id: String,
    pub channel: u16,
    pub label: Option<String>,
}

/// Virtual device created.
pub struct VirtualDeviceCreated {
    pub node: String,
    pub device: DeviceInfo,
}
```

---

## Persistence

### channel-names.toml

```toml
# Channel names for devices on this node

[devices."Focusrite USB ASIO"]
display_name = "FOH Console"
input_channels = [
    { number = 1, label = "Kick" },
    { number = 2, label = "Snare Top" },
    { number = 3, label = "Snare Bottom" },
    { number = 4, label = "Hi-Hat" },
    # ... more channels
]
output_channels = [
    { number = 1, label = "Main L" },
    { number = 2, label = "Main R" },
    # ... more channels
]

[devices."VASIO-IEM"]
display_name = "IEM Send"
input_channels = [
    { number = 1, label = "IEM Mix L" },
    { number = 2, label = "IEM Mix R" },
]
output_channels = [
    { number = 1, label = "From DAW L" },
    { number = 2, label = "From DAW R" },
]
```

### virtual-devices.toml

```toml
# Virtual ASIO devices on this node

[[devices]]
name = "VASIO-DAW"
input_channels = 16
output_channels = 16
sample_rate = 48000
buffer_size = 64
auto_attach = true

[[devices]]
name = "VASIO-IEM"
input_channels = 8
output_channels = 8
sample_rate = 48000
buffer_size = 64
auto_attach = true
```

---

## Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| Device attachment API | Not Started | Core workflow |
| Device status tracking | Not Started | State machine |
| Virtual device creation API | Not Started | Windows-only initially |
| Channel label storage | Not Started | TOML persistence |
| Channel label API | Not Started | CRUD endpoints |
| Enhanced DeviceInfo model | Not Started | Extended properties |
| Matrix filter UI | Not Started | Text/node/preset filters |
| Crosspoint interaction | Partial | Basic click working |
| Route control panel | Not Started | Detailed route view |
| Bulk channel label import | Not Started | CSV support |

---

## References

- [Dante Controller](https://www.getdante.com/products/software-essentials/dante-controller/) - Device naming, channel labels, routing matrix
- [VB-Audio Matrix](https://vb-audio.com/Matrix/) - Device slots, explicit attachment, preset patches
- [Dante Device Naming](https://dev.audinate.com/GA/dante-controller/userguide/webhelp/content/device_and_channel_names.htm) - Naming rules and conventions
