# AudioMatrix - Routing Architecture

## Matrix Controller (Dante-Style)

The matrix presents all devices (local and remote) in a unified grid:
- **Rows**: Destinations (receivers)
- **Columns**: Sources (transmitters)
- **Crosspoints**: Click to connect/disconnect

### Hierarchy & Expansion

Each level is independently collapsible:
- Computer level (shows all devices on that machine)
- Device level (shows individual channels)
- Channel level (individual routing points)

### Filtering

| Filter Type | Example | Effect |
|-------------|---------|--------|
| Text search | `VASIO` | Shows only matching devices |
| Computer | `STUDIO-PC` | Shows only that computer |
| Regex | `^VASIO-.*$` | Pattern matching |
| Presets | "Studio Only" | Saved filter combinations |

## Connection Model

### Connection ID Format

```
{src_computer}:{src_device}:{src_ch}>{dst_computer}:{dst_device}:{dst_ch}
```

Example: `STUDIO-PC:Focusrite 18i20:1>LOCAL:VASIO-DAW:1`

### Per-Connection Controls

Every connection has individual controls:
- **Gain**: -60 to +12 dB (values <= -60dB treated as silence)
- **Mute**: Silences output but keeps connection active
- **Enabled**: Master switch (false = removed from audio path, saves CPU)
- **Solo**: UI-only, mutes other sources to same destination

### Connection States

| State | Meaning | Persisted? |
|-------|---------|------------|
| `active` | Audio flowing | No (runtime only) |
| `waiting` | Subscription stored, source offline | Yes |
| `pending` | Destination offline, queued | Yes |
| `error` | Failed (sample rate mismatch, etc.) | Yes |

### Destination N:1 Mixing

Multiple sources can connect to one destination:
- Samples are summed (mixed)
- Headroom modes prevent clipping: Clip, AutoGain, Limiter, Manual

## Route Types

| Route Type | Implementation | Typical Latency |
|------------|----------------|-----------------|
| **Local** | Direct memory copy | <0.5ms |
| **Cross-computer** | VBAN over UDP | 1-3ms |
| **Resampled** | Via resampler stage | +0.5ms |

## Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| RoutingMatrix structure | Complete | Full connection management |
| AudioEngine | Complete | Connection add/remove working |
| Per-connection atomics | Complete | Gain, mute, enabled - lock-free |
| Audio processing loop | Not Started | Ring buffers exist, not wired |
| Headroom/mixing | Partial | Enums defined, not applied |
| Resampling | Complete | Rubato integrated, not connected |
