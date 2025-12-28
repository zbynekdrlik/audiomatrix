# AudioMatrix - State Management

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

## File Storage

```
Windows: %PROGRAMDATA%\AudioMatrix\
Linux:   /var/lib/audiomatrix/

audiomatrix/
├── config.toml              # Service configuration
├── devices/                 # Virtual device definitions
│   ├── VASIO-DAW.toml
│   └── VASIO-FX.toml
├── subscriptions.toml       # What local destinations receive
├── channel-names.toml       # Custom channel labels
└── cache/
    ├── computers.toml       # Discovered computers
    └── presets/             # Routing presets
```

## Configuration Schema

### config.toml

| Section | Key | Default | Description |
|---------|-----|---------|-------------|
| service | computer_name | hostname | mDNS instance name |
| service | api_port | 8400 | REST/WebSocket port |
| audio | default_sample_rate | 48000 | Hz |
| audio | default_buffer_size | 64 | Samples |
| audio | sample_rate_mismatch | "resample" | resample/block/warn |

### subscriptions.toml

Stores active routes where this computer is the destination:
- Connection ID
- Source reference (computer:device:channel)
- Destination reference (device:channel, computer implicit)
- Control values (gain_db, mute, enabled)
- Version number for conflict resolution

### devices/*.toml

Virtual device definitions:
- Name (displayed in DAW)
- Input/output channel count
- Sample rate
- Buffer size

## Connection State Machine

```
Disconnected ←→ Connecting ←→ Connected
     ↑               ↓              ↓
     └───── Reconnecting ←─────────┘
                ↓
            Backoff
                ↓
            Failed (permanent)
```

Transitions:
- Connected → Reconnecting: Timeout/packet loss
- Reconnecting → Backoff: Retry failed
- Backoff → Connecting: Backoff timer expired
- Any → Failed: Permanent error (sample rate mismatch)

## Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| Config persistence | Complete | TOML-based |
| Route persistence | Complete | subscriptions.toml |
| Device persistence | Complete | devices/*.toml |
| Connection state machine | Complete | 7-state implementation |
| Channel names | Complete | Persistence ready |
| Presets | Not Started | Structure defined |
