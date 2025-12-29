# AudioMatrix - State Management

## Core Principle: Full Persistence

**CRITICAL**: ALL user configuration survives restarts. The system always starts in the last configured state.

| What | Persisted | Restored on Restart |
|------|-----------|---------------------|
| Device attachments | Yes | Yes - auto-attach on startup |
| Virtual devices | Yes | Yes - recreated automatically |
| Channel labels | Yes | Yes |
| Routes/subscriptions | Yes | Yes - auto-reconnect |
| Device aliases | Yes | Yes |
| Gain/mute per route | Yes | Yes |
| Test generator state | No | Disabled on restart (safety) |

### Startup Sequence

```
┌─────────────────────────────────────────────────────────────────────┐
│                    AudioMatrix Startup Sequence                      │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  1. LOAD CONFIGURATION                                              │
│     ├─ Read config.toml (ports, node name, defaults)                │
│     ├─ Read attached-devices.toml (which devices to attach)         │
│     ├─ Read virtual-devices/*.toml (recreate virtual devices)       │
│     └─ Read channel-labels.toml (all channel names)                 │
│                                                                      │
│  2. RESTORE VIRTUAL DEVICES                                         │
│     ├─ For each virtual-devices/*.toml:                             │
│     │   ├─ Register COM object (Windows)                            │
│     │   ├─ Create shared memory                                     │
│     │   └─ Mark as "available" (DAW can connect)                    │
│     └─ Virtual devices ready before any DAW starts                  │
│                                                                      │
│  3. ENUMERATE PHYSICAL DEVICES                                      │
│     ├─ Scan all audio backends (ASIO, WASAPI, ALSA)                 │
│     └─ Build device list (all "available" initially)                │
│                                                                      │
│  4. RESTORE ATTACHMENTS                                             │
│     ├─ For each device in attached-devices.toml:                    │
│     │   ├─ If device exists: attach (allocate resources)            │
│     │   └─ If device missing: log warning, skip                     │
│     └─ Attached devices ready for routing                           │
│                                                                      │
│  5. RESTORE ROUTES                                                  │
│     ├─ Read routes.toml (all route definitions)                     │
│     ├─ For each route:                                              │
│     │   ├─ If local: connect immediately                            │
│     │   ├─ If cross-node, dest is remote: forward to dest node      │
│     │   └─ If cross-node, source is remote: initiate subscription   │
│     └─ Routes with missing devices: mark "pending"                  │
│                                                                      │
│  6. START NETWORK SERVICES                                          │
│     ├─ Start mDNS announcer                                         │
│     ├─ Start mDNS browser                                           │
│     ├─ Start VBAN receiver                                          │
│     └─ Start API server                                             │
│                                                                      │
│  7. READY                                                           │
│     └─ System operational, accepting connections                    │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Destination-Owned Subscriptions

Like Dante, AudioMatrix uses a **receiver-centric** model:
- Destination "owns" its subscriptions
- Source derives sender list from requests
- Maximum resilience, no single point of failure

### State Ownership

| Location | Stores | Authority |
|----------|--------|-----------|
| **Destination** | What I want to receive | Authoritative |
| **Source** | Who requested from me | Derived, ephemeral |
| **Controller** | Pending routes (dest offline) | Temporary |

### Failure Handling

| Failure | Destination-Owned Behavior |
|---------|---------------------------|
| Source fails | Destination auto-reconnects when source returns |
| Destination fails | Destination reloads subscriptions on restart |
| Network split | Each side works locally, reconnect when healed |
| Device unplugged | Routes marked "pending", restored when device returns |

---

## File Storage

```
Windows: %PROGRAMDATA%\AudioMatrix\
Linux:   /var/lib/audiomatrix/
macOS:   /Library/Application Support/AudioMatrix/

audiomatrix/
├── config.toml                # Service configuration
├── attached-devices.toml      # Which devices are attached
├── channel-labels.toml        # All channel labels (all devices)
├── routes.toml                # All route definitions
├── virtual-devices/           # Virtual device definitions
│   ├── VASIO-DAW.toml
│   └── VASIO-IEM.toml
└── cache/
    ├── discovered-nodes.toml  # Recently seen nodes
    └── presets/               # Routing presets
        ├── live-show.toml
        └── rehearsal.toml
```

---

## Configuration Schemas

### config.toml

```toml
# AudioMatrix Service Configuration

[service]
node_name = "stagebox1"        # mDNS instance name
api_port = 8080                # REST/WebSocket port
vban_port = 6980               # VBAN UDP port

[audio]
# Supported sample rates: 96000, 48000, 44100 (in order of preference)
default_sample_rate = 48000
default_buffer_size = 64       # Samples (32, 64, 128, 256, 512, 1024, 2048)
sample_rate_mismatch = "resample"  # "resample", "block", or "warn"

[ui]
theme = "dark"                 # "light" or "dark"
meter_hold_time_ms = 2000      # Peak hold time
```

### attached-devices.toml

```toml
# Devices to attach on startup
# These are automatically attached when the service starts

[[devices]]
id = "Focusrite USB ASIO"
display_name = "FOH Console"
attached_at = 2025-12-29T14:30:00Z

[[devices]]
id = "VASIO-DAW"
display_name = "DAW Send"
attached_at = 2025-12-29T14:35:00Z
```

### channel-labels.toml

```toml
# Channel labels for all devices
# Labels persist even if device is not currently attached

[devices."Focusrite USB ASIO"]
display_name = "FOH Console"

[devices."Focusrite USB ASIO".input]
1 = "Kick"
2 = "Snare Top"
3 = "Snare Bottom"
4 = "Hi-Hat"
5 = "Tom 1"
6 = "Tom 2"
7 = "Overhead L"
8 = "Overhead R"
# Channels without labels use number as display

[devices."Focusrite USB ASIO".output]
1 = "Main L"
2 = "Main R"
3 = "Monitor L"
4 = "Monitor R"

[devices."VASIO-IEM"]
display_name = "IEM Send"

[devices."VASIO-IEM".input]
1 = "IEM Mix L"
2 = "IEM Mix R"
3 = "Click L"
4 = "Click R"
```

### routes.toml

```toml
# All routes owned by this node (destination-owned model)
# Routes where this node is the destination

[[routes]]
id = "develbox:Focusrite:1>LOCAL:VASIO-IEM:1"
source_node = "develbox"
source_device = "Focusrite USB ASIO"
source_channel = 1
destination_device = "VASIO-IEM"
destination_channel = 1
volume = 1.0
muted = false
created_at = 2025-12-29T14:40:00Z

[[routes]]
id = "develbox:Focusrite:2>LOCAL:VASIO-IEM:2"
source_node = "develbox"
source_device = "Focusrite USB ASIO"
source_channel = 2
destination_device = "VASIO-IEM"
destination_channel = 2
volume = 0.8
muted = false
created_at = 2025-12-29T14:40:05Z
```

### virtual-devices/VASIO-DAW.toml

```toml
# Virtual ASIO Device Definition

name = "VASIO-DAW"
input_channels = 16
output_channels = 16
sample_rate = 48000           # 96000, 48000, or 44100 only
buffer_size = 64
created_at = 2025-12-29T14:00:00Z

# COM registration (Windows only)
[windows]
clsid = "{12345678-1234-1234-1234-123456789ABC}"
```

---

## Persistence Triggers

| Action | Files Updated | Immediate Write |
|--------|---------------|-----------------|
| Attach device | attached-devices.toml | Yes |
| Detach device | attached-devices.toml | Yes |
| Create virtual device | virtual-devices/*.toml | Yes |
| Delete virtual device | virtual-devices/*.toml (deleted) | Yes |
| Update channel label | channel-labels.toml | Yes (debounced 500ms) |
| Create route | routes.toml | Yes |
| Delete route | routes.toml | Yes |
| Update route (gain/mute) | routes.toml | Debounced (1s) |
| Rename device alias | channel-labels.toml | Yes |

### Write Strategy

- **Atomic writes**: Write to temp file, then rename
- **Debouncing**: Frequent changes (gain slider) debounced to reduce I/O
- **Backup**: Keep `.bak` of previous version
- **Corruption recovery**: If file invalid, restore from `.bak`

---

## Connection State Machine

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Route Connection States                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   ┌───────────┐                      ┌───────────┐                  │
│   │  Pending  │──── device found ───▶│Connecting │                  │
│   │ (no dev)  │                      │           │                  │
│   └───────────┘                      └─────┬─────┘                  │
│        ▲                                   │                         │
│        │ device lost                       │ success                 │
│        │                                   ▼                         │
│   ┌────┴────┐                        ┌───────────┐                  │
│   │Suspended│◀─── device lost ───────│ Connected │                  │
│   │         │                        │ (active)  │                  │
│   └─────────┘                        └─────┬─────┘                  │
│        │                                   │                         │
│        │ device returns                    │ network issue           │
│        ▼                                   ▼                         │
│   ┌───────────┐                      ┌───────────┐                  │
│   │Connecting │◀─── retry ───────────│Reconnect  │                  │
│   │           │                      │  (wait)   │                  │
│   └───────────┘                      └─────┬─────┘                  │
│                                            │                         │
│                                            │ max retries             │
│                                            ▼                         │
│                                      ┌───────────┐                  │
│                                      │  Failed   │                  │
│                                      │(permanent)│                  │
│                                      └───────────┘                  │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

| State | UI Symbol | Description |
|-------|-----------|-------------|
| **Pending** | ◇̇ (blue) | Route defined, waiting for device |
| **Connecting** | ◇̇ (blue pulse) | Establishing connection |
| **Connected** | ◆ (green) | Audio flowing |
| **Suspended** | ◈ (amber) | Device temporarily unavailable |
| **Reconnecting** | ◆̇ (green pulse) | Recovering from network issue |
| **Failed** | ✕ (red) | Permanent failure |

---

## Sample Rate Configuration

**AudioMatrix supports exactly 3 sample rates**, ordered by frequency of use:

| Sample Rate | Priority | Use Case |
|-------------|----------|----------|
| **96000 Hz** | Primary | High-quality studio, broadcast |
| **48000 Hz** | Common | Standard professional audio |
| **44100 Hz** | Legacy | CD-quality, older equipment |

**No other sample rates are supported.**

### Sample Rate Handling

| Source Rate | Dest Rate | Action |
|-------------|-----------|--------|
| Same | Same | Direct passthrough |
| Different | Different | Automatic resampling |
| Unsupported | Any | Route rejected with error |

### Virtual Device Sample Rates

When creating virtual devices, only these three rates are offered:
- 96000 Hz (default for new devices)
- 48000 Hz
- 44100 Hz

---

## Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| Config persistence | Complete | TOML-based |
| Route persistence | Complete | routes.toml |
| Device attachment persistence | Not Started | attached-devices.toml |
| Virtual device persistence | Partial | Files exist, restore on startup needed |
| Channel label persistence | Not Started | channel-labels.toml |
| Startup restoration | Not Started | Full state restore sequence |
| Atomic writes | Not Started | Temp file + rename |
| Backup/recovery | Not Started | .bak files |
| Connection state machine | Complete | 7-state implementation |
| Presets | Not Started | Structure defined |
