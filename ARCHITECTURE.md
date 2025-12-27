# AudioMatrix - Open Source Audio Routing System

## Overview

A Dante-like audio routing system built in Rust, providing unified control over local and networked audio devices with ~2.7ms local latency (at 64-sample buffer) and ~4ms network latency.

**Core Principles:**
- **Unified Matrix**: All devices (local and remote) appear in one seamless routing matrix
- **Network Transparency**: Cross-computer routing works identically to local routing
- **Dynamic Devices**: Create, configure, and resize virtual ASIO devices on demand
- **Ultra-Low Latency**: Lock-free audio paths, optimized buffers, real-time priorities
- **Dante-Like UX**: Expandable/collapsible devices, filtering, familiar workflow

## Implementation Status

> **Current Version:** 0.1.0-dev
> **Last Updated:** 2025-12-27

| Component | Status | Notes |
|-----------|--------|-------|
| **Device Enumeration** | 🟡 Partial | WASAPI/CoreAudio/ALSA working. ASIO requires SDK. |
| **Virtual ASIO Devices** | 🔴 Not Started | `ram-asio` crate not yet implemented |
| **Audio Routing Matrix** | 🟡 Partial | Data structures complete, no audio processing |
| **Real-time Audio Loop** | 🔴 Not Started | No ASIO callback integration yet |
| **VBAN Protocol** | 🟢 Complete | Full protocol parsing/generation working |
| **VBAN Integration** | 🔴 Not Started | Protocol not connected to routing engine |
| **Cross-computer Routing** | 🔴 Not Started | Network audio path not implemented |
| **mDNS Discovery** | 🟢 Complete | Full service announcement and browsing |
| **REST API** | 🟡 Partial | Endpoints exist, limited functionality |
| **WebSocket Events** | 🟡 Partial | Events defined, metering not implemented |
| **Configuration Persistence** | 🟢 Complete | Routes and settings persist to disk |
| **Per-connection Controls** | 🟡 Partial | Gain/mute atomics exist, not applied |
| **Resampling** | 🔴 Not Started | Rubato included but never called |
| **Latency Measurement** | 🔴 Not Started | No measurement infrastructure |

**Legend:** 🟢 Complete | 🟡 Partial | 🔴 Not Started

### Platform Notes

- **Windows**: Enumerates all hosts (WASAPI + ASIO if SDK installed). ASIO4ALL or native ASIO driver recommended for low latency.
- **Linux**: ALSA only. PipeWire/PulseAudio may work through ALSA compatibility.
- **macOS**: CoreAudio only.

## System Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                                    Network (LAN)                                 │
│                                                                                  │
│  ┌─────────────────────────┐      ┌─────────────────────────┐                   │
│  │      STUDIO-PC          │      │      LIVE-PC            │                   │
│  │                         │      │                         │                   │
│  │  ┌───────────────────┐  │      │  ┌───────────────────┐  │                   │
│  │  │ AudioMatrix       │  │      │  │ AudioMatrix       │  │                   │
│  │  │ Service           │  │      │  │ Service           │  │                   │
│  │  │                   │  │      │  │                   │  │                   │
│  │  │ Devices:          │  │      │  │ Devices:          │  │                   │
│  │  │ • Focusrite 18i20 │◄─┼──────┼──► RME Fireface     │  │                   │
│  │  │ • VASIO-DAW (32ch)│  │      │  │ • VASIO-Main (8ch)│  │                   │
│  │  │ • VASIO-FX (16ch) │  │      │  │ • VASIO-Mon (4ch) │  │                   │
│  │  └───────────────────┘  │      │  └───────────────────┘  │                   │
│  └────────────┬────────────┘      └────────────┬────────────┘                   │
│               │           mDNS + VBAN          │                                │
│               └────────────────┬───────────────┘                                │
│                                │                                                │
│                    ┌───────────▼───────────┐                                    │
│                    │   Matrix Controller    │                                    │
│                    │   (Web Interface)      │                                    │
│                    │                        │                                    │
│                    │  Any browser, any PC   │                                    │
│                    └────────────────────────┘                                    │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## Device Model

### Device Identification

Every audio endpoint is uniquely identified as `{computer}:{device}`:

```
STUDIO-PC:Focusrite 18i20      # Physical ASIO device
STUDIO-PC:VASIO-DAW            # Virtual ASIO device
LIVE-PC:RME Fireface           # Physical on another computer
LIVE-PC:VASIO-Main             # Virtual on another computer
```

### Device Types

| Type | Description | Channel Config | Visibility |
|------|-------------|----------------|------------|
| **Physical ASIO** | Hardware audio interface | Fixed by hardware | Auto-detected |
| **Virtual ASIO** | Software device for DAW routing | User-configurable | Created on demand |

### Virtual ASIO Device Management

Virtual ASIO devices can be:
- **Created** with custom name and initial channel count
- **Resized** to expand or shrink channel count without recreation
- **Deleted** when no longer needed

```
Device: STUDIO-PC:VASIO-DAW
├── Input Channels:  32 (configurable: 2-256)
├── Output Channels: 32 (configurable: 2-256)
├── Sample Rate:     48000 Hz
├── Buffer Size:     64 samples (1.33ms)
└── Status:          Active (Reaper connected)
```

**Channel Resizing:**
- Shrinking: Removes highest-numbered channels, disconnects affected routes
- Expanding: Adds new channels, ready for immediate routing
- Live operation: No restart required, DAW sees updated channel count

**Virtual ASIO Lifecycle:**

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         VIRTUAL ASIO DEVICE LIFECYCLE                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  1. CREATE (via API or config)                                                  │
│     - Generate unique CLSID for COM registration                                │
│     - Register in Windows Registry (HKLM\SOFTWARE\ASIO)                         │
│     - Device appears in ASIO device lists                                       │
│     - DAW must rescan to see new device (ASIO limitation)                       │
│                                                                                  │
│  2. CONNECT (DAW opens device)                                                  │
│     - DAW calls ASIOInit() on the virtual driver                                │
│     - Driver connects to AudioMatrix service via shared memory/pipe            │
│     - Audio buffers allocated based on DAW's requested buffer size             │
│     - ASIOStart() begins audio callback loop                                    │
│                                                                                  │
│  3. RESIZE (while DAW connected)                                                │
│     - Service updates channel count in shared state                             │
│     - Driver signals ASIOResetRequest to DAW                                    │
│     - DAW re-queries channel count, sees new value                              │
│     - No disconnect required                                                    │
│                                                                                  │
│  4. DELETE (device removal)                                                     │
│     - If DAW connected: ASIOStop() + ASIODisposeBuffers() first               │
│     - Disconnect all routes using this device                                   │
│     - Unregister COM object                                                     │
│     - Remove from Windows Registry                                              │
│     - Delete config file                                                        │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## Matrix Controller Interface

### Layout (Dante Controller Style)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  [Filter: ________] [Computers: All ▼] [Show Offline ☐]              [⚙ Config] │
├─────────────────────────────────────────────────────────────────────────────────┤
│                          SOURCES (Transmitters)                                  │
│             ┌─────────────────────────────────────────────────────────┐         │
│             │ STUDIO-PC          │ LIVE-PC            │               │         │
│             │ ▼ Focusrite 18i20  │ ▼ RME Fireface     │               │         │
│             │   1  2  3  4 ···18 │   1  2  3  4 ··· 8 │               │         │
├─────────────┼────────────────────┼────────────────────┼───────────────┤         │
│ D           │                    │                    │               │         │
│ E  STUDIO-PC│                    │                    │               │         │
│ S  ▼ VASIO-DAW                   │                    │               │         │
│ T    1      │ ●                  │                    │               │         │
│ I    2      │    ●               │                    │               │         │
│ N    3      │       ●            │                    │               │         │
│ A    4      │          ●         │                    │               │         │
│ T  ▼ VASIO-FX                    │                    │               │         │
│ I    1      │                    │ ●                  │               │         │
│ O    2      │                    │    ●               │               │         │
│ N  LIVE-PC  │                    │                    │               │         │
│ S  ▼ VASIO-Main                  │                    │               │         │
│      1      │ ●                  │                    │               │         │
│      2      │    ●               │                    │               │         │
└─────────────┴────────────────────┴────────────────────┴───────────────┘

● = Active connection (routed)
○ = Available (can connect)
─ = Incompatible (sample rate mismatch, etc.)
```

### Connection Properties (Per-Crosspoint)

Every connection point has individual controls (like VB-Matrix):

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         PER-CONNECTION CONTROLS                                  │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  Click on any connection (●) to open control popup:                             │
│                                                                                  │
│  ┌─────────────────────────────────────────┐                                    │
│  │ STUDIO-PC:Focusrite:1 → VASIO-DAW:1     │                                    │
│  │                                         │                                    │
│  │  Volume ──────────────────────────────  │                                    │
│  │  [━━━━━━━━━━━━━━━━━━●━━━] -2.4 dB      │                                    │
│  │   -∞                  0            +12  │                                    │
│  │                                         │                                    │
│  │  [🔇 Mute]  [S Solo]  [× Remove]       │                                    │
│  │                                         │                                    │
│  │  ─────────────────────────────────────  │                                    │
│  │  Status: Active | Latency: 2.7ms        │  ← Total end-to-end latency        │
│  │  Level: ████████░░ -6.2 dBFS           │                                    │
│  └─────────────────────────────────────────┘                                    │
│                                                                                  │
│  Keyboard shortcuts (when cell selected):                                       │
│  • M = Toggle mute                                                              │
│  • S = Solo this connection                                                     │
│  • Delete = Remove connection                                                   │
│  • +/- = Adjust gain ±1dB                                                       │
│  • Shift +/- = Adjust gain ±0.1dB                                               │
│                                                                                  │
│  Visual indicators on matrix:                                                   │
│  ● = Active (normal)                                                            │
│  ◐ = Muted (half-filled or different color)                                    │
│  ●̲ = Gain adjusted (underline or color tint)                                   │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Connection Data Model:**

```typescript
// Channel reference - always uses 1-based indexing in API/UI
interface ChannelRef {
  computer: string;              // e.g., "STUDIO-PC"
  device: string;                // e.g., "Focusrite 18i20"
  channel: number;               // 1-based (1, 2, 3...)
}

interface Connection {
  id: string;                      // Unique ID: "{src_computer}:{src_device}:{src_ch}>{dst_computer}:{dst_device}:{dst_ch}"
  source: ChannelRef;
  destination: ChannelRef;         // In subscriptions.toml, destination.computer is implicit (always local)

  // Per-connection controls
  gain_db: number;                 // -60 to +12 dB (≤-60 = silence, default: 0)
  mute: boolean;                   // default: false
  enabled: boolean;                // Master enable - false removes from audio path entirely
  solo: boolean;                   // UI-only, not persisted (see Solo Behavior below)

  // Versioning for conflict resolution
  version: number;                 // Incremented on each modification

  // Status (partially persisted - see table below)
  status: "active" | "waiting" | "pending" | "error";
  latency_ms?: number;             // Measured latency (runtime only)
  level_dbfs?: number;             // Current signal level -60 to 0 (runtime only)
  last_active?: string;            // ISO timestamp of last active state
  error_reason?: string;           // Populated when status == "error"
}
```

**Status States:**
| State | Meaning | Persisted? | Location |
|-------|---------|------------|----------|
| `active` | Audio flowing | No | Runtime only |
| `waiting` | Subscription stored, source offline | Yes | subscriptions.toml |
| `pending` | Destination offline, queued | Yes | controller-pending.toml |
| `error` | Failed (sample rate mismatch, etc.) | Yes | subscriptions.toml (with error_reason) |

**Why partial persistence?**
- `active` has no meaning after restart (must re-establish)
- `waiting` and `error` are persisted so service knows what to retry/report on startup
- `pending` is controller-side only (destination can't store it - it's offline)

**Solo Behavior:**
Solo is UI-only and works per-destination:
- Solo source A to destination X → mutes all OTHER sources to destination X
- Does NOT affect other destinations
- Multiple solos allowed (solo A and B = hear only A+B)
- Clear all solos with Escape key

**Subscription Storage with Controls:**

```toml
# subscriptions.toml - authoritative source for what THIS computer receives
# Note: destination.computer is implicit (always this computer)
# Note: channel numbers are 1-based

[[subscription]]
# ID format: {src_computer}:{src_device}:{src_ch}>{dst_computer}:{dst_device}:{dst_ch}
# Note: In subscriptions.toml, dst_computer is always THIS computer (implicit)
id = "STUDIO-PC:Focusrite 18i20:1>LOCAL:VASIO-DAW:1"
destination = { device = "VASIO-DAW", channel = 1 }
source = { computer = "STUDIO-PC", device = "Focusrite 18i20", channel = 1 }
gain_db = -2.4          # Range: -60 to +12 (≤-60 = silence)
mute = false            # Silences output but keeps connection
enabled = true          # Master enable - false removes from audio path entirely
version = 3             # Incremented on each modification (for conflict resolution)

[[subscription]]
id = "STUDIO-PC:Focusrite 18i20:2>LOCAL:VASIO-DAW:2"
destination = { device = "VASIO-DAW", channel = 2 }
source = { computer = "STUDIO-PC", device = "Focusrite 18i20", channel = 2 }
gain_db = 0.0           # Unity gain
mute = true             # Currently muted (audio still processed, just silenced)
enabled = true
version = 1

[[subscription]]
id = "LIVE-PC:RME:5>LOCAL:VASIO-FX:1"
destination = { device = "VASIO-FX", channel = 1 }
source = { computer = "LIVE-PC", device = "RME", channel = 5 }
gain_db = 6.0           # +6dB boost
mute = false
enabled = true
version = 2
```

**`enabled` vs `mute` Semantics:**
| enabled | mute | Result |
|---------|------|--------|
| true | false | Audio flows normally |
| true | true | Audio processed but output silenced (zero samples) |
| false | * | Connection removed from audio path entirely (saves CPU) |

**Gain Implementation (Lock-Free):**

```rust
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

/// Atomic f32 wrapper using bit-casting (std doesn't provide AtomicF32)
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }

    pub fn load(&self, order: Ordering) -> f32 {
        f32::from_bits(self.0.load(order))
    }

    pub fn store(&self, v: f32, order: Ordering) {
        self.0.store(v.to_bits(), order);
    }
}

pub struct SourceConnection {
    input_buffer: Arc<AudioRingBuffer>,
    gain: AtomicF32,           // Linear gain (linear, not dB), updated atomically
    mute: AtomicBool,          // Mute flag
    enabled: AtomicBool,       // Master enable - false skips processing entirely
}

impl SourceConnection {
    /// Process audio with gain and mute (lock-free, audio thread safe)
    /// Called from audio thread - must not allocate or block
    #[inline]
    pub fn process(&self, output: &mut [f32], temp_buffer: &mut [f32]) {
        // Skip entirely if disabled (saves CPU)
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }

        // Skip if muted (still "connected" but silent)
        if self.mute.load(Ordering::Relaxed) {
            return;
        }

        let gain = self.gain.load(Ordering::Relaxed);

        // Read from source buffer and apply gain
        let samples_read = self.input_buffer.read(&mut temp_buffer[..output.len()]);

        for i in 0..samples_read {
            output[i] += temp_buffer[i] * gain;  // Accumulate for mixing
        }
    }

    /// Set gain in dB (called from control thread, not audio thread)
    /// Range: -60dB to +12dB (values ≤-60dB treated as silence)
    pub fn set_gain_db(&self, db: f32) {
        let linear = if db <= -60.0 {
            0.0  // Treat ≤-60dB as silence (-∞)
        } else {
            10.0_f32.powf(db / 20.0)
        };
        self.gain.store(linear, Ordering::Relaxed);
    }

    pub fn set_mute(&self, mute: bool) {
        self.mute.store(mute, Ordering::Relaxed);
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }
}
```

**Bulk Operations:**

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  Select multiple connections (Ctrl+click or drag-select):                       │
│                                                                                  │
│  ┌─────────────────────────────────────────┐                                    │
│  │ 4 connections selected                  │                                    │
│  │                                         │                                    │
│  │  Relative Gain ───────────────────────  │                                    │
│  │  [━━━━━━━━━━●━━━━━━━━━] +3.0 dB        │  ← Adds to existing gains          │
│  │                                         │                                    │
│  │  [🔇 Mute All]  [Unmute All]  [× Remove All]                                │
│  │                                         │                                    │
│  │  [Set All to 0dB]  [Copy Gains]  [Paste Gains]                              │
│  └─────────────────────────────────────────┘                                    │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Hierarchy & Expansion

Each level is independently collapsible:

```
▼ STUDIO-PC                    # Computer expanded
  ▼ Focusrite 18i20            # Device expanded (shows channels)
      1: Mic 1
      2: Mic 2
      ...
  ► VASIO-DAW                  # Device collapsed (single row)
  ► VASIO-FX                   # Device collapsed

► LIVE-PC                      # Computer collapsed (single row for all)
```

### Filtering

The matrix can become very large. Filters reduce visible scope:

| Filter Type | Example | Effect |
|-------------|---------|--------|
| **Text search** | `VASIO` | Shows only devices containing "VASIO" |
| **Computer** | `STUDIO-PC` | Shows only devices on STUDIO-PC |
| **Regex** | `^VASIO-.*$` | Pattern matching |
| **Presets** | "Studio Only" | Saved filter combinations |

## Network Transparency

### Unified Routing Model

The user sees **one matrix** regardless of whether connections are local or cross-network:

```
Connection: STUDIO-PC:Focusrite[1] → LIVE-PC:VASIO-Main[1]

User sees:     Simple crosspoint click (●)
System does:
  1. STUDIO-PC creates VBAN sender for channel 1
  2. LIVE-PC creates VBAN receiver
  3. Receiver output connects to VASIO-Main input 1
  4. Latency/jitter monitoring starts
```

### Connection Types (Internal)

| Route Type | Implementation | Typical Latency |
|------------|----------------|-----------------|
| **Local** | Direct memory copy | <0.5ms |
| **Cross-computer** | VBAN over UDP | 1-3ms |
| **Resampled** | Via resampler stage | +0.5ms |

**Key Principle:** The transport mechanism (VBAN) is never exposed in the main UI. Users work with a unified abstraction. Technical details are available in:
- Device properties dialog
- Connection info tooltip
- Advanced diagnostics panel

### Device Properties Dialog

```
┌─────────────────────────────────────────────────────────────┐
│ Device: LIVE-PC:RME Fireface                          [×]   │
├─────────────────────────────────────────────────────────────┤
│ General                                                      │
│   Type:           Physical ASIO                             │
│   Computer:       LIVE-PC (192.168.1.42)                    │
│   Status:         Online                                    │
│                                                              │
│ Audio Configuration                                          │
│   Sample Rate:    48000 Hz                                  │
│   Buffer Size:    64 samples                                │
│   Input Channels: 8                                         │
│   Output Channels: 8                                        │
│                                                              │
│ Network (for cross-computer routes)                          │
│   Transport:      VBAN                                      │
│   Active Streams: 2 TX, 1 RX                                │
│   Avg Latency:    2.1ms                                     │
│   Jitter:         ±0.3ms                                    │
│                                                              │
│ [Rename] [Configure Channels] [Disconnect All]              │
└─────────────────────────────────────────────────────────────┘
```

## Ultra-Low Latency Architecture

### Design Principles

1. **Zero-Copy Where Possible**: Audio data stays in place, pointers move
2. **Lock-Free Audio Path**: No mutexes in real-time audio callbacks
3. **Minimal Buffering**: Smallest safe buffer sizes (64-256 samples)
4. **Real-Time Thread Priority**: ASIO callbacks run at highest OS priority
5. **Batch Operations**: Group small packets, reduce syscall overhead

### Audio Path Latency Budget

```
Local Route (DAW → Physical Output):
┌────────────────────────────────────────────────────────┐
│ VASIO Buffer     │ 64 samples  │ 1.33ms @ 48kHz       │
│ Matrix Copy      │ ~0 samples  │ <0.01ms (lock-free)  │
│ Physical Buffer  │ 64 samples  │ 1.33ms @ 48kHz       │
├────────────────────────────────────────────────────────┤
│ TOTAL            │             │ ~2.7ms               │
└────────────────────────────────────────────────────────┘

Network Route (Computer A → Computer B):
┌────────────────────────────────────────────────────────┐
│ Source VASIO     │ 64 samples  │ 1.33ms               │
│ VBAN Packetize   │ -           │ <0.1ms               │
│ Network Transit  │ -           │ 0.1-0.5ms (LAN)      │
│ Jitter Buffer    │ 48 samples  │ 1.0ms (adaptive)     │
│ Dest Matrix      │ ~0 samples  │ <0.01ms              │
│ Physical Buffer  │ 64 samples  │ 1.33ms               │
├────────────────────────────────────────────────────────┤
│ TOTAL            │             │ ~3.8ms               │
└────────────────────────────────────────────────────────┘
```

### Lock-Free Ring Buffer

```rust
// Note: Uses crossbeam_utils::CachePadded to prevent false sharing
// between producer and consumer positions on different CPU cores
use crossbeam_utils::CachePadded;

/// Single-producer, single-consumer lock-free ring buffer
/// Optimized for audio: cache-line aligned, no false sharing
pub struct AudioRingBuffer {
    buffer: Box<[f32]>,                    // Contiguous sample storage
    capacity: usize,                       // Power of 2 for fast modulo
    write_pos: CachePadded<AtomicUsize>,   // Producer position (64-byte aligned)
    read_pos: CachePadded<AtomicUsize>,    // Consumer position (64-byte aligned)
}

impl AudioRingBuffer {
    /// Write samples without blocking
    /// Returns: number of samples actually written (may be less if buffer full)
    pub fn write(&self, samples: &[f32]) -> usize;

    /// Read samples without blocking
    /// Returns: number of samples actually read (may be less if buffer empty)
    pub fn read(&self, output: &mut [f32]) -> usize;

    /// Returns number of samples available for reading
    pub fn available(&self) -> usize;

    /// Returns free space available for writing
    pub fn free_space(&self) -> usize;
}
```

### VBAN Protocol Details

AudioMatrix uses **VBAN Audio** sub-protocol (not Text, Serial, or Service):

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         VBAN PACKET FORMAT                                       │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  Header (28 bytes):                                                             │
│  ┌──────────────────────────────────────────────────────────────────────────┐   │
│  │ Bytes │ Field       │ Description                                       │   │
│  │ 0-3   │ "VBAN"      │ Magic bytes (0x56 0x42 0x41 0x4E)                 │   │
│  │ 4     │ SR + SubProto│ Sample rate index (5 bits) + sub-protocol (3)    │   │
│  │ 5     │ Samples/frame│ Number of samples per channel in this packet     │   │
│  │ 6     │ Channels    │ Number of channels - 1 (0 = 1 channel)            │   │
│  │ 7     │ Format      │ Data format (PCM16, PCM24, Float32, etc.)         │   │
│  │ 8-23  │ Stream Name │ 16-char ASCII stream identifier                   │   │
│  │ 24-27 │ Frame Counter│ 32-bit sequence number (for reordering)          │   │
│  └──────────────────────────────────────────────────────────────────────────┘   │
│                                                                                  │
│  Payload (variable):                                                            │
│  - Interleaved samples: ch1[0], ch2[0], ch1[1], ch2[1], ...                    │
│  - Max payload: ~1400 bytes (fits in MTU without fragmentation)                │
│                                                                                  │
│  AudioMatrix uses:                                                              │
│  - Format: PCM24 (3 bytes/sample) or Float32 (4 bytes/sample)                  │
│  - Sample rate index: 11 (48000 Hz) typical                                     │
│  - Samples/frame: 64-256 (matches buffer size)                                  │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Sequence Number Handling:**
- Destination tracks expected sequence number
- Out-of-order packets within jitter buffer window: reorder
- Out-of-order packets beyond window: drop and log
- Missing packets: fill with silence or repeat last frame (configurable)

### VBAN Optimization

```rust
/// Optimized VBAN sender with batching and priority
pub struct VbanSender {
    socket: UdpSocket,                     // Non-blocking, high-priority
    packet_pool: PacketPool,               // Pre-allocated packet buffers
    sequence: AtomicU32,                   // Lock-free sequence counter
}

impl VbanSender {
    /// Send audio with minimal latency
    /// - Pre-allocated buffers (no allocation in audio path)
    /// - Vectored I/O where supported
    /// - Immediate send (no Nagle algorithm)
    pub fn send(&self, channels: &[&[f32]], stream_name: &str) -> Result<()>;
}
```

### Thread Model

```
┌─────────────────────────────────────────────────────────────┐
│ ASIO Callback Thread (per device)                           │
│   Priority: REALTIME                                        │
│   Affinity: Dedicated core (if available)                   │
│   Work: Copy samples to/from ring buffers                   │
│   Rules: NO allocation, NO locks, NO syscalls               │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ Matrix Router Thread                                         │
│   Priority: HIGH                                            │
│   Work: Route samples between ring buffers                  │
│   Rate: Runs every buffer period                            │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ Network I/O Thread (tokio runtime)                          │
│   Priority: HIGH                                            │
│   Work: VBAN send/receive, packet assembly                  │
│   Features: Zero-copy receive, batched send                 │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ Control Thread (tokio runtime)                              │
│   Priority: NORMAL                                          │
│   Work: API, WebSocket, mDNS, configuration                 │
└─────────────────────────────────────────────────────────────┘
```

**Thread Synchronization:**

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         THREAD SYNCHRONIZATION                                   │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  Audio Path (lock-free):                                                        │
│  ┌─────────────┐     Ring Buffer     ┌─────────────┐     Ring Buffer           │
│  │ ASIO Thread │ ──── (SPSC) ─────► │ Router      │ ──── (SPSC) ─────►        │
│  │ (producer)  │                     │ Thread      │                            │
│  └─────────────┘                     └─────────────┘                            │
│        │                                    │                                   │
│        │                                    │                                   │
│        └──── Atomic flags ─────────────────┘                                   │
│              (gain, mute, enabled)                                              │
│                                                                                  │
│  Control Path (mutex-protected):                                                │
│  ┌─────────────┐     RwLock          ┌─────────────┐                           │
│  │ Control     │ ──── (state) ─────► │ Router      │                           │
│  │ Thread      │                     │ Thread      │                           │
│  └─────────────┘                     └─────────────┘                            │
│        │                                                                        │
│        │ RwLock protects:                                                       │
│        │ - Routing table (add/remove connections)                              │
│        │ - Device list (add/remove devices)                                    │
│        │                                                                        │
│        │ Atomic values (no lock needed):                                        │
│        │ - Gain, mute, enabled per connection                                  │
│        │ - Level meters (updated by router, read by API)                       │
│                                                                                  │
│  Key rule: Router thread holds RwLock briefly only when routing table changes  │
│  Audio callbacks NEVER block - they only use atomics and ring buffers          │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## State Management & Resilience

### Design Philosophy: Destination-Owned Subscriptions

Like Dante, AudioMatrix uses a **receiver-centric** model where the destination device "owns" its subscriptions. This provides maximum resilience with no single point of failure.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         STATE OWNERSHIP MODEL                                    │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  STUDIO-PC (Source)                      LIVE-PC (Destination)                  │
│  ┌─────────────────────┐                 ┌─────────────────────┐                │
│  │                     │                 │                     │                │
│  │  Derived State:     │   "Send to me"  │  Authoritative:     │                │
│  │  "LIVE-PC wants     │ ◄────────────── │  "I subscribe to    │                │
│  │   my channel 1"     │                 │   STUDIO-PC ch 1"   │                │
│  │                     │                 │                     │                │
│  │  Active Senders:    │   Audio Stream  │  Subscriptions:     │                │
│  │  └─► LIVE-PC:1      │ ───────────────►│  └─► STUDIO-PC:1    │                │
│  │                     │     (VBAN)      │       → VASIO-Main:1│                │
│  └─────────────────────┘                 └─────────────────────┘                │
│                                                                                  │
│  Source stores: "who requested from me" (ephemeral, rebuilt from requests)      │
│  Destination stores: "what I want to receive" (persistent, survives restart)    │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Why Destination-Owned?

| Aspect | Destination-Owned | Source-Owned | Centralized |
|--------|-------------------|--------------|-------------|
| **Resilience** | Excellent | Good | Poor |
| **Source failure** | Auto-reconnect | Lost subscriptions | Depends |
| **Dest failure** | Reload & reconnect | Source keeps sending (waste) | Depends |
| **Network split** | Each side works locally | Each side works locally | May lose all |
| **Consistency** | Natural (1 owner) | Conflicts possible | Single truth |
| **Scalability** | Excellent | Excellent | Bottleneck |

### State Storage Per Computer

Each AudioMatrix service maintains its own persistent state:

```
Windows: %PROGRAMDATA%\AudioMatrix\
Linux:   /var/lib/audiomatrix/

audiomatrix/
├── config.toml                 # Service configuration
│
├── devices/                    # Virtual device definitions
│   ├── VASIO-DAW.toml         # Name, channels, sample rate
│   └── VASIO-FX.toml
│
├── subscriptions.toml          # What local destinations receive (AUTHORITATIVE)
│
├── channel-names.toml          # Custom channel labels
│
└── cache/
    ├── computers.toml          # Discovered computers (faster startup)
    └── presets/                # Routing presets (optional, can be global)
        ├── studio-default.toml
        └── live-show.toml
```

### Configuration Files

**config.toml** - Service settings:
```toml
[service]
computer_name = "STUDIO-PC"     # mDNS instance name
api_port = 8400
auto_start = true

[audio]
default_sample_rate = 48000
default_buffer_size = 64        # samples (powers of 2: 32, 64, 128, 256, 512)
max_channels_per_device = 256

# Sample rate mismatch handling
sample_rate_mismatch = "resample"  # "resample" | "block" | "warn"
#   resample: Auto-resample mismatched sources (see resampler settings below)
#   block:    Prevent connection, show error in UI
#   warn:     Allow connection but show warning (may cause artifacts)

[audio.resampler]
# Resampling is performed at the DESTINATION (receiver side)
# This keeps sources simple and allows per-destination quality settings
algorithm = "sinc"             # "linear" | "sinc" | "sinc_fastest" | "sinc_best"
#   linear:        Lowest CPU, poor quality, +0.02ms latency
#   sinc_fastest:  Good balance, +0.3ms latency (default)
#   sinc:          High quality, +0.5ms latency
#   sinc_best:     Highest quality, +1.0ms latency (for offline/mastering)
chunk_size = 64                # Process in chunks matching buffer size

[network]
# VBAN port allocation for RECEIVING streams
# Destination (receiver) picks a free port from this range
# Port is communicated to source in SUBSCRIBE message
vban_base_port = 6980
max_vban_streams = 64           # Ports 6980-7043

# Port assignment flow:
# 1. Destination needs to receive from source
# 2. Destination finds next free port (e.g., 6980)
# 3. Destination sends SUBSCRIBE { dest_port: 6980, ... } to source
# 4. Source sends VBAN packets to destination:6980
# Note: Source doesn't need to allocate ports - it sends to destination's port

jitter_buffer_ms = 2.0          # Adaptive range: 1.0 - 5.0 (auto-tuned based on jitter)
packet_timeout_ms = 100         # Mark source offline after no packets

# Reconnection with exponential backoff
# Prevents connection storms when many devices come online simultaneously
reconnect_initial_ms = 1000     # First retry after 1 second
reconnect_max_ms = 30000        # Maximum backoff: 30 seconds
reconnect_multiplier = 2.0      # Double delay after each failure
reconnect_jitter = 0.2          # Add ±20% random jitter to prevent thundering herd
# Example sequence: 1s, 2s, 4s, 8s, 16s, 30s, 30s, ...
# With jitter: 0.8-1.2s, 1.6-2.4s, 3.2-4.8s, ...

[headroom]
# Default headroom mode for new destinations
default_mode = "clip"           # clip | auto_gain | limiter | manual

[cache]
# Device cache settings
device_cache_ttl_hours = 24     # 24 hours - after this, offline devices are removed from cache
                                # Increase to 168 (7 days) for rarely-powered equipment
sync_cache_on_change = true     # Write cache immediately when devices change
```

**devices/VASIO-DAW.toml** - Virtual device:
```toml
[device]
name = "VASIO-DAW"
input_channels = 32
output_channels = 32
sample_rate = 48000
buffer_size = 64

[channels.inputs]
1 = "DAW Out L"
2 = "DAW Out R"
# ... remaining get default names "3", "4", etc.

[channels.outputs]
1 = "DAW In L"
2 = "DAW In R"
```

**subscriptions.toml** - The critical persistence file:
```toml
# This file is the AUTHORITATIVE source for what this computer receives.
# On startup, all subscriptions are re-requested from sources.
# This example is from LIVE-PC's subscriptions.toml

[[subscription]]
id = "STUDIO-PC:Focusrite 18i20:1>LOCAL:VASIO-DAW:1"
destination = { device = "VASIO-DAW", channel = 1 }
source = { computer = "STUDIO-PC", device = "Focusrite 18i20", channel = 1 }
gain_db = 0.0
mute = false
enabled = true
version = 1

[[subscription]]
id = "STUDIO-PC:Focusrite 18i20:2>LOCAL:VASIO-DAW:2"
destination = { device = "VASIO-DAW", channel = 2 }
source = { computer = "STUDIO-PC", device = "Focusrite 18i20", channel = 2 }
gain_db = 0.0
mute = false
enabled = true
version = 1

[[subscription]]
id = "LIVE-PC:RME Fireface:1>LOCAL:VASIO-Main:1"
destination = { device = "VASIO-Main", channel = 1 }
source = { computer = "LIVE-PC", device = "RME Fireface", channel = 1 }
gain_db = 0.0
mute = false
enabled = true
version = 1
# source.computer == this computer (LIVE-PC), so this is a LOCAL route
# No network involved - direct memory routing
```

### Connection Lifecycle

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         CONNECTION STATE MACHINE                                 │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  User clicks         Destination         Source              Destination        │
│  crosspoint          stores sub          creates sender      confirms           │
│       │                  │                    │                   │             │
│       ▼                  ▼                    ▼                   ▼             │
│  ┌─────────┐       ┌───────────┐       ┌───────────┐       ┌───────────┐       │
│  │ INTENT  │──────►│ REQUESTED │──────►│  PENDING  │──────►│  ACTIVE   │       │
│  └─────────┘       └───────────┘       └───────────┘       └───────────┘       │
│       │                  │                    │                   │             │
│       │            Source offline?     Timeout/Error?      Disconnect?         │
│       │                  │                    │                   │             │
│       │                  ▼                    ▼                   ▼             │
│       │            ┌───────────┐       ┌───────────┐       ┌───────────┐       │
│       │            │  WAITING  │       │   ERROR   │       │  REMOVED  │       │
│       │            │ (for src) │       │           │       │           │       │
│       │            └─────┬─────┘       └───────────┘       └───────────┘       │
│       │                  │                                                      │
│       │            Source online                                                │
│       │                  │                                                      │
│       │                  └─────────────────────────────────────────────►        │
│       │                              (retry → PENDING → ACTIVE)                 │
│       │                                                                         │
└───────┴─────────────────────────────────────────────────────────────────────────┘
```

### Subscription Protocol

**Creating a subscription:**

```
1. Controller → Destination API: POST /api/connections
   { source: { computer: "STUDIO-PC", device: "Focusrite", channel: 1 },
     destination: { computer: "LIVE-PC", device: "VASIO-Main", channel: 1 } }

2. Destination (LIVE-PC):
   a. Validates: device exists, channel in range, no conflict
   b. Stores subscription to subscriptions.toml
   c. Sends subscription request to source

3. Destination → Source: SUBSCRIBE message (via direct TCP or mDNS-SD TXT)
   { stream_id: "uuid", channels: [1], dest_ip: "192.168.1.42", dest_port: 6980 }

4. Source (STUDIO-PC):
   a. Creates VBAN sender for requested channels
   b. Starts sending audio packets

5. Destination → Controller: WebSocket event
   { type: "connection.status", id: "...", status: "active", latency_ms: 2.1 }
```

**Source failure and recovery:**

```
1. Source goes offline (STUDIO-PC crashes)

2. Destination (LIVE-PC) detects:
   - No packets received for > timeout (e.g., 100ms)
   - mDNS: STUDIO-PC service disappeared

3. Destination marks connection as WAITING:
   - Subscription remains in subscriptions.toml (persistent!)
   - Audio output: silence or last frame (configurable)
   - UI shows: orange indicator "Source Offline"

4. Source comes back online:
   - STUDIO-PC service starts
   - mDNS announces STUDIO-PC
   - Destination discovers source

5. Destination re-requests subscription:
   - Sends SUBSCRIBE message again
   - Source recreates sender
   - Audio flows again

6. Total recovery time: ~1-2 seconds (mDNS discovery + reconnect)
```

### Local vs Remote Connections

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         CONNECTION ROUTING DECISION                              │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  Source: STUDIO-PC:Focusrite:1                                                  │
│  Destination: ???                                                               │
│                                                                                  │
│  ┌────────────────────────────────────────────────────────────────────────────┐ │
│  │ if destination.computer == source.computer:                                │ │
│  │     # LOCAL CONNECTION                                                     │ │
│  │     # Direct memory routing within same service                            │ │
│  │     # Latency: <0.5ms                                                      │ │
│  │     route = LocalRoute(src_buffer, dst_buffer)                             │ │
│  │                                                                            │ │
│  │ else:                                                                      │ │
│  │     # NETWORK CONNECTION                                                   │ │
│  │     # Create VBAN stream transparently                                     │ │
│  │     # Latency: 1-3ms                                                       │ │
│  │     source_service.create_vban_sender(channels, dest_ip, dest_port)        │ │
│  │     dest_service.create_vban_receiver(stream_id)                           │ │
│  │     route = NetworkRoute(vban_receiver_buffer, dst_buffer)                 │ │
│  └────────────────────────────────────────────────────────────────────────────┘ │
│                                                                                  │
│  User sees: Identical crosspoint (●) regardless of local/network               │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Failure Scenarios & Recovery

| Scenario | Detection | Behavior | Recovery |
|----------|-----------|----------|----------|
| **Source computer offline** | mDNS disappear + packet timeout | Subscriptions persist, connections marked WAITING | Auto-reconnect when source returns |
| **Destination computer offline** | Source: no ACKs (optional) | Source stops sending | Destination reloads subs on restart |
| **Network partition** | Packet loss, mDNS split | Local routes continue, cross-net fails | Auto-heals when network restores |
| **Single device failure** | ASIO error callback | Other devices unaffected | Reconnect or recreate device |
| **Controller UI crash** | N/A | **Audio continues flowing** | Just restart UI |
| **Service restart** | N/A | Reload config + subscriptions | Re-establish all connections |
| **Corrupted config** | Parse error | Use defaults, log warning | Manual fix or backup restore |

### Matrix Controller Role

The Matrix Controller is **NOT** authoritative. It is purely a view and command interface:

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         CONTROLLER ARCHITECTURE                                  │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│                    Matrix Controller (Web UI)                                    │
│                    ┌────────────────────────┐                                   │
│                    │                        │                                   │
│                    │  Queries all computers │──── GET /api/devices              │
│                    │  Aggregates state      │──── GET /api/matrix               │
│                    │  Sends commands        │──── POST /api/connections         │
│                    │  Receives updates      │──── WebSocket events              │
│                    │                        │                                   │
│                    │  Stores: Minimal       │                                   │
│                    │  (pending subs only)   │                                   │
│                    │                        │                                   │
│                    └───────────┬────────────┘                                   │
│                                │                                                │
│          ┌─────────────────────┼─────────────────────┐                         │
│          │                     │                     │                         │
│          ▼                     ▼                     ▼                         │
│    ┌──────────┐          ┌──────────┐          ┌──────────┐                    │
│    │STUDIO-PC │          │ LIVE-PC  │          │ FOH-PC   │                    │
│    │          │          │          │          │          │                    │
│    │ Stores:  │          │ Stores:  │          │ Stores:  │                    │
│    │ - devices│          │ - devices│          │ - devices│                    │
│    │ - subs   │          │ - subs   │          │ - subs   │                    │
│    │ - config │          │ - config │          │ - config │                    │
│    └──────────┘          └──────────┘          └──────────┘                    │
│                                                                                  │
│    Each computer is authoritative for its own subscriptions                     │
│    Controller can run on ANY computer or separate machine                       │
│    Multiple controllers can run simultaneously (read-only or with locking)      │
│                                                                                  │
│    Controller's minimal state (in browser localStorage or config file):         │
│    - Pending subscriptions for offline destinations (see Offline Configuration) │
│    - UI preferences (collapsed devices, filter settings)                        │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Preset System

Presets can be stored locally or shared:

**Local preset** (per-computer):
```toml
# ~/.audiomatrix/presets/studio-default.toml
[preset]
name = "Studio Default"
description = "Standard studio routing"
created = "2024-01-15T10:30:00Z"
# version field not needed - preset files are replaced, not merged

# Only contains subscriptions for THIS computer
# Note: destination.computer omitted (always LOCAL for local presets)
[[subscription]]
destination = { device = "VASIO-DAW", channel = 1 }
source = { computer = "STUDIO-PC", device = "Focusrite 18i20", channel = 1 }
gain_db = 0.0
mute = false
enabled = true

[[subscription]]
destination = { device = "VASIO-DAW", channel = 2 }
source = { computer = "STUDIO-PC", device = "Focusrite 18i20", channel = 2 }
gain_db = 0.0
mute = false
enabled = true
```

**Global preset** (network-wide, stored on controller or shared location):
```toml
# /shared/audiomatrix-presets/live-show.toml
[preset]
name = "Live Show"
description = "Full live routing for all computers"
created = "2024-01-15T10:30:00Z"

# Contains subscriptions for ALL computers (must include destination.computer)
# When loaded, controller sends relevant parts to each computer

[[subscription]]
destination = { computer = "LIVE-PC", device = "VASIO-Main", channel = 1 }
source = { computer = "STUDIO-PC", device = "Focusrite 18i20", channel = 1 }
gain_db = 0.0
mute = false
enabled = true

[[subscription]]
destination = { computer = "FOH-PC", device = "VASIO-FOH", channel = 1 }
source = { computer = "STUDIO-PC", device = "Focusrite 18i20", channel = 1 }
gain_db = -6.0   # FOH gets lower level
mute = false
enabled = true
```

**Global Preset Distribution Logic:**

When loading a global preset, the controller performs:

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         PRESET DISTRIBUTION ALGORITHM                            │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  1. PARSE PRESET                                                                │
│     - Load all subscriptions from preset file                                   │
│     - Group by destination.computer                                             │
│                                                                                  │
│  2. VALIDATE COMPUTERS                                                          │
│     For each destination computer in preset:                                    │
│     - If online: mark as "ready"                                                │
│     - If offline but in cache: mark as "pending"                               │
│     - If unknown: mark as "unknown" (warning to user)                          │
│                                                                                  │
│  3. CLEAR EXISTING (optional, based on load mode)                               │
│     - "replace": DELETE all existing subscriptions on each destination         │
│     - "merge": Keep existing, add new (may duplicate)                          │
│     - "update": Update matching IDs, add new, keep unmatched                   │
│                                                                                  │
│  4. DISTRIBUTE                                                                  │
│     For each "ready" computer:                                                  │
│       POST /api/connections/batch with that computer's subscriptions           │
│                                                                                  │
│     For each "pending" computer:                                                │
│       Store in controller-pending.toml                                          │
│       When computer comes online → push subscriptions                           │
│                                                                                  │
│  5. REPORT                                                                      │
│     Return summary: { applied: N, pending: M, unknown: K, errors: [...] }      │
│                                                                                  │
│  Note: Preset loading is NOT transactional across computers.                   │
│  Some computers may succeed while others fail.                                  │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Multi-Source Mixing (N:1 Routing)

Unlike simple 1:1 routing, AudioMatrix supports **mixing multiple sources to one destination** (like VB-Matrix):

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         MULTI-SOURCE MIXING                                      │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  Example: Headphone mix with multiple sources                                   │
│                                                                                  │
│  Sources:                          Destination:                                 │
│  ┌─────────────────────┐           ┌─────────────────────┐                     │
│  │ STUDIO-PC:DAW:1     │──[0dB]───►│                     │                     │
│  │ (Main Mix L)        │           │                     │                     │
│  └─────────────────────┘           │  LIVE-PC:VASIO-HP:1 │                     │
│  ┌─────────────────────┐           │  (Headphone L)      │                     │
│  │ STUDIO-PC:DAW:3     │──[-12dB]─►│                     │                     │
│  │ (Click Track)       │           │  Mixed output =     │                     │
│  └─────────────────────┘           │  Sum of all inputs  │                     │
│  ┌─────────────────────┐           │                     │                     │
│  │ FOH-PC:Talkback:1   │──[-6dB]──►│                     │                     │
│  │ (Engineer Talkback) │           │                     │                     │
│  └─────────────────────┘           └─────────────────────┘                     │
│                                                                                  │
│  Matrix View (multiple ● in same row):                                          │
│                                                                                  │
│             │ DAW:1 │ DAW:3 │ Talkback:1 │                                      │
│  ───────────┼───────┼───────┼────────────┤                                      │
│  VASIO-HP:1 │  ●    │  ●    │     ●      │  ← All three mixed to one output    │
│  VASIO-HP:2 │  ●    │  ●    │     ●      │                                      │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Subscription with Gain Control:**

```toml
# subscriptions.toml - multiple sources to one destination (N:1 mixing)
# This example shows three sources mixed to VASIO-HP channel 1

[[subscription]]
id = "STUDIO-PC:DAW:1>LOCAL:VASIO-HP:1"
destination = { device = "VASIO-HP", channel = 1 }
source = { computer = "STUDIO-PC", device = "DAW", channel = 1 }
gain_db = 0.0           # Unity gain (main mix)
mute = false
enabled = true
version = 1

[[subscription]]
id = "STUDIO-PC:DAW:3>LOCAL:VASIO-HP:1"
destination = { device = "VASIO-HP", channel = 1 }  # Same destination!
source = { computer = "STUDIO-PC", device = "DAW", channel = 3 }
gain_db = -12.0         # Click track quieter
mute = false
enabled = true
version = 1

[[subscription]]
id = "FOH-PC:Talkback:1>LOCAL:VASIO-HP:1"
destination = { device = "VASIO-HP", channel = 1 }  # Same destination!
source = { computer = "FOH-PC", device = "Talkback", channel = 1 }
gain_db = -6.0          # Talkback at -6dB
mute = false            # Can be muted without removing
enabled = true
version = 2
```

**Mixing Implementation:**

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Destination channel with N:1 mixing support
pub struct DestinationChannel {
    sources: Vec<SourceConnection>,
    output_buffer: AudioRingBuffer,
    headroom_mode: HeadroomMode,
    buffer_size: usize,          // Configured buffer size (64, 128, 256, 512)
}

pub enum HeadroomMode {
    Clip,       // Hard clip at ±1.0 (default, no latency)
    AutoGain,   // Reduce all sources proportionally when sum > 1.0
    Limiter,    // Soft-knee limiter (+0.5ms latency)
    Manual,     // No protection, clip indicator only
}

impl DestinationChannel {
    /// Mix all sources to output (lock-free, runs in audio thread)
    /// temp_buffer must be at least output.len() in size
    pub fn process(&self, output: &mut [f32], temp_buffer: &mut [f32]) {
        // Zero the output buffer
        output.fill(0.0);

        // Sum all sources with their gains
        // Each source handles its own enabled/mute checks
        for source in &self.sources {
            source.process(output, temp_buffer);
        }

        // Apply headroom management
        match self.headroom_mode {
            HeadroomMode::Clip => {
                for sample in output.iter_mut() {
                    *sample = sample.clamp(-1.0, 1.0);
                }
            }
            HeadroomMode::AutoGain => {
                let peak = output.iter().fold(0.0f32, |max, &s| max.max(s.abs()));
                if peak > 1.0 {
                    let reduction = 1.0 / peak;
                    for sample in output.iter_mut() {
                        *sample *= reduction;
                    }
                }
            }
            HeadroomMode::Limiter => {
                // Soft-knee limiter (simplified, real impl uses lookahead)
                for sample in output.iter_mut() {
                    let abs = sample.abs();
                    if abs > 0.8 {
                        let knee = 1.0 - (abs - 0.8) * 0.5;
                        *sample *= knee.max(0.5);
                    }
                }
            }
            HeadroomMode::Manual => {} // No processing
        }
    }
}

// Note: temp_buffer should be allocated per-thread at startup based on
// max buffer size (e.g., 512 samples). This avoids allocation in audio path.
```

**Matrix UI for N:1:**

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  Click on existing connection → shows gain control popup:                       │
│                                                                                  │
│  ┌─────────────────────────────────┐                                            │
│  │ STUDIO-PC:DAW:1 → VASIO-HP:1    │                                            │
│  │                                 │                                            │
│  │ Gain: [━━━━━━━●━━━] -3.2 dB    │                                            │
│  │                                 │                                            │
│  │ [Mute] [Solo] [Remove]         │                                            │
│  └─────────────────────────────────┘                                            │
│                                                                                  │
│  Right-click on cell → "Add source to mix" (doesn't replace, adds)             │
│  Shift+click → Quick add to existing mix                                        │
│                                                                                  │
│  Visual indicators:                                                             │
│  ● = Single source (1:1)                                                        │
│  ◉ = Part of mix (N:1) - thicker dot or different color                        │
│  ●₃ = Part of 3-source mix (optional: show count)                              │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Headroom Management:**

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         HEADROOM & CLIPPING PREVENTION                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  When mixing N sources, sum can exceed 0dBFS.                                   │
│                                                                                  │
│  Options (configurable per destination):                                        │
│                                                                                  │
│  1. CLIP (default)     - Hard clip at 0dBFS, fast, no latency                  │
│  2. AUTO-GAIN          - Reduce all sources proportionally when sum > 0dB      │
│  3. LIMITER            - Soft-knee limiter on output (+0.5ms latency)          │
│  4. MANUAL             - User manages gain structure, clip indicator only       │
│                                                                                  │
│  Visual feedback:                                                               │
│  - Meter shows output level                                                     │
│  - Clip indicator (red) when limiting/clipping                                  │
│  - "OVR" warning if consistent overload                                         │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Conflict Resolution

With N:1 mixing, conflict resolution is simpler:

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         CONFLICT RESOLUTION                                      │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  Adding a connection: Always succeeds (adds to mix, no replacement)             │
│                                                                                  │
│  Removing a connection: Just removes that source from the mix                   │
│                                                                                  │
│  True conflicts only occur with concurrent modifications to SAME subscription:  │
│                                                                                  │
│  Controller A                        Controller B                               │
│       │                                   │                                     │
│       │ Set gain Src1→Dest1 = -6dB        │ Set gain Src1→Dest1 = -12dB        │
│       │ (version: 5)                      │ (version: 5)                        │
│       │                                   │                                     │
│       └──────────────┬────────────────────┘                                     │
│                      │                                                          │
│                      ▼                                                          │
│               Destination PC                                                    │
│               ┌─────────────┐                                                   │
│               │ version: 5  │                                                   │
│               │             │                                                   │
│               │ First write │ → version becomes 6, gain = -6dB                 │
│               │ Second write│ → version mismatch! Return conflict              │
│               │             │                                                   │
│               └─────────────┘                                                   │
│                                                                                  │
│  Resolution: Last-write-wins with version numbers (optimistic locking)          │
│  Controller B can: retry with version 6, or notify user                        │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Offline Device Configuration

**Critical requirement:** Users must be able to configure routing even when devices/computers are offline.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         OFFLINE CONFIGURATION SCENARIOS                          │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  Scenario 1: Source computer offline during configuration                       │
│  ───────────────────────────────────────────────────────────────────────────    │
│                                                                                  │
│  STUDIO-PC: OFFLINE               LIVE-PC: ONLINE                               │
│  (powered off)                    (running AudioMatrix)                         │
│                                                                                  │
│  User wants: STUDIO-PC:Focusrite:1 → LIVE-PC:VASIO-Main:1                       │
│                                                                                  │
│  Solution:                                                                      │
│  1. Controller shows STUDIO-PC as offline (grayed, from cached device list)    │
│  2. User can still click the crosspoint                                        │
│  3. LIVE-PC stores subscription immediately (in subscriptions.toml)            │
│  4. Connection state: WAITING (orange indicator)                               │
│  5. When STUDIO-PC comes online → auto-activates                               │
│                                                                                  │
│  ───────────────────────────────────────────────────────────────────────────    │
│  Scenario 2: Destination computer offline during configuration                  │
│  ───────────────────────────────────────────────────────────────────────────    │
│                                                                                  │
│  STUDIO-PC: ONLINE                LIVE-PC: OFFLINE                              │
│  (running AudioMatrix)            (powered off)                                 │
│                                                                                  │
│  User wants: STUDIO-PC:Focusrite:1 → LIVE-PC:VASIO-Main:1                       │
│                                                                                  │
│  Problem: Can't write to LIVE-PC's subscriptions.toml (it's offline!)          │
│                                                                                  │
│  Solution: Controller stores PENDING SUBSCRIPTION locally                       │
│  1. Controller caches the intent: { pending_for: "LIVE-PC", subscription: {...}}│
│  2. When LIVE-PC comes online, controller pushes the subscription              │
│  3. LIVE-PC stores it, requests stream from STUDIO-PC                          │
│  4. Connection activates                                                        │
│                                                                                  │
│  Alternative: STUDIO-PC can store "I should send to LIVE-PC when it appears"   │
│  (source-side pending, less elegant but works without controller persistence)  │
│                                                                                  │
│  ───────────────────────────────────────────────────────────────────────────    │
│  Scenario 3: Both computers offline (preset creation)                           │
│  ───────────────────────────────────────────────────────────────────────────    │
│                                                                                  │
│  Creating a preset for a venue before any equipment is powered on               │
│                                                                                  │
│  Solution:                                                                      │
│  1. Presets are stored as files (not dependent on running services)            │
│  2. Preset includes full device specifications (channels, names, etc.)         │
│  3. When loaded, controller pushes to each computer as it comes online         │
│  4. Subscriptions activate progressively                                        │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Device Cache for Offline Display:**

Each computer maintains a cache of known devices (from previous discovery):

```toml
# cache/computers.toml
[[computer]]
id = "STUDIO-PC"
last_seen = "2024-01-15T10:30:00Z"
address = "192.168.1.10"

[[computer.device]]
name = "Focusrite 18i20"
type = "physical"
input_channels = 18
output_channels = 20

[[computer.device]]
name = "VASIO-DAW"
type = "virtual"
input_channels = 32
output_channels = 32

[[computer]]
id = "LIVE-PC"
last_seen = "2024-01-15T09:00:00Z"
# ... devices ...
```

**UI Indicators for Connection States:**

| State | Indicator | Meaning |
|-------|-----------|---------|
| **ACTIVE** | ● (green) | Audio flowing |
| **WAITING** | ◐ (orange) | Subscription stored, source offline |
| **PENDING** | ○ (yellow) | Destination offline, will push when online |
| **ERROR** | ✕ (red) | Failed (sample rate mismatch, etc.) |
| **OFFLINE** | ◌ (gray) | Device/computer not seen recently |

**Subscription with Offline Awareness:**

```toml
# subscriptions.toml with full state tracking

[[subscription]]
id = "STUDIO-PC:Focusrite 18i20:1>LOCAL:VASIO-Main:1"
destination = { device = "VASIO-Main", channel = 1 }
source = { computer = "STUDIO-PC", device = "Focusrite 18i20", channel = 1 }
gain_db = 0.0
mute = false
enabled = true
version = 5
state = "waiting"           # Runtime state, persisted for recovery (see note below)
last_active = "2024-01-15T10:30:00Z"
error_reason = ""           # Populated if state == "error"

[[subscription]]
id = "STUDIO-PC:Focusrite 18i20:2>LOCAL:VASIO-Main:2"
destination = { device = "VASIO-Main", channel = 2 }
source = { computer = "STUDIO-PC", device = "Focusrite 18i20", channel = 2 }
gain_db = -3.0
mute = false
enabled = true
version = 2
state = "active"
last_active = "2024-01-15T14:22:00Z"
error_reason = ""

# Note on state persistence:
# - "active" is runtime-only (not meaningful after restart)
# - "waiting" is persisted so service knows to retry on startup
# - "error" is persisted with error_reason for diagnostics
```

**Pending Subscriptions (Controller-Side):**

When destination is offline, controller stores pending operations:

```toml
# controller-pending.toml (stored in browser localStorage or controller config)

[[pending]]
created = "2024-01-15T10:30:00Z"
target_computer = "LIVE-PC"
operation = "subscribe"
subscription = {
  destination = { device = "VASIO-Main", channel = 1 },
  source = { computer = "STUDIO-PC", device = "Focusrite 18i20", channel = 1 }
}
retry_until = "2024-01-16T10:30:00Z"  # optional: give up after 24h
```

**Startup Sequence with Offline Awareness:**

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         SERVICE STARTUP SEQUENCE                                 │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  1. Load config.toml, devices/*.toml                                            │
│     └─► Start local devices (VASIOs, physical ASIO)                            │
│                                                                                  │
│  2. Load subscriptions.toml                                                     │
│     └─► Mark all as WAITING initially                                          │
│                                                                                  │
│  3. Start mDNS browser                                                          │
│     └─► Begin discovering other computers                                       │
│                                                                                  │
│  4. Announce self via mDNS                                                      │
│     └─► Other computers can now discover us                                     │
│                                                                                  │
│  5. Start API server (REST + WebSocket)                                         │
│     └─► Controllers can connect                                                 │
│                                                                                  │
│  6. For each discovered computer:                                               │
│     a. Check if we have subscriptions from that source                         │
│     b. Send SUBSCRIBE requests                                                  │
│     c. Receive streams, mark as ACTIVE                                          │
│                                                                                  │
│  7. For each LOCAL subscription (same computer):                                │
│     a. Create direct memory route immediately                                   │
│     b. Mark as ACTIVE                                                           │
│                                                                                  │
│  8. Ongoing: React to mDNS events                                               │
│     - Computer appears → send pending subscription requests                     │
│     - Computer disappears → mark connections as WAITING                         │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### State Synchronization Protocol

When computers discover each other, they sync state:

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         SYNC PROTOCOL                                            │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  1. DISCOVERY (mDNS)                                                            │
│     STUDIO-PC announces: _audiomatrix._tcp.local                                │
│     TXT records: version=1.0, channels=64, computer=STUDIO-PC                   │
│                                                                                  │
│  2. HANDSHAKE (TCP - via REST API)                                              │
│     LIVE-PC connects to STUDIO-PC:8400                                          │
│     GET /api/devices → { devices: [...], sampleRates: [...] }                   │
│                                                                                  │
│  3. SUBSCRIPTION REQUEST (via REST API)                                         │
│     LIVE-PC needs audio from STUDIO-PC                                          │
│     POST http://STUDIO-PC:8400/api/streams/subscribe                            │
│     (see SUBSCRIBE message format below)                                        │
│                                                                                  │
│  4. AUDIO STREAMING (UDP - VBAN)                                                │
│     STUDIO-PC sends VBAN packets to LIVE-PC's specified port                    │
│                                                                                  │
│  5. ONGOING MAINTENANCE                                                         │
│     - Heartbeat every 1s (see Heartbeat Protocol)                               │
│     - State changes broadcast via WebSocket to connected controllers            │
│     - mDNS TTL refresh every 120s                                               │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### SUBSCRIBE Message Format

Destination sends to source's REST API to request audio stream:

```
POST /api/streams/subscribe
Content-Type: application/json

{
  "stream_id": "550e8400-e29b-41d4-a716-446655440000",  // UUID for this stream
  "source_device": "Focusrite 18i20",
  "source_channels": [1, 2],                            // 1-based channel numbers
  "dest_computer": "LIVE-PC",
  "dest_ip": "192.168.1.42",
  "dest_port": 6980,                                    // UDP port for VBAN
  "format": "float32",                                  // pcm16 | pcm24 | float32
  "sample_rate": 48000
}

Response (201 Created):
{
  "stream_id": "550e8400-e29b-41d4-a716-446655440000",
  "status": "active",
  "vban_stream_name": "AM_550e8400"                     // 16-char stream identifier
}
```

**Unsubscribe:**
```
DELETE /api/streams/subscribe/{stream_id}

Response (204 No Content)
```

### Heartbeat Protocol

Heartbeat uses lightweight UDP packets (not VBAN audio):

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         HEARTBEAT PROTOCOL                                       │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  Packet format (16 bytes):                                                      │
│  ┌────────────────────────────────────────────────────────────────────────────┐ │
│  │ Bytes │ Field          │ Description                                      │ │
│  │ 0-3   │ "AMHB"         │ Magic bytes (AudioMatrix HeartBeat)              │ │
│  │ 4-7   │ Timestamp      │ Sender's Unix timestamp (seconds)                │ │
│  │ 8-11  │ Sequence       │ Monotonic counter                                │ │
│  │ 12-15 │ Flags          │ Reserved for future use                          │ │
│  └────────────────────────────────────────────────────────────────────────────┘ │
│                                                                                  │
│  Sent: Every 1 second from destination to source (for active subscriptions)    │
│  Port: Same as VBAN stream (dest_port)                                          │
│                                                                                  │
│  Failure detection:                                                             │
│  - Source: 3 missed heartbeats (3s) → stop sending, mark stream idle           │
│  - Destination: 3 missed VBAN packets (based on buffer period) → mark offline  │
│                                                                                  │
│  Heartbeat also serves as:                                                      │
│  - NAT keepalive for home router scenarios                                      │
│  - Latency measurement (round-trip time / 2)                                    │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## Security & Access Control

### Security Model

AudioMatrix is designed for **trusted LAN environments** (studios, venues, broadcast facilities). The security model prioritizes ease of use over strict isolation.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         SECURITY ARCHITECTURE                                    │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  Trust Boundary: LAN                                                            │
│  ┌────────────────────────────────────────────────────────────────────────────┐ │
│  │                                                                            │ │
│  │   STUDIO-PC ◄──────────► LIVE-PC ◄──────────► FOH-PC                      │ │
│  │       │                      │                    │                        │ │
│  │       │    Trusted Zone      │                    │                        │ │
│  │       └──────────────────────┴────────────────────┘                        │ │
│  │                              │                                             │ │
│  │                    ┌─────────▼─────────┐                                   │ │
│  │                    │ Matrix Controller │                                   │ │
│  │                    │   (Web Browser)   │                                   │ │
│  │                    └───────────────────┘                                   │ │
│  │                                                                            │ │
│  └────────────────────────────────────────────────────────────────────────────┘ │
│                                    │                                            │
│  ────────────────────────────────────────────────────────────────────────────── │
│                                    │ Firewall                                   │
│                                    ▼                                            │
│                              Internet (Untrusted)                               │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Authentication Modes

| Mode | Description | Use Case |
|------|-------------|----------|
| **None** (default) | No authentication, all LAN users can control | Home studio, single-user |
| **PIN** | 4-6 digit PIN required for control operations | Small team, shared space |
| **Token** | API token required in header | Integration with other systems |
| **Full** | Username/password with role-based access | Large facilities, multi-team |

**config.toml security settings:**
```toml
[security]
mode = "pin"                    # none | pin | token | full
pin = "1234"                    # For pin mode (WARNING: stored in plaintext)
api_token = ""                  # For token mode (see token generation below)
read_only_without_auth = true   # Allow viewing matrix without auth

# Token generation (when api_token is empty and mode = "token"):
# 1. Service generates 32-byte random token on first startup
# 2. Token is base64-encoded and stored in this file
# 3. Token is also written to: %PROGRAMDATA%\AudioMatrix\api_token.txt
# 4. Admin retrieves token from that file to configure clients
# 5. Clients send token in header: Authorization: Bearer <token>
#
# To regenerate token: delete api_token value and restart service

[security.roles]                # For full mode
# role = ["permission1", "permission2"]
admin = ["*"]
operator = ["connect", "disconnect", "mute", "gain"]
viewer = ["read"]
```

**WebSocket Authentication:**
```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         WEBSOCKET AUTHENTICATION                                 │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  WebSocket connections authenticate via query parameter on initial connection:  │
│                                                                                  │
│  ws://STUDIO-PC:8400/ws?auth=<token>                                            │
│                                                                                  │
│  Where <token> is:                                                              │
│  - mode=none:  No token required (query param optional)                         │
│  - mode=pin:   Base64(pin), e.g., "MTIzNA==" for pin "1234"                    │
│  - mode=token: The full API token                                               │
│  - mode=full:  JWT from /api/auth/login endpoint                               │
│                                                                                  │
│  Invalid/missing auth → connection rejected with 401 status                     │
│                                                                                  │
│  Note: WebSocket auth is validated ONCE at connection time.                     │
│  If token is revoked, existing connections remain active until closed.          │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Permission Enforcement:**
```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         PERMISSION ENFORCEMENT                                   │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  Permissions are checked at the API layer for each operation:                   │
│                                                                                  │
│  Permission     │ Operations Protected                                          │
│  ───────────────┼────────────────────────────────────────────────────────────── │
│  read           │ GET /api/* (always allowed if read_only_without_auth=true)   │
│  connect        │ POST /api/connections                                         │
│  disconnect     │ DELETE /api/connections/*                                     │
│  mute           │ PATCH /api/connections/* (mute field only)                   │
│  gain           │ PATCH /api/connections/* (gain_db field only)                │
│  device.create  │ POST /api/devices/virtual                                     │
│  device.delete  │ DELETE /api/devices/*                                         │
│  device.modify  │ PATCH /api/devices/*                                          │
│  config         │ PATCH /api/config                                             │
│  preset.save    │ POST /api/presets                                             │
│  preset.load    │ POST /api/presets/*/load                                      │
│  *              │ All permissions (admin only)                                  │
│                                                                                  │
│  On permission denied: HTTP 403 Forbidden with error message                    │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**VBAN Security Considerations:**
```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         VBAN SECURITY WARNING                                    │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  ⚠️  VBAN protocol has NO built-in authentication or encryption.                │
│                                                                                  │
│  Risks:                                                                         │
│  - Any device on the LAN can send VBAN packets (audio injection)               │
│  - Any device on the LAN can receive VBAN packets (eavesdropping)              │
│  - No integrity protection (packets could be modified in transit)               │
│                                                                                  │
│  Mitigations:                                                                   │
│  - Use dedicated VLAN for audio network (isolates from general traffic)        │
│  - Firewall rules to limit which IPs can send/receive VBAN                     │
│  - Physical network security (no untrusted devices on audio VLAN)              │
│                                                                                  │
│  Future consideration:                                                          │
│  - DTLS encryption for VBAN (adds ~0.5ms latency)                              │
│  - Would require custom VBAN extension (not standard VBAN)                     │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Network Security Recommendations

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         NETWORK SECURITY CHECKLIST                               │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  ☐ Dedicated VLAN for audio network (recommended)                               │
│  ☐ Firewall blocking external access to ports 8400 (API), 6980-7043 (VBAN)     │
│  ☐ mDNS traffic contained within LAN                                            │
│  ☐ No internet-facing AudioMatrix services                                       │
│                                                                                  │
│  Ports used by AudioMatrix:                                                     │
│  ┌───────────────────────────────────────────────────────────────────────────┐  │
│  │ Port       │ Protocol │ Purpose                                          │  │
│  │ 8400       │ TCP      │ REST API + WebSocket                             │  │
│  │ 6980-7043  │ UDP      │ VBAN audio streams (configurable range)          │  │
│  │ 5353       │ UDP      │ mDNS (standard, shared with other services)      │  │
│  └───────────────────────────────────────────────────────────────────────────┘  │
│                                                                                  │
│  Future consideration: TLS for API, DTLS for VBAN (adds ~0.5ms latency)        │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Error Handling & Recovery

| Error Type | Detection | Automatic Recovery | User Action |
|------------|-----------|-------------------|-------------|
| **Disk full** | Write fails | Use in-memory state, retry periodically | Clear disk space |
| **Config parse error** | Startup check | Use defaults, log warning | Fix config file |
| **ASIO driver crash** | Exception callback | Attempt reconnect 3x | Restart device/service |
| **Network buffer overflow** | Packet loss metrics | Increase jitter buffer | Check network |
| **VBAN port conflict** | Bind fails | Try next port in range | Check other software |
| **Permission denied** | OS error | N/A | Run as admin (Windows) |

**Error persistence:**
```toml
# errors.log - rotated, max 10MB
2024-01-15T10:30:00Z ERROR disk_full: Failed to write subscriptions.toml
2024-01-15T10:30:01Z WARN  using_memory_state: Changes will be lost on restart
2024-01-15T10:35:00Z INFO  disk_available: Persisted 5 pending changes
```

## Cross-Platform Support

### Platform Status

| Platform | Audio Backend | Service Type | Status |
|----------|--------------|--------------|--------|
| **Windows** | ASIO (via cpal) | Windows Service | Primary target |
| **Linux** | ALSA, JACK, PipeWire | systemd service | Planned |
| **macOS** | CoreAudio | launchd daemon | Future |

### Linux Implementation Notes

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         LINUX AUDIO BACKENDS                                     │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  Priority order (auto-detected):                                                │
│                                                                                  │
│  1. JACK       - Preferred for pro audio (lowest latency, ~1ms possible)       │
│                  Requires JACK server running                                   │
│                                                                                  │
│  2. PipeWire   - Modern, good latency (~3-5ms), seamless desktop integration   │
│                  Auto-detected on modern distros (Fedora 34+, Ubuntu 22.04+)   │
│                                                                                  │
│  3. ALSA       - Direct hardware access, requires root or audio group          │
│                  Latency depends on hardware (~2-10ms)                          │
│                                                                                  │
│  Virtual devices on Linux:                                                      │
│  - JACK: Create virtual ports (native support)                                 │
│  - PipeWire: Create virtual nodes via pw-cli                                   │
│  - ALSA: Use snd-aloop kernel module                                           │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Linux config.toml additions:**
```toml
[audio.linux]
backend = "auto"              # auto | jack | pipewire | alsa
jack_client_name = "AudioMatrix"
jack_autoconnect = false      # Auto-connect to system ports
alsa_device = "default"       # ALSA device name
realtime_priority = 90        # RT priority (requires rtprio permissions)
```

## Project Structure

```
audiomatrix/
├── Cargo.toml                    # Workspace root
├── ARCHITECTURE.md               # This file
│
├── crates/
│   ├── ram-core/                 # Core audio engine
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── routing.rs        # Global routing matrix
│   │       ├── connection.rs     # Connection state machine
│   │       ├── buffer.rs         # Lock-free ring buffers
│   │       ├── resampler.rs      # Sample rate conversion
│   │       └── mixer.rs          # Channel mixing/summing
│   │
│   ├── ram-asio/                 # ASIO device management
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── physical.rs       # Physical ASIO enumeration
│   │   │   ├── virtual.rs        # Virtual ASIO lifecycle
│   │   │   ├── registry.rs       # Windows registry operations
│   │   │   └── callback.rs       # ASIO callback handlers
│   │   └── cpp/                  # COM DLL for virtual ASIO
│   │       ├── virtual_asio.cpp
│   │       ├── virtual_asio.h
│   │       └── CMakeLists.txt
│   │
│   ├── ram-net/                  # Network transport (internal)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── vban/
│   │       │   ├── mod.rs
│   │       │   ├── protocol.rs   # VBAN packet format
│   │       │   ├── sender.rs     # Optimized UDP sender
│   │       │   ├── receiver.rs   # Jitter-buffered receiver
│   │       │   └── stream.rs     # Stream lifecycle
│   │       └── transport.rs      # Abstract transport trait
│   │
│   ├── ram-discovery/            # Service discovery
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── announce.rs       # mDNS service announcement
│   │       ├── browse.rs         # mDNS browsing
│   │       └── sync.rs           # State synchronization
│   │
│   ├── ram-api/                  # Control plane API
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── rest/
│   │       │   ├── mod.rs
│   │       │   ├── devices.rs    # Device CRUD
│   │       │   ├── connections.rs # Connection CRUD
│   │       │   └── config.rs     # System configuration
│   │       ├── websocket.rs      # Real-time updates
│   │       └── models.rs         # API data structures
│   │
│   └── ram-service/              # Service executable
│       └── src/
│           ├── main.rs           # Entry point
│           ├── service.rs        # Windows service integration
│           └── coordinator.rs    # Component orchestration
│
├── controller/                   # Web-based Matrix Controller
│   ├── package.json
│   ├── vite.config.ts
│   └── src/
│       ├── App.tsx
│       ├── components/
│       │   ├── Matrix/
│       │   │   ├── Matrix.tsx          # Main matrix grid
│       │   │   ├── MatrixCell.tsx      # Individual crosspoint
│       │   │   ├── DeviceHeader.tsx    # Expandable device row/col
│       │   │   └── ChannelLabel.tsx    # Channel name display
│       │   ├── Sidebar/
│       │   │   ├── DeviceList.tsx      # All devices tree view
│       │   │   ├── DeviceConfig.tsx    # Device properties panel
│       │   │   └── VirtualCreate.tsx   # Create virtual device
│       │   ├── Toolbar/
│       │   │   ├── FilterBar.tsx       # Search and filter
│       │   │   └── ViewControls.tsx    # Expand/collapse all
│       │   └── Meters/
│       │       ├── LevelMeter.tsx      # Per-channel meter
│       │       └── MeterBridge.tsx     # Multi-channel strip
│       ├── hooks/
│       │   ├── useMatrix.ts            # Matrix state management
│       │   ├── useWebSocket.ts         # Real-time connection
│       │   └── useDevices.ts           # Device state
│       ├── api/
│       │   └── client.ts               # REST + WS client
│       └── types/
│           └── index.ts                # TypeScript definitions
│
└── docs/
    ├── user-guide.md
    ├── api-reference.md
    └── deployment.md
```

## API Design

### API Versioning

All API endpoints are prefixed with `/api/v1/`. Future breaking changes will use `/api/v2/`, etc.

- Current version: `v1`
- Version in URL: `/api/v1/devices`, `/api/v1/connections`, etc.
- Version header (optional): `Accept: application/vnd.audiomatrix.v1+json`
- Unversioned `/api/*` routes → redirect to current version

Backwards compatibility policy:
- Minor additions (new fields, new endpoints) do NOT bump version
- Breaking changes (removed fields, changed semantics) bump version
- Old versions supported for minimum 1 year after deprecation

### REST Endpoints

```
# All endpoints below are under /api/v1/ (version prefix omitted for brevity)

# Devices
GET    /api/devices                         # List all devices (local + discovered)
POST   /api/devices/virtual                 # Create virtual ASIO device
GET    /api/devices/{id}                    # Get device details
PATCH  /api/devices/{id}                    # Update device (rename, resize channels)
DELETE /api/devices/{id}                    # Delete virtual device (only virtual allowed)

# Connections (Matrix routing)
GET    /api/connections                     # List all connections
POST   /api/connections                     # Create connection
GET    /api/connections/{id}                # Get connection details
PATCH  /api/connections/{id}                # Update connection (gain, mute, enabled)
DELETE /api/connections/{id}                # Remove connection
POST   /api/connections/batch               # Batch create/update/delete (see format below)

# Matrix view (convenience endpoint)
GET    /api/matrix                          # Full matrix state (devices + connections)

# Computers
GET    /api/computers                       # List discovered computers
GET    /api/computers/{id}                  # Get computer details
GET    /api/computers/{id}/devices          # Devices on specific computer

# Monitoring
GET    /api/meters                          # Current level meters (all channels)
GET    /api/meters/{device_id}              # Meters for specific device
GET    /api/diagnostics                     # Latency, jitter, packet loss, errors

# Configuration
GET    /api/config                          # System configuration
PATCH  /api/config                          # Update configuration

# Presets
GET    /api/presets                         # List presets
POST   /api/presets                         # Save new preset
GET    /api/presets/{id}                    # Get preset details
DELETE /api/presets/{id}                    # Delete preset
POST   /api/presets/{id}/load               # Load/apply preset
```

**Connection ID format:** `{src_computer}:{src_device}:{src_ch}>{dst_computer}:{dst_device}:{dst_ch}`
Example: `STUDIO-PC:Focusrite 18i20:1>LIVE-PC:VASIO-DAW:1`

**URL Encoding:** Connection IDs contain special characters (`:` and `>`).
When used in URLs, they must be percent-encoded:
- `:` → `%3A`
- `>` → `%3E`
- Space → `%20`
Example: `GET /api/connections/STUDIO-PC%3AFocusrite%2018i20%3A1%3ELIVE-PC%3AVASIO-DAW%3A1`

**Note:** In `subscriptions.toml`, destination computer is always `LOCAL` (implicit this computer).
In the API and controller, full computer names are used for cross-computer visibility.

### WebSocket Events

```typescript
// Client → Server
// Subscribe to event topics (not audio channels!)
{ type: "subscribe", topics: ["meters", "connections", "devices", "computers"] }
{ type: "unsubscribe", topics: ["meters"] }

// Commands via WebSocket (alternative to REST)
{ type: "connection.create", source: "STUDIO-PC:VASIO-DAW:1", destination: "VASIO-Main:1" }
{ type: "connection.update", id: "...", gain_db: -3.0, mute: false }
{ type: "connection.delete", id: "..." }

// Server → Client
{ type: "connection.created", connection: {...} }
{ type: "connection.updated", connection: {...} }
{ type: "connection.deleted", id: "..." }
{ type: "connection.status", id: "...", status: "active" | "waiting" | "error" }

{ type: "device.added", device: {...} }
{ type: "device.removed", deviceId: "..." }
{ type: "device.updated", device: {...} }

{ type: "computer.online", computer: {...} }
{ type: "computer.offline", computerId: "..." }

{ type: "meters", levels: { "STUDIO-PC:VASIO-DAW:1": -12.5, ... } }  // ~10Hz
{ type: "meters.clip", channel: "STUDIO-PC:VASIO-DAW:1" }  // Clipping detected

{ type: "error", code: "...", message: "..." }
```

### Data Models

```typescript
interface Computer {
  id: string;                    // "STUDIO-PC"
  name: string;                  // Display name (same as id usually)
  address: string;               // IP address
  online: boolean;
  lastSeen?: string;             // ISO timestamp
  latency_ms?: number;           // Network latency to this computer
  deviceCount: number;           // Number of devices on this computer
}

interface Device {
  id: string;                    // "STUDIO-PC:VASIO-DAW"
  computerId: string;            // "STUDIO-PC"
  name: string;                  // "VASIO-DAW"
  type: "physical" | "virtual";
  inputChannels: Channel[];
  outputChannels: Channel[];
  sampleRate: number;
  bufferSize: number;
  online: boolean;
  headroomMode?: "clip" | "auto_gain" | "limiter" | "manual";  // For destinations
}

interface Channel {
  index: number;                 // 1-based (matches UI and TOML)
  name: string;                  // "Mic 1" or "1" (default is index as string)
  level_dbfs?: number;           // Current level, updated via WebSocket (-60 to 0)
  clipping?: boolean;            // True if clipping detected
}

// Consistent with earlier ChannelRef definition
interface ChannelRef {
  computer: string;              // "STUDIO-PC"
  device: string;                // "Focusrite 18i20"
  channel: number;               // 1-based
}

interface Connection {
  id: string;                    // "STUDIO-PC:Focusrite:1>LIVE-PC:VASIO-DAW:1"
  source: ChannelRef;
  destination: ChannelRef;       // Full ChannelRef; in subscriptions.toml computer is implicit

  // Controls
  gain_db: number;               // -60 to +12 (≤-60 = silence)
  mute: boolean;
  enabled: boolean;
  version: number;               // For optimistic locking

  // Status
  status: "active" | "waiting" | "pending" | "error";
  latency_ms?: number;           // Measured end-to-end latency
  error_reason?: string;         // If status == "error"
}

// Batch operation for efficiency
interface BatchOperation {
  create?: Array<{ source: ChannelRef, destination: ChannelRef, gain_db?: number }>;
  update?: Array<{ id: string, gain_db?: number, mute?: boolean, enabled?: boolean }>;
  delete?: string[];             // Array of connection IDs
}

// Batch operation response
interface BatchResult {
  created: Array<{ id: string, status: "success" | "error", error?: string }>;
  updated: Array<{ id: string, status: "success" | "error", error?: string }>;
  deleted: Array<{ id: string, status: "success" | "error", error?: string }>;
  summary: {
    total: number;
    succeeded: number;
    failed: number;
  };
}
// Note: Batch operations are NOT transactional - partial success is possible
// Client should check individual results and retry failed operations
```

## Comparison to Alternatives

| Feature | Dante | VB-Matrix | AudioMatrix |
|---------|-------|-----------|-------------|
| **Transport** | Proprietary | VBAN (visible) | VBAN (hidden) |
| **Discovery** | mDNS | Manual | mDNS |
| **Interface** | Desktop app | Desktop app | Web-based |
| **Matrix View** | Unified | Separate streams | Unified |
| **Device Management** | Limited | Manual VBAN | Dynamic VASIO |
| **Local Latency** | <1ms | 2-5ms | ~2.7ms (64 samples @ 48kHz) |
| **Network Latency** | <1ms | 2-5ms | ~4ms (LAN) |
| **Channel Resize** | No | Recreate stream | Live resize |
| **Filtering** | Yes | No | Yes |
| **Kernel Driver** | Yes | Yes | No |
| **License** | Per-channel $ | Free | Open source |

## Implementation Phases

### Phase 1: Core Foundation
- [ ] Workspace setup
- [ ] Lock-free ring buffer with benchmarks
- [ ] Basic routing matrix (local only)
- [ ] VBAN sender/receiver (standalone test)

### Phase 2: ASIO Integration
- [ ] Physical ASIO enumeration via cpal
- [ ] Virtual ASIO COM DLL (C++ interop)
- [ ] ASIO ↔ ring buffer integration
- [ ] Local routing: VASIO → Physical

### Phase 3: Network Transparency
- [ ] mDNS discovery (announce + browse)
- [ ] Cross-computer routing via VBAN
- [ ] Unified device model (local + remote)
- [ ] Automatic stream lifecycle management

### Phase 4: Service & API
- [ ] Windows service wrapper
- [ ] REST API (devices, matrix, config)
- [ ] WebSocket (real-time updates)
- [ ] State persistence (routes, devices)

### Phase 5: Matrix Controller
- [ ] React app scaffold
- [ ] Matrix grid component (expandable rows/cols)
- [ ] Device sidebar with configuration
- [ ] Filter bar and view controls
- [ ] Level meters (WebSocket-driven)

### Phase 6: Polish & Performance
- [ ] Adaptive jitter buffer tuning
- [ ] Latency measurement and display
- [ ] Preset save/load
- [ ] Error handling and recovery
- [ ] Performance profiling and optimization

## Development Practices

> **Full development guidelines**: See `CLAUDE.md` for comprehensive standards.

### Core Requirements

| Aspect | Requirement |
|--------|-------------|
| **Language** | Rust 2021 edition, MSRV 1.75+ |
| **Test Coverage** | Minimum 80%, enforced by CI |
| **File Size** | Maximum 1000 lines per file |
| **Code Style** | `cargo fmt` + `clippy::pedantic` |
| **Branch Protection** | PRs required, 1+ review, all checks pass |

### Test-Driven Development (TDD)

```
1. Write failing test
2. Implement minimal code to pass
3. Refactor while keeping tests green
4. Add benchmarks for performance-critical code
```

### Audio Code Requirements

All audio path code MUST be:
- **Lock-free**: No mutex/rwlock in callbacks
- **Allocation-free**: No heap allocations in hot paths
- **Deterministic**: Bounded execution time
- **Verified**: Run under Miri for undefined behavior detection

### CI Pipeline (GitHub Actions)

```
PR Check Pipeline:
├── fmt          → Format check
├── clippy       → Lint check (pedantic, deny warnings)
├── test         → All platforms (Linux, Windows, macOS)
├── coverage     → 80% minimum, fail below
├── audit        → Security vulnerability scan
├── benchmark    → Regression detection (fail on >10% regression)
└── miri         → Undefined behavior check (ram-core)
```

### Git Workflow

```
main (protected)
└── develop (integration)
    ├── feature/xxx
    ├── fix/xxx
    └── refactor/xxx
```

Commit format: `<type>(<scope>): <subject>`
Types: `feat`, `fix`, `refactor`, `test`, `docs`, `ci`, `perf`

## License

MIT OR Apache-2.0
