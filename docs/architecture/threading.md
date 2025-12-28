# AudioMatrix - Threading Architecture

## Design Principles

1. **Zero-Copy Where Possible**: Audio data stays in place, pointers move
2. **Lock-Free Audio Path**: No mutexes in real-time callbacks
3. **Minimal Buffering**: 64-256 samples typical
4. **Real-Time Priority**: ASIO callbacks at highest OS priority
5. **Batch Operations**: Group small packets, reduce syscalls

## Thread Hierarchy

```
┌─────────────────────────────────────────────────────────┐
│ ASIO Callback Thread (per device)                       │
│   Priority: REALTIME                                    │
│   Work: Copy samples to/from ring buffers               │
│   Rules: NO allocation, NO locks, NO syscalls           │
└─────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────┐
│ Matrix Router Thread                                     │
│   Priority: HIGH                                        │
│   Work: Route samples between ring buffers              │
│   Rate: Runs every buffer period                        │
└─────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────┐
│ Network I/O Thread (tokio)                              │
│   Priority: HIGH                                        │
│   Work: VBAN send/receive, packet assembly              │
└─────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────┐
│ Control Thread (tokio)                                  │
│   Priority: NORMAL                                      │
│   Work: API, WebSocket, mDNS, configuration             │
└─────────────────────────────────────────────────────────┘
```

## Latency Budget

### Local Route (DAW to Physical Output)

| Stage | Samples | Latency @ 48kHz |
|-------|---------|-----------------|
| VASIO Buffer | 64 | 1.33ms |
| Matrix Copy | ~0 | <0.01ms |
| Physical Buffer | 64 | 1.33ms |
| **TOTAL** | | **~2.7ms** |

### Network Route (Computer A to B)

| Stage | Samples | Latency |
|-------|---------|---------|
| Source VASIO | 64 | 1.33ms |
| VBAN Packetize | - | <0.1ms |
| Network Transit | - | 0.1-0.5ms |
| Jitter Buffer | 48 | 1.0ms |
| Matrix Copy | ~0 | <0.01ms |
| Physical Buffer | 64 | 1.33ms |
| **TOTAL** | | **~4ms** |

## Lock-Free Primitives

### Ring Buffer (SPSC)

- Single-producer, single-consumer
- Cache-line padded positions (no false sharing)
- Power-of-2 capacity for fast modulo
- Used between ASIO callbacks and router

### Atomic Parameters

- AtomicF32 for gain (bit-cast from AtomicU32)
- AtomicBool for mute, enabled
- Relaxed ordering (no synchronization needed)
- Updated from control thread, read from audio thread

## Synchronization Rules

| Path | Mechanism | Notes |
|------|-----------|-------|
| Audio → Router | Ring buffers | Lock-free SPSC |
| Router → Audio | Ring buffers | Lock-free SPSC |
| Control → Audio | Atomics | gain, mute, enabled |
| Control → Router | RwLock | Route table changes only |

**Key rule**: Router thread holds RwLock briefly only when routing table changes. Audio callbacks NEVER block.

## Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| Ring buffer | Complete | With CachePadded atomics |
| AtomicF32 | Complete | Bit-cast implementation |
| Packet pool | Complete | Zero-alloc transmission |
| Jitter buffer | Complete | Adaptive sizing |
| Shutdown signals | Complete | Broadcast channels |
| ASIO callbacks | Not Started | Feature gate not enabled |
| Router loop | Not Started | Infrastructure ready |
