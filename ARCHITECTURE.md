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

> **Current Version:** 0.1.0-dev.2
> **Last Updated:** 2025-12-28

| Component | Status | Crate | Notes |
|-----------|--------|-------|-------|
| **ASIO Device Support** | Complete | ram-asio | Enumeration, device access via cpal |
| **Virtual ASIO (Rust)** | Complete | ram-asio | Shared memory IPC |
| **Virtual ASIO (C++)** | Partial | ram-asio/cpp | Skeleton created, CI build pending |
| **Device Enumeration** | Complete | ram-core | All platforms (ASIO, WASAPI, ALSA) |
| **Audio Routing Matrix** | Complete | ram-core | Routes, connections, engine |
| **Per-connection Controls** | Complete | ram-core | Lock-free gain/mute/enabled |
| **Resampling** | Complete | ram-core | Rubato integration |
| **VBAN Protocol** | Complete | ram-vban | Full encode/decode |
| **VBAN Streaming** | Complete | ram-vban | Sender, receiver, jitter buffer |
| **mDNS Discovery** | Complete | ram-discovery | Announce and browse |
| **REST API** | Partial | ram-api | Basic endpoints |
| **WebSocket Events** | Partial | ram-api | Event types defined |
| **Configuration** | Complete | ram-core | TOML persistence |
| **Audio Processing Loop** | Not Started | - | Infrastructure ready |
| **Subscription Protocol** | Not Started | - | Messages not implemented |
| **Latency Measurement** | Not Started | - | No infrastructure |

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
│   ├── ram-core/        # Audio engine, routing, devices
│   ├── ram-asio/        # ASIO support (Windows)
│   ├── ram-vban/        # VBAN protocol
│   ├── ram-discovery/   # mDNS service
│   ├── ram-api/         # REST/WebSocket API
│   └── ram-service/     # Main service binary
└── docs/
    └── architecture/    # Detailed architecture docs
```

## Platform Support

| Platform | Backend | Status |
|----------|---------|--------|
| **Windows** | ASIO (primary), WASAPI (fallback) | Primary target |
| **Linux** | ALSA | Fully supported |
| **macOS** | CoreAudio | Basic support |

## Key Design Decisions

1. **ASIO-First on Windows**: Professional audio production requires ASIO latency
2. **Destination-Owned Routes**: Subscriptions stored at receiver for resilience
3. **Lock-Free Audio**: Atomics and ring buffers only, no mutexes in callbacks
4. **Single Matrix View**: All devices visible regardless of location
5. **VBAN for Network**: Compatible with VB-Audio ecosystem

## Next Steps

Priority order for remaining work:

1. **Audio Processing Loop**: Wire ring buffers to ASIO callbacks
2. **Virtual ASIO C++ Build**: Complete CI integration
3. **Subscription Protocol**: Implement SUBSCRIBE/UNSUBSCRIBE messages
4. **Auto Stream Creation**: Create VBAN streams on cross-node routes
5. **Metering**: Real-time level monitoring
