# AudioMatrix - System Architecture

## Overview

A Dante-like audio routing system built in Rust, providing unified control over local and networked audio devices with ~2.7ms local latency and ~4ms network latency.

**Core Principles:**
- **Zero Auto-Connect**: User explicitly attaches devices; no automatic streaming
- **Unified Matrix**: All devices (local and remote) in one seamless routing matrix
- **Network Transparency**: Cross-computer routing identical to local routing
- **Dynamic Devices**: Create/resize virtual ASIO devices on demand via Web UI
- **Ultra-Low Latency**: Lock-free audio paths, real-time priorities
- **Destination-Owned**: Receiver-centric subscriptions (like Dante)
- **Channel-First**: Every routing operation works at channel level with user labels

## Architecture Documents

| Document | Description |
|----------|-------------|
| [devices.md](docs/architecture/devices.md) | Device model, attachment lifecycle, virtual ASIO |
| [routing.md](docs/architecture/routing.md) | Matrix controller, connections, controls |
| [network.md](docs/architecture/network.md) | VBAN protocol, mDNS, cross-computer |
| [threading.md](docs/architecture/threading.md) | Thread model, lock-free design, latency |
| [state.md](docs/architecture/state.md) | Persistence, subscriptions, configuration |
| [api.md](docs/architecture/api.md) | REST API, WebSocket, device/channel endpoints |
| [ui-ux.md](docs/architecture/ui-ux.md) | Web UI design, routing matrix, channel naming |

## Implementation Status

> **Current Version:** 0.1.0-dev.36
> **Last Updated:** 2026-01-01

### Backend Implementation Status

| Component | Status | Crate | Notes |
|-----------|--------|-------|-------|
| **ASIO Device Support** | Complete | ram-asio | Enumeration, device access via cpal |
| **Virtual ASIO (Rust)** | Complete | ram-asio | Shared memory IPC |
| **Virtual ASIO (C++)** | Complete | ram-asio/cpp | COM/ASIO driver with IClassFactory, E2E tests |
| **Device Enumeration** | Complete | ram-core | All platforms (ASIO, WASAPI, ALSA) |
| **Audio Routing Matrix** | Complete | ram-core | RCU-based lock-free routing table |
| **Per-connection Controls** | Complete | ram-core | Lock-free gain/mute via AtomicF32/AtomicBool |
| **Ring Buffer Pool** | Complete | ram-core | Pre-allocated SPSC buffers (256 x 2048) |
| **Lock-Free Callbacks** | Complete | ram-core | Input/output callbacks, zero allocations |
| **Resampling** | Complete | ram-core | Rubato integration |
| **VBAN Protocol** | Complete | ram-vban | Full encode/decode |
| **VBAN Streaming** | Complete | ram-vban | Sender, receiver, jitter buffer |
| **mDNS Discovery** | Complete | ram-discovery | Announce and browse |
| **REST API** | Complete | ram-api | All endpoints implemented and tested |
| **WebSocket Events** | Complete | ram-api | Event types, metering broadcast at 30Hz |
| **Configuration** | Complete | ram-core | JSON persistence |
| **Audio Processing Loop** | Complete | ram-service | cpal streams with RouteController trait |
| **Subscription Protocol** | Complete | ram-core | Subscribe/unsubscribe messages, manager |
| **Latency Measurement** | Complete | ram-core | Calculator, callback timer, jitter tracking |
| **Metering** | Complete | ram-core | Lock-free level meters, peak/RMS, per-channel |
| **Cross-Node VBAN** | Complete | ram-service | Auto-subscription on cross-node routes |
| **Windows System Tray** | Complete | ram-service | Tray icon with menu |
| **Device Attachment API** | Complete | ram-api | Explicit attach/detach endpoints |
| **Virtual Device API** | Complete | ram-api | Create/modify/delete endpoints |
| **Channel Label API** | Complete | ram-api | Per-channel naming CRUD |
| **Wave Generator API** | Complete | ram-api | Per-channel test signal endpoints |
| **Zero Auto-Connect** | Complete | ram-service | Config option `auto_start_devices` |
| **State Persistence** | Complete | ram-core | Routes, attachments, labels, virtual devices |
| **Device State Manager** | Complete | ram-service | JSON persistence for device config |
| **Device Config API** | Complete | ram-api | Sample rate, buffer size update endpoints |

### Web UI Implementation Status (~75% Complete)

| Feature | Status | Notes |
|---------|--------|-------|
| **Routing Matrix Grid** | Complete | Click to create/delete routes |
| **Cross-Node Selection** | Complete | Source/dest node selectors |
| **Device List Display** | Complete | Shows devices with meters |
| **Route Create/Delete** | Complete | API integration working |
| **Header/Navigation** | Complete | Node selector, WebSocket status indicator |
| **WebSocket Connection** | Complete | Auto-connect, reconnect, event handling |
| **Real-Time Metering** | Complete | WebSocket subscription per attached device |
| **Route Volume/Mute** | Complete | RouteControl component with slider/toggle |
| **Device Attachment UI** | Complete | Attach/detach buttons, status display |
| **Virtual Device Creation** | Complete | Dialog with name, channels, sample rate |
| **Channel Label Editor** | Complete | Per-channel naming component |
| **Generator Controls** | Complete | Waveform selection, frequency, amplitude |
| **Device Config Dialog** | Complete | Sample rate/buffer size configuration UI |
| **Settings Page** | Not Started | Empty placeholder only |
| **Stream Monitor** | Not Started | API ready, no streams page |

### Testing Status

| Category | Status | Notes |
|----------|--------|-------|
| **Unit Tests** | 154+ tests | All backend crates fully tested |
| **API E2E Tests** | In Progress | 14 device config tests implemented |
| **WebSocket E2E** | Not Started | Spec created, needs implementation |
| **UI E2E (Playwright)** | Not Started | Spec created, needs implementation |
| **Cross-Node E2E** | Not Started | Spec created, needs Docker setup |

**Legend:** Complete = Working | Infrastructure Only = Framework exists | Not Started = Planned

## System Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                              Network (LAN)                          │
│                                                                     │
│  ┌───────────────────┐              ┌───────────────────┐          │
│  │     STUDIO-PC     │              │      LIVE-PC      │          │
│  │                   │              │                   │          │
│  │  AudioMatrix      │◄────────────►│  AudioMatrix      │          │
│  │  Service          │   VBAN/mDNS  │  Service          │          │
│  │                   │              │                   │          │
│  │  • Focusrite 18i20│              │  • RME Fireface   │          │
│  │  • VASIO-DAW      │              │  • VASIO-Main     │          │
│  └─────────┬─────────┘              └─────────┬─────────┘          │
│            │                                  │                     │
│            └──────────────┬───────────────────┘                     │
│                           │                                         │
│                  ┌────────▼────────┐                               │
│                  │  Web Interface  │                               │
│                  │  (Any browser)  │                               │
│                  └─────────────────┘                               │
└─────────────────────────────────────────────────────────────────────┘
```

## Crate Structure

```
audiomatrix/
├── crates/
│   ├── ram-core/           # Audio engine, routing, devices
│   │   ├── buffer.rs           # SPSC ring buffer
│   │   ├── ring_buffer_pool.rs # Pre-allocated buffer pool
│   │   ├── routing_table.rs    # RCU routing table
│   │   ├── routing_snapshot.rs # Immutable routing snapshots
│   │   ├── callbacks.rs        # Lock-free audio callbacks (with metering)
│   │   ├── metering.rs         # Lock-free level meters (peak/RMS)
│   │   ├── active_stream.rs    # Stream types and stats
│   │   ├── stream_registry.rs  # Stream lifecycle management
│   │   ├── connection.rs       # Connection state machine
│   │   ├── device.rs           # Device enumeration
│   │   ├── resampler.rs        # Sample rate conversion
│   │   ├── subscription.rs     # Subscription protocol messages
│   │   ├── subscription_manager.rs # Subscription lifecycle
│   │   ├── latency.rs          # Latency measurement
│   │   ├── wave_generator.rs   # Lock-free test signal generator
│   │   └── route_controller.rs # RouteController trait for API-audio wiring
│   ├── ram-asio/           # ASIO support (Windows)
│   │   └── cpp/                # C++ COM driver
│   ├── ram-vban/           # VBAN protocol
│   │   ├── protocol.rs         # Packet format
│   │   ├── jitter.rs           # Jitter buffer
│   │   ├── sender.rs           # UDP sender
│   │   └── receiver.rs         # UDP receiver
│   ├── ram-discovery/      # mDNS service
│   ├── ram-api/            # REST/WebSocket API
│   └── ram-service/        # Main service binary
│       ├── service.rs          # Service lifecycle
│       ├── audio_processor.rs  # Audio coordinator
│       ├── route_manager.rs    # Cross-node route & buffer management
│       ├── vban_manager.rs     # VBAN sender/receiver lifecycle
│       └── tray.rs             # Windows system tray (Windows-only)
└── docs/
    └── architecture/       # Detailed architecture docs
```

## Platform Support

| Platform | Backend | Status |
|----------|---------|--------|
| **Windows** | ASIO (CORE - always enabled) | Primary target |
| **Linux** | ALSA | Fully supported |
| **macOS** | CoreAudio | Basic support |

**IMPORTANT**: ASIO is a CORE feature on Windows, NOT optional:
- `ram-asio` is a mandatory dependency for Windows builds
- ASIO is always enabled by default in `ram-asio/Cargo.toml`
- CI verifies ASIO is compiled in on Windows builds
- Professional audio production requires ASIO latency - there is no fallback

## Windows System Tray Integration

The Windows service includes a system tray icon for easy access:

| Feature | Description |
|---------|-------------|
| **Version Display** | Show current version (vX.Y.Z) in tooltip and menu |
| **Self-Upgrade** | Check for updates and auto-upgrade to latest release |
| **Stats Display** | Show active streams, routes, buffer usage, latency |
| **Open Web UI** | Menu item to open web interface in default browser |
| **View Logs** | Open log file or monitoring console |
| **Start/Stop** | Control service state from tray menu |
| **Exit** | Graceful shutdown with confirmation |

Implementation: Use `tray-icon` crate with `muda` for menus.

## Key Design Decisions

1. **ASIO is CORE on Windows**: Professional audio requires ASIO - it is a mandatory dependency, not a feature flag
2. **Destination-Owned Routes**: Subscriptions stored at receiver for resilience
3. **Lock-Free Audio**: Atomics and ring buffers only, no mutexes in callbacks
4. **Single Matrix View**: All devices visible regardless of location
5. **VBAN for Network**: Compatible with VB-Audio ecosystem

## Next Steps

*All planned features have been implemented.*

## Known Technical Debt

- VbanManager dead code: `is_running()` and `sender_count()` methods unused
- Settings page not implemented (placeholder only)
- Stream monitor page not implemented

## Known Bugs

*No known bugs at this time.*

## Recently Completed

- **Sample Rate/Buffer Size Change Now Works** (2026-01-01): CRITICAL fix
  - Previously, changing sample rate or buffer size only updated stored value
  - Now triggers stream reconfiguration: stops and restarts streams with new config
  - Added `ReconfigureStreams` device command
  - Added `start_input_stream_with_config()` and `start_output_stream_with_config()` methods
  - Persists new config to device state file

- **Stream Start Failure Reporting** (2026-01-01): Improved device attachment error handling
  - Device status set to "Error" when all streams fail to start
  - WebSocket error event broadcast to UI for user feedback
  - Error logging with detailed stream failure messages
  - Fixes issue where device attach appeared to succeed but streams silently failed

- **Metering Subscription Node ID Fix** (2026-01-01): Fixed metering not appearing in UI
  - UI was subscribing with "LOCAL" fallback but server broadcast with actual node ID
  - Changed to reactive Effect that waits for `current_node` to be set
  - Metering now correctly matches between subscription and broadcast

- **Routing Matrix "(this device)" Indicator** (2026-01-01): UI consistency fix
  - Added "(this device)" label to source/dest node selectors in routing matrix
  - Matches the pattern used in Devices tab

- **WebSocket Schema Alignment** (2025-12-30): Fixed frontend/backend message format mismatch
  - Fixed serde tagging: `#[serde(tag = "type", content = "data")]` for WsEvent
  - Fixed serde tagging: `#[serde(tag = "command", content = "data")]` for WsCommand
  - Fixed metering node ID: now uses full `{name}@{hostname}` format
  - Verified: node_status, device_attached, metering subscription all working

- **Web UI Device Management** (2025-12-30): Complete device attachment workflow
  - Device cards with Attach/Detach buttons
  - Virtual device creation dialog
  - Proper API response type handling (VirtualDeviceResponse, AttachDeviceResponse)
  - WebSocket event handling for device_attached/device_detached

- **3-Node Network Deployment** (2025-12-30): All targets running v0.1.0-dev.15
  - stagebox1.lan (Windows) with tray icon
  - iem/Ableton-IEM (Windows) with tray icon under console user
  - develbox (Linux) with local binary

- **File Size Refactoring** (2025-12-29): All files now under 1000 lines per CLAUDE.md guidelines
  - `state.rs` (1275 lines) → `state/mod.rs` (957) + `devices.rs` (303) + `generators.rs` (108)
  - `device.rs` (1080 lines) → `device/mod.rs` (16) + `types.rs` (529) + `manager.rs` (569)
  - `handlers.rs` (1017 lines) → `handlers/mod.rs` (745) + `subscriptions.rs` (153)
  - `persistence.rs` (1011 lines) → `persistence/mod.rs` (37) + `config.rs` (342) + `store.rs` (455) + `types.rs` (228)

- **8-Phase Architecture Sync** (2025-12-29): Full implementation of architecture specifications
  - **Phase 1 - Zero Auto-Connect**: Added `auto_start_devices` config option (default: false)
  - **Phase 2 - Device State Machine**: Added `AttachmentState` enum (Available, Attached, Detached)
  - **Phase 3 - State Persistence**: Extended `PersistedConfig` with device attachments, channel labels, virtual devices
  - **Phase 4 - Device Attachment API**: Added attach/detach/rename endpoints with persistence
  - **Phase 5 - Channel Labels**: Per-channel naming with 31-char limit, bulk update support
  - **Phase 6 - Virtual Device CRUD API**: Create/update/delete virtual ASIO devices via REST
  - **Phase 7 - Sync Wave Generator**: Lock-free per-channel test signal injection (Sine, PinkNoise, ChannelId, Sweep, Click)
  - **Phase 8 - Unique Tray Icon**: Programmatic 3x3 grid with diamond symbols (Idle=blue, Active=green, Error=red)
  - All 278 workspace tests pass

- **Windows System Tray** (2025-12-29): System tray icon with menu for Windows
  - Version display in tooltip and menu header
  - Open Web UI menu item (opens browser to localhost:port)
  - View Logs menu item (opens log directory)
  - Check for Updates menu item (opens GitHub releases)
  - Exit menu item with graceful shutdown
  - Uses `tray-icon` and `muda` crates (Windows-only dependencies)
  - Tray runs in dedicated thread alongside service

- **Per-Device Metering Subscriptions** (2025-12-29): WebSocket metering filtering
  - Clients can subscribe/unsubscribe to specific devices
  - MeteringSubscriptions struct tracks per-client subscriptions
  - Backwards compatible: clients without subscriptions receive all meters
  - Commands: `subscribe_metering`, `unsubscribe_metering`

- **3-Node Network Deployment** (2025-12-29): Complete 3-point network operational
  - develbox (Linux) running 0.1.0-dev.8
  - stagebox1 (Windows) running 0.1.0-dev.8 with tray icon
  - Ableton-IEM (Windows) running 0.1.0-dev.8 with tray icon
  - All nodes discover each other, cross-node routes working
  - Task Scheduler used for GUI session access on Windows

- **Bidirectional Cross-Node Routing** (2025-12-29): Full support for local→remote routes
  - When route destination is remote, forwards route to destination node
  - Destination node receives route as remote→local and initiates VBAN subscription
  - Tested: develbox → stagebox1 routes correctly forward and create subscriptions
  - E2E verified: 3-point network (develbox, stagebox1, Ableton-IEM) all discovered

- **Web UI Cross-Node Support** (2025-12-29): Multi-node routing in web interface
  - Added node selectors for source and destination in routing matrix
  - Devices fetched dynamically from selected nodes via API
  - RouteCell component updated to pass node IDs for cross-node routes
  - AppState extended with source_devices, dest_devices, source_node, dest_node

- **Metering WebSocket Broadcast** (2025-12-29): Real-time levels to web clients
  - RouteController trait extended with metering methods
  - RouteManager wired to MeteringContexts via setter
  - Service broadcasts all input/output meters at 30 Hz

- **Cross-Node VBAN Auto-Creation** (2025-12-29): Automatic subscription on cross-node routes
  - When route source is remote node, automatically creates VBAN subscription
  - RouteController trait extended with `ensure_output_stream()` method
  - RouteManager handles output stream starter callback
  - Subscription initiates via HTTP POST to remote node's `/api/v1/subscriptions`
  - VBAN stream registered with buffer allocation on subscription acceptance
  - Tested: stagebox1 (Windows) → develbox (Linux) verified with ~187 Hz packet rate

- **UDP Broadcast Discovery** (2025-12-28): Reliable cross-node discovery
  - Added `BroadcastDiscovery` as fallback when mDNS doesn't work reliably
  - Nodes broadcast announcements every 5 seconds on UDP port 6981
  - 15-second timeout for stale node detection
  - Works alongside mDNS - first to discover wins
  - Tested: stagebox1 (Windows) ↔ develbox (Linux) now discover each other


- **Route Deletion Race Condition Fix** (2025-12-28): Fixed crash when deleting routes on Windows/ASIO
  - Implemented deferred buffer freeing in `RingBufferPool` with 2-generation grace period
  - Buffers are queued for deferred free and only actually freed after audio callbacks refresh their snapshots
  - `defer_free()` and `process_pending_frees()` methods added to `RingBufferPool`
  - Prevents race between API thread freeing buffers and ASIO callback reading them

- **mDNS Hostname Fix** (2025-12-28): Fixed hostname suffix for mDNS registration
  - Hostnames now end with `.local.` as required by mDNS spec
  - Service announcer successfully registers on both Windows and Linux

- **API ↔ AudioProcessor Wiring** (2025-12-28): COMPLETE
  - RouteController trait implemented by AudioProcessor
  - AppState.upsert_route() calls controller.add_route()
  - AppState.remove_route() calls controller.remove_route()
  - AppState.all_streams() queries controller.stream_registry()
  - AppState.all_subscriptions() queries controller.subscription_manager()
  - Volume/mute changes applied to live audio paths

- **Metering Infrastructure** (2025-12-28): Lock-free level measurement
  - `ChannelMeter` for atomic peak/RMS tracking (`metering.rs`)
  - `MeterBank` for multi-channel metering
  - Integrated into input/output callbacks
  - Linear and dB level conversion

- **REST API Completion** (2025-12-28): Full endpoint coverage
  - `/api/v1/streams` - List active audio streams
  - `/api/v1/streams/count` - Get stream counts
  - `/api/v1/subscriptions` - List cross-node subscriptions
  - `/api/v1/subscriptions/stats` - Subscription statistics
  - `/api/v1/routes/:id/latency` - Latency breakdown per route

- **Cross-Node Route Support** (2025-12-28): AudioProcessor integration
  - SubscriptionManager wired into AudioProcessor
  - `is_cross_node()` detection for routes
  - `calculate_route_latency()` with network/local distinction
  - Node name configuration

- **Subscription Protocol** (2025-12-28): Cross-node routing infrastructure
  - Subscribe/unsubscribe message types (`subscription.rs`)
  - SubscriptionManager for tracking active subscriptions (`subscription_manager.rs`)
  - Heartbeat and timeout handling
  - Device validation callbacks

- **Latency Measurement** (2025-12-28): Comprehensive latency tracking
  - LatencyCalculator for buffer-based latency estimation (`latency.rs`)
  - CallbackTimer for lock-free jitter measurement
  - LatencyReport for breakdown analysis (input, ring, network, output)
  - TimingStats for callback interval analysis

- **Audio Processing Loop** (2025-12-28): Full lock-free audio path implemented
  - RCU-based routing table (`routing_table.rs`, `routing_snapshot.rs`)
  - Pre-allocated ring buffer pool (`ring_buffer_pool.rs`)
  - Lock-free input/output callbacks (`callbacks.rs`)
  - Stream registry and lifecycle (`stream_registry.rs`, `active_stream.rs`)
  - AudioProcessor coordinator (`audio_processor.rs`)
  - Auto-start of default input/output devices on service startup
