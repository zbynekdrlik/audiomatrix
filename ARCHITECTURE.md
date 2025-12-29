# AudioMatrix - System Architecture

## Overview

A Dante-like audio routing system built in Rust, providing unified control over local and networked audio devices with ~2.7ms local latency and ~4ms network latency.

**Core Principles:**
- **Unified Matrix**: All devices (local and remote) in one seamless routing matrix
- **Network Transparency**: Cross-computer routing identical to local routing
- **Dynamic Devices**: Create/resize virtual ASIO devices on demand
- **Ultra-Low Latency**: Lock-free audio paths, real-time priorities
- **Destination-Owned**: Receiver-centric subscriptions (like Dante)

## Architecture Documents

| Document | Description |
|----------|-------------|
| [devices.md](docs/architecture/devices.md) | Device model, virtual ASIO lifecycle |
| [routing.md](docs/architecture/routing.md) | Matrix controller, connections, controls |
| [network.md](docs/architecture/network.md) | VBAN protocol, mDNS, cross-computer |
| [threading.md](docs/architecture/threading.md) | Thread model, lock-free design, latency |
| [state.md](docs/architecture/state.md) | Persistence, subscriptions, configuration |
| [api.md](docs/architecture/api.md) | REST API, WebSocket, mDNS service |

## Implementation Status

> **Current Version:** 0.1.0-dev.7
> **Last Updated:** 2025-12-29

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
| **REST API** | Complete | ram-api | Route CRUD wired to AudioProcessor via RouteController trait |
| **WebSocket Events** | Complete | ram-api | Event types, metering broadcast at 30Hz |
| **Configuration** | Complete | ram-core | JSON persistence |
| **Audio Processing Loop** | Complete | ram-service | cpal streams, auto-start default devices |
| **Subscription Protocol** | Complete | ram-core | Subscribe/unsubscribe messages, manager |
| **Latency Measurement** | Complete | ram-core | Calculator, callback timer, jitter tracking |
| **Metering** | Complete | ram-core | Lock-free level meters, peak/RMS, per-channel |
| **Cross-Node VBAN Auto-Creation** | Complete | ram-service | Auto-subscription on cross-node routes (bidirectional) |
| **Web UI Cross-Node Support** | Complete | ram-ui | Node selectors, cross-node route creation |

**Legend:** Complete = Working | Partial = Structure exists | Not Started = Planned

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
│       └── vban_manager.rs     # VBAN sender/receiver lifecycle
└── docs/
    └── architecture/       # Detailed architecture docs
```

## Platform Support

| Platform | Backend | Status |
|----------|---------|--------|
| **Windows** | ASIO (primary), WASAPI (fallback) | Primary target |
| **Linux** | ALSA | Fully supported |
| **macOS** | CoreAudio | Basic support |

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

1. **ASIO-First on Windows**: Professional audio production requires ASIO latency
2. **Destination-Owned Routes**: Subscriptions stored at receiver for resilience
3. **Lock-Free Audio**: Atomics and ring buffers only, no mutexes in callbacks
4. **Single Matrix View**: All devices visible regardless of location
5. **VBAN for Network**: Compatible with VB-Audio ecosystem

## Next Steps

Priority order for remaining work:

1. **Windows System Tray**: Implement tray icon with menu for service control
2. **Split state.rs**: File exceeds 1000 lines (currently 1027) - extract subscription/stream logic
3. **Ableton-IEM Update**: Deploy 0.1.0-dev.7 to complete 3-node network
4. **Per-Device Metering Subscriptions**: Optional WebSocket filtering by device

## Known Technical Debt

- `ram-api/state.rs` exceeds 1000 lines (1027) - needs refactoring
- Windows system tray integration not yet implemented
- VbanManager dead code: `is_running()` and `sender_count()` methods unused
- Per-device metering subscriptions not implemented (broadcasts all meters)

## Known Bugs

*No known bugs at this time.*

## Recently Completed

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
