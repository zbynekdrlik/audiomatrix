# Project Context

## Purpose

AudioMatrix is a professional network audio routing system that enables:
- Real-time audio streaming over IP networks using VBAN protocol
- Virtual ASIO device creation for DAW integration on Windows
- Flexible audio routing with per-connection mixing and volume control
- mDNS/DNS-SD auto-discovery for zero-configuration networking
- Multi-node coordination with a Dante-like subscription model

The system is designed for professional audio applications requiring low-latency,
lock-free audio processing with deterministic performance.

## Tech Stack

- **Language**: Rust 2021 edition (MSRV 1.75+)
- **Audio Backend**: cpal (cross-platform), ALSA (Linux), WASAPI (Windows)
- **Network Protocol**: VBAN (VB-Audio Network Protocol)
- **Discovery**: mDNS/DNS-SD via mdns-sd crate
- **API Framework**: Axum with WebSocket support
- **Async Runtime**: Tokio
- **DSP**: rubato (resampling), dasp (sample processing)
- **Serialization**: serde + serde_json

## Project Conventions

### Code Style

- Format with `cargo fmt` (rustfmt.toml configured)
- Lint with `cargo clippy -- -D warnings` (clippy.toml configured)
- Maximum 100 characters per line
- Maximum 1000 lines per file
- Use `imports_granularity = "Module"` and `group_imports = "StdExternalCrate"`
- Prefer descriptive names over comments

### Architecture Patterns

- **Destination-Owned Subscriptions**: Receivers initiate connections (like Dante)
- **Lock-Free Audio Path**: Use AtomicF32, AtomicBool, no Mutex in audio callbacks
- **CachePadded Atomics**: Prevent false sharing with 64-byte alignment
- **Message-Based Control**: flume channels for audio thread communication
- **Single Source of Truth**: ARCHITECTURE.md defines all design decisions

### Testing Strategy

- **TDD Approach**: Write tests before implementation
- **80% Minimum Coverage**: Enforced in CI with cargo-tarpaulin
- **Test Categories**:
  - Unit tests: Per-module correctness
  - Integration tests: Cross-crate behavior
  - Audio tests: Latency, jitter, sample accuracy
  - Property tests: Fuzzing with proptest

### Git Workflow

- **Branch Protection**: main requires PR approval + CI pass
- **Commit Format**: Conventional commits (`feat:`, `fix:`, `refactor:`, etc.)
- **PR Requirements**:
  - All CI checks pass
  - Audio safety checklist completed
  - No clippy warnings
  - Documentation updated for public API changes
- **Squash Merge**: Single commit per feature

## Domain Context

### Audio Processing Constraints

- **No Heap Allocation**: Pre-allocate all buffers before audio starts
- **No Blocking**: Never use Mutex::lock, file I/O, or network in audio callbacks
- **No Panics**: Use Result types, avoid unwrap()/expect() in audio path
- **Fixed Latency**: Buffer size determines latency (e.g., 256 samples @ 48kHz = 5.3ms)

### VBAN Protocol

- UDP-based, connectionless streaming
- 28-byte header + audio payload (max 1436 bytes)
- Sample rates: 6000 to 705600 Hz
- Up to 8 channels per stream
- Network byte order (big-endian) for header

### Key Concepts

- **Source**: Audio output endpoint (hardware device or VBAN sender)
- **Destination**: Audio input endpoint (hardware device or VBAN receiver)
- **Connection**: Routing from source to destination with volume/mute controls
- **Subscription**: Destination's request to receive from a specific source
- **Preset**: Named snapshot of routing configuration

## Important Constraints

### Performance Requirements

- **Max Latency**: 10ms end-to-end for local routing
- **Audio Callback Budget**: Must complete within buffer period
- **Sample Rate Support**: Native 44.1kHz, 48kHz, 96kHz with resampling
- **Buffer Sizes**: 64, 128, 256, 512, 1024 samples

### Security Constraints

- **API Authentication**: Bearer token with 24-hour expiry
- **WebSocket Auth**: Token in query string or first message
- **VBAN Warning**: Protocol has no built-in encryption (LAN only recommended)
- **Default Bind**: 127.0.0.1 (require explicit config for network access)

### Platform Support

- **Linux**: ALSA backend (primary development target)
- **Windows**: WASAPI + Virtual ASIO driver
- **macOS**: CoreAudio (future)

## External Dependencies

### Network Services

- **mDNS/DNS-SD**: Automatic service discovery (port 5353)
- **VBAN Protocol**: Audio transport (port 6980 default)
- **REST API**: Control interface (port 8080 default)
- **WebSocket**: Real-time state sync (same port as REST)

### Key Crates

| Crate | Purpose |
|-------|---------|
| cpal | Cross-platform audio I/O |
| axum | HTTP/WebSocket server |
| tokio | Async runtime |
| mdns-sd | Service discovery |
| rubato | Sample rate conversion |
| flume | Lock-free channels |
| parking_lot | Fast synchronization (control path only) |

## Crate Structure

```
crates/
├── ram-core/       # Audio buffer, mixer, routing, resampler
├── ram-vban/       # VBAN protocol implementation
├── ram-discovery/  # mDNS announce/browse
├── ram-asio/       # Virtual ASIO driver (Windows)
├── ram-api/        # REST/WebSocket API
└── ram-service/    # Main service binary
```

## OpenSpec Guidance

### When to Create Proposals

- New audio routing features
- Changes to VBAN protocol handling
- API endpoint additions/modifications
- WebSocket event schema changes
- New device backend support

### When to Skip Proposals

- Bug fixes restoring documented behavior
- Test additions for existing functionality
- Documentation updates
- Dependency updates (non-breaking)
- Internal refactoring without behavior change

### Naming Conventions

- Capabilities: `audio-routing`, `vban-transport`, `device-management`
- Changes: `add-volume-control`, `update-preset-schema`, `refactor-mixer`
