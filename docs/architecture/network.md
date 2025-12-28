# AudioMatrix - Network Architecture

## Network Transparency

Users see **one unified matrix** regardless of connection type:
- Local routes: Direct memory copy
- Cross-computer routes: Automatic VBAN stream creation
- Technical details hidden in device properties dialogs

## VBAN Protocol

AudioMatrix uses **VBAN Audio** sub-protocol for cross-computer routing.

### Packet Structure

| Field | Size | Description |
|-------|------|-------------|
| Magic | 4 bytes | "VBAN" (0x56424E41) |
| SR + SubProto | 1 byte | Sample rate index + sub-protocol |
| Samples/frame | 1 byte | Samples per channel in packet |
| Channels | 1 byte | Number of channels - 1 |
| Format | 1 byte | PCM16, PCM24, Float32, etc. |
| Stream Name | 16 bytes | ASCII stream identifier |
| Frame Counter | 4 bytes | Sequence number |
| Payload | Variable | Interleaved audio samples |

### Stream Configuration

- Format: PCM24 or Float32
- Sample rate: 48000 Hz typical
- Samples/frame: 64-256 (matches buffer size)
- Max payload: ~1400 bytes (fits MTU)

### Sequence Handling

- Packets within jitter window: reorder
- Packets beyond window: drop and log
- Missing packets: silence or repeat (configurable)

## mDNS Discovery

Each AudioMatrix instance announces via mDNS:
- Service type: `_audiomatrix._tcp`
- Instance name: Computer name
- Port: API port (default 8400)
- TXT records: Version, capabilities

### Discovery Flow

1. Service starts → announces on network
2. Browsers discover other instances
3. Device lists merge into unified matrix
4. Routes auto-reconnect when sources reappear

## Subscription Protocol

**Destination-owned model** (like Dante):
- Destination stores what it wants to receive
- Source derives sender list from incoming requests
- No central controller required

### Subscription Flow

1. User clicks crosspoint on destination's matrix
2. Destination sends SUBSCRIBE to source
3. Source creates VBAN sender for that channel
4. Audio flows until UNSUBSCRIBE

## Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| VBAN protocol parsing | Complete | Full encode/decode |
| VBAN sender | Complete | With packet pooling |
| VBAN receiver | Complete | With jitter buffer |
| Stream lifecycle | Complete | Timeout detection |
| mDNS announce | Complete | Full TXT records |
| mDNS browse | Complete | Device discovery |
| Subscription protocol | Not Started | Messages not implemented |
| Auto stream creation | Not Started | Manual setup only |
