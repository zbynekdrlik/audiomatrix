# Design: Windows ASIO Core Architecture

## Overview

This document details the architecture for Windows ASIO support in AudioMatrix, covering hardware ASIO access, virtual ASIO devices, and the real-time audio routing engine.

## Component Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         DAW Applications                                 │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐       │
│  │ Ableton #1  │ │ Ableton #2  │ │ Pro Tools   │ │ Other DAW   │       │
│  └──────┬──────┘ └──────┬──────┘ └──────┬──────┘ └──────┬──────┘       │
│         │               │               │               │               │
│         ▼               ▼               ▼               ▼               │
│  ┌─────────────────────────────────────────────────────────────┐       │
│  │           Virtual ASIO Devices (ram-asio driver)            │       │
│  │  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐         │       │
│  │  │ VASIO-DAW1   │ │ VASIO-DAW2   │ │ VASIO-Main   │         │       │
│  │  │ 32ch in/out  │ │ 32ch in/out  │ │ 8ch in/out   │         │       │
│  │  └──────┬───────┘ └──────┬───────┘ └──────┬───────┘         │       │
│  └─────────┼────────────────┼────────────────┼─────────────────┘       │
│            │                │                │                          │
│            ▼                ▼                ▼                          │
│  ┌─────────────────────────────────────────────────────────────┐       │
│  │                   Audio Routing Matrix                       │       │
│  │                      (ram-core)                              │       │
│  │                                                              │       │
│  │   Lock-free ring buffers, per-channel routing, gain/mute    │       │
│  └──────────────────────────┬──────────────────────────────────┘       │
│                             │                                           │
│            ┌────────────────┼────────────────┐                         │
│            ▼                ▼                ▼                         │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐                    │
│  │ Hardware     │ │ VBAN Network │ │ Other Local  │                    │
│  │ ASIO Device  │ │ Transport    │ │ ASIO Devices │                    │
│  │ (Dante/RME)  │ │ (UDP)        │ │              │                    │
│  └──────────────┘ └──────────────┘ └──────────────┘                    │
└─────────────────────────────────────────────────────────────────────────┘
```

## Virtual ASIO Driver Approach

### Option Analysis

#### 1. Kernel-Mode WDM Driver (VB-Audio style)
- **Pros**: True ASIO device, lowest latency, full control
- **Cons**: Complex development, requires driver signing, Windows Update risks
- **Examples**: VB-Audio Cable, Synchronous Audio Router

#### 2. ASIO Wrapper Driver
- **Pros**: Simpler than kernel driver, user-mode
- **Cons**: Still requires ASIO SDK, limited flexibility
- **Examples**: ASIO4ALL approach

#### 3. COM-based Virtual ASIO Host
- **Pros**: Pure user-mode, no kernel driver needed
- **Cons**: May not appear as separate ASIO device in all DAWs
- **Approach**: Register as ASIO driver in registry, handle COM interface

### Recommended Approach: COM-based Virtual ASIO

1. **Create ASIO driver DLL** that implements `IASIO` interface
2. **Register in Windows Registry** under `HKLM\SOFTWARE\ASIO\AudioMatrix-Virtual`
3. **Multiple instances** via separate registry entries
4. **Inter-process communication** with AudioMatrix service via shared memory

```
Registry Structure:
HKLM\SOFTWARE\ASIO\
├── AudioMatrix-VASIO-1\
│   ├── CLSID = {guid}
│   ├── Description = "AudioMatrix Virtual 1 (32ch)"
│   └── ChannelCount = 32
├── AudioMatrix-VASIO-2\
│   ├── CLSID = {guid}
│   ├── Description = "AudioMatrix Virtual 2 (32ch)"
│   └── ChannelCount = 32
```

## Real-Time Audio Path

### Buffer Flow

```
Hardware ASIO Input
        │
        ▼ (ASIO bufferSwitch callback)
┌───────────────────┐
│ Input Ring Buffer │  Lock-free SPSC queue
│ (per channel)     │  Pre-allocated, fixed size
└────────┬──────────┘
         │
         ▼ (Routing thread, high priority)
┌───────────────────┐
│ Routing Matrix    │  Reads from input buffers
│                   │  Applies gain/mute
│                   │  Writes to output buffers
└────────┬──────────┘
         │
         ▼
┌───────────────────┐
│ Output Ring Buffer│  Lock-free SPSC queue
│ (per channel)     │  Pre-allocated, fixed size
└────────┬──────────┘
         │
         ▼ (ASIO bufferSwitch callback)
Virtual ASIO Output (to DAW)
```

### Thread Model

| Thread | Priority | Responsibility |
|--------|----------|----------------|
| ASIO Callback | REALTIME | Copy samples to/from ring buffers only |
| Router | HIGH | Matrix routing, gain, mute, metering |
| Network I/O | HIGH | VBAN send/receive |
| API | NORMAL | REST/WebSocket handling |
| Discovery | LOW | mDNS browsing |

### Lock-Free Requirements

The ASIO callback thread must NEVER:
- Allocate memory
- Take locks/mutexes
- Make system calls
- Block on I/O

All inter-thread communication via:
- Atomic operations
- Lock-free ring buffers (crossbeam-channel or custom SPSC)
- Pre-allocated buffers

## Network Audio (VBAN)

### Packet Flow

```
Local ASIO Input → Ring Buffer → VBAN Sender → UDP → Network
                                                         │
                                                         ▼
Network → UDP → VBAN Receiver → Ring Buffer → Remote Virtual ASIO Output
```

### Latency Budget (64-sample @ 48kHz)

| Component | Target | Max |
|-----------|--------|-----|
| ASIO buffer | 1.33ms | 1.33ms |
| Ring buffer copy | <0.1ms | 0.2ms |
| Network transit | ~1ms | 3ms |
| Jitter buffer | 1-3ms | 5ms |
| Total local | ~1.5ms | 2ms |
| Total network | ~4ms | 8ms |

## API Design

### Virtual Device Management

```
POST /api/v1/virtual-devices
{
  "name": "VASIO-DAW1",
  "channels": 32,
  "sample_rates": [44100, 48000, 96000]
}

DELETE /api/v1/virtual-devices/{id}

GET /api/v1/virtual-devices
[
  {"id": "vasio-1", "name": "VASIO-DAW1", "channels": 32, "in_use": true},
  {"id": "vasio-2", "name": "VASIO-DAW2", "channels": 32, "in_use": false}
]
```

### Route Configuration (existing, enhanced)

```
POST /api/v1/routes
{
  "source": {"node": "local", "device": "Dante-PCIe", "channel": 1},
  "destination": {"node": "local", "device": "VASIO-DAW1", "channel": 1},
  "gain": 0.0,
  "mute": false
}
```

## File Structure

```
crates/ram-asio/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API
│   ├── host.rs             # ASIO host enumeration
│   ├── device.rs           # Hardware ASIO device wrapper
│   ├── stream.rs           # ASIO stream management
│   ├── callback.rs         # bufferSwitch handler
│   └── virtual/
│       ├── mod.rs          # Virtual device module
│       ├── driver.rs       # ASIO COM driver implementation
│       ├── registry.rs     # Windows registry management
│       └── ipc.rs          # Shared memory communication
```

## Dependencies

- **ASIO SDK**: Steinberg ASIO SDK 2.3+ (compile-time)
- **windows-rs**: Windows API bindings
- **crossbeam**: Lock-free data structures
- **shared_memory** or **memmap2**: IPC with virtual driver

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Virtual driver complexity | Start with single virtual device, expand later |
| Driver signing requirements | Use test signing initially, proper signing for release |
| DAW compatibility | Test with Ableton first, then other DAWs |
| Latency regression | Continuous benchmarking, latency tests in CI |
