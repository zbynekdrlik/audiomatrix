# AudioMatrix - API Architecture

## REST API

Base URL: `http://{node}:{port}/api/v1`

Default port: 8080

---

## Device Management Endpoints

### List All Devices (Available + Attached)

```http
GET /api/v1/nodes/{node_id}/devices
```

Query parameters:
- `status` - Filter by status: `available`, `attached`, `active`, `all` (default: `all`)
- `type` - Filter by type: `input`, `output`, `duplex`, `all` (default: `all`)

Response:
```json
[
    {
        "id": "Focusrite USB ASIO",
        "system_name": "Focusrite USB ASIO",
        "display_name": "FOH Console",
        "device_type": "duplex",
        "input_channels": 18,
        "output_channels": 20,
        "sample_rate": 48000,
        "buffer_size": 64,
        "is_virtual": false,
        "status": "attached",
        "backend": "asio"
    }
]
```

### Get Device Details

```http
GET /api/v1/nodes/{node_id}/devices/{device_id}
```

Response: Same as single device in list.

### Attach Device

Attach an available device to AudioMatrix for routing.

```http
POST /api/v1/nodes/{node_id}/devices/{device_id}/attach
Content-Type: application/json

{
    "display_name": "FOH Console"  // Optional alias
}
```

Response:
```json
{
    "id": "Focusrite USB ASIO",
    "system_name": "Focusrite USB ASIO",
    "display_name": "FOH Console",
    "status": "attached",
    "input_channels": 18,
    "output_channels": 20
}
```

Errors:
- `404` - Device not found
- `409` - Device already attached
- `503` - Device busy (used by another application)

### Detach Device

Detach a device from AudioMatrix, stopping all streams.

```http
POST /api/v1/nodes/{node_id}/devices/{device_id}/detach
```

Response:
```json
{
    "id": "Focusrite USB ASIO",
    "status": "detached",
    "routes_removed": 5
}
```

Errors:
- `404` - Device not found
- `409` - Device not attached

### Update Device (Rename Alias)

```http
PATCH /api/v1/nodes/{node_id}/devices/{device_id}
Content-Type: application/json

{
    "display_name": "Stage Box Main"
}
```

Response: Updated device info.

---

## Virtual Device Endpoints

### List Virtual Devices

```http
GET /api/v1/nodes/{node_id}/virtual-devices
```

Response: Array of virtual devices.

### Create Virtual Device

```http
POST /api/v1/nodes/{node_id}/virtual-devices
Content-Type: application/json

{
    "name": "VASIO-DAW",
    "input_channels": 16,
    "output_channels": 16,
    "sample_rate": 48000,
    "buffer_size": 64,
    "auto_attach": true
}
```

Constraints:
- `name`: 1-31 characters, alphanumeric + `-_`
- `input_channels`: 2-256, even number
- `output_channels`: 2-256, even number
- `sample_rate`: 44100, 48000, 88200, 96000, 176400, 192000
- `buffer_size`: 32, 64, 128, 256, 512, 1024, 2048

Response:
```json
{
    "id": "VASIO-DAW",
    "system_name": "VASIO-DAW",
    "display_name": null,
    "device_type": "duplex",
    "input_channels": 16,
    "output_channels": 16,
    "sample_rate": 48000,
    "buffer_size": 64,
    "is_virtual": true,
    "status": "attached",
    "backend": "asio"
}
```

Errors:
- `400` - Invalid parameters
- `409` - Device name already exists

### Update Virtual Device

Modify channel count (while DAW connected triggers ASIOResetRequest).

```http
PATCH /api/v1/nodes/{node_id}/virtual-devices/{device_id}
Content-Type: application/json

{
    "input_channels": 32,
    "output_channels": 32
}
```

Response: Updated device info.

Errors:
- `404` - Device not found
- `400` - Cannot change sample_rate/buffer_size while connected

### Delete Virtual Device

```http
DELETE /api/v1/nodes/{node_id}/virtual-devices/{device_id}
```

Response:
```json
{
    "id": "VASIO-DAW",
    "deleted": true,
    "routes_removed": 3
}
```

Errors:
- `404` - Device not found
- `409` - Device in use by DAW (must close first)

---

## Channel Label Endpoints

### Get Channel Labels for Device

```http
GET /api/v1/nodes/{node_id}/devices/{device_id}/channels
```

Response:
```json
{
    "device_id": "Focusrite USB ASIO",
    "input_channels": [
        {"number": 1, "label": "Kick"},
        {"number": 2, "label": "Snare Top"},
        {"number": 3, "label": "Snare Bottom"},
        {"number": 4, "label": null},
        ...
    ],
    "output_channels": [
        {"number": 1, "label": "Main L"},
        {"number": 2, "label": "Main R"},
        ...
    ]
}
```

### Update Single Channel Label

```http
PATCH /api/v1/nodes/{node_id}/devices/{device_id}/channels/{channel_number}
Content-Type: application/json

{
    "direction": "input",  // "input" or "output"
    "label": "Hi-Hat"
}
```

Constraints:
- `label`: 0-31 characters (empty string clears label)
- Forbidden characters: `=`, `.`, `@`
- Must be unique within device direction

Response:
```json
{
    "number": 4,
    "label": "Hi-Hat"
}
```

### Bulk Update Channel Labels

```http
PUT /api/v1/nodes/{node_id}/devices/{device_id}/channels
Content-Type: application/json

{
    "input_channels": [
        {"number": 1, "label": "Kick"},
        {"number": 2, "label": "Snare"},
        {"number": 3, "label": "Hi-Hat"}
    ],
    "output_channels": [
        {"number": 1, "label": "Main L"},
        {"number": 2, "label": "Main R"}
    ]
}
```

Response: Full channel list as in GET.

---

## Route Endpoints

### List Routes

```http
GET /api/v1/routes
```

Query parameters:
- `source_node` - Filter by source node
- `dest_node` - Filter by destination node
- `source_device` - Filter by source device
- `dest_device` - Filter by destination device

Response:
```json
[
    {
        "id": "develbox:Focusrite:1>stagebox1:VASIO-IEM:1",
        "source_node": "develbox",
        "source_device": "Focusrite USB ASIO",
        "source_channel": 1,
        "source_channel_label": "Kick",
        "destination_node": "stagebox1",
        "destination_device": "VASIO-IEM",
        "destination_channel": 1,
        "destination_channel_label": "IEM Mix L",
        "volume": 1.0,
        "muted": false,
        "status": "active"
    }
]
```

### Create Route

```http
POST /api/v1/routes
Content-Type: application/json

{
    "source_node": "develbox",
    "source_device": "Focusrite USB ASIO",
    "source_channel": 1,
    "destination_node": "stagebox1",
    "destination_device": "VASIO-IEM",
    "destination_channel": 1,
    "volume": 1.0,
    "muted": false
}
```

Response:
```json
{
    "id": "develbox:Focusrite:1>stagebox1:VASIO-IEM:1",
    "status": "active"
}
```

Errors:
- `400` - Invalid parameters
- `404` - Source or destination device not found
- `409` - Route already exists
- `422` - Device not attached (must attach first)

### Update Route

```http
PATCH /api/v1/routes/{route_id}
Content-Type: application/json

{
    "volume": 0.75,
    "muted": false
}
```

### Delete Route

```http
DELETE /api/v1/routes/{route_id}
```

---

## Node Endpoints

### List Nodes

```http
GET /api/v1/nodes
```

Response:
```json
[
    {
        "id": "stagebox1",
        "name": "stagebox1",
        "addresses": ["10.77.9.100"],
        "api_port": 8080,
        "vban_port": 6980,
        "online": true,
        "is_local": true
    },
    {
        "id": "develbox",
        "name": "develbox",
        "addresses": ["10.77.9.21"],
        "api_port": 8080,
        "vban_port": 6980,
        "online": true,
        "is_local": false
    }
]
```

### Get Node Details

```http
GET /api/v1/nodes/{node_id}
```

---

## Stream Endpoints

### List Active Streams

```http
GET /api/v1/streams
```

Response:
```json
[
    {
        "device_id": "Focusrite USB ASIO",
        "device_name": "FOH Console",
        "direction": "input",
        "channels": 18,
        "sample_rate": 48000,
        "running": true
    }
]
```

### Get Stream Count

```http
GET /api/v1/streams/count
```

Response:
```json
{
    "input_streams": 2,
    "output_streams": 1
}
```

---

## Subscription Endpoints

### List Subscriptions

```http
GET /api/v1/subscriptions
```

### Create Subscription (Cross-Node)

Called by destination node on source node.

```http
POST /api/v1/subscriptions
Content-Type: application/json

{
    "stream_name": "AM_IEM_1",
    "source_device": "Focusrite USB ASIO",
    "source_channels": [1, 2, 3, 4],
    "destination_node": "stagebox1",
    "destination_addr": "10.77.9.100:6980",
    "sample_rate": 48000
}
```

### Get Subscription Stats

```http
GET /api/v1/subscriptions/stats
```

---

## Latency Endpoint

### Get Route Latency

```http
GET /api/v1/routes/{route_id}/latency
```

Response:
```json
{
    "route_id": "develbox:Focusrite:1>stagebox1:VASIO-IEM:1",
    "input_buffer_ms": 1.33,
    "ring_buffer_ms": 0.50,
    "output_buffer_ms": 1.33,
    "network_ms": 2.10,
    "processing_ms": 0.05,
    "total_ms": 5.31,
    "is_local": false
}
```

---

## Sync Wave Generator Endpoints

Test signal generator for output channels. **Generator state is NOT persisted** - always disabled on restart.

### Start Generator on Channel

```http
POST /api/v1/nodes/{node_id}/devices/{device_id}/channels/{channel}/generator
Content-Type: application/json

{
    "enabled": true,
    "type": "sine",         // sine, pink_noise, channel_id, sweep, click
    "frequency": 1000,      // Hz (for sine mode only)
    "level_db": -20,        // dBFS, max -6 to prevent clipping
    "duration_ms": null     // null = continuous, or timeout in ms
}
```

Response:
```json
{
    "device_id": "VASIO-IEM",
    "channel": 1,
    "generator": {
        "enabled": true,
        "type": "sine",
        "frequency": 1000,
        "level_db": -20
    }
}
```

### Stop Generator on Channel

```http
POST /api/v1/nodes/{node_id}/devices/{device_id}/channels/{channel}/generator
Content-Type: application/json

{
    "enabled": false
}
```

### Start Generator on All Channels

```http
POST /api/v1/nodes/{node_id}/devices/{device_id}/generator/all
Content-Type: application/json

{
    "enabled": true,
    "type": "channel_id",   // Each channel gets unique frequency
    "level_db": -20
}
```

### Stop All Generators

```http
POST /api/v1/nodes/{node_id}/devices/{device_id}/generator/all
Content-Type: application/json

{
    "enabled": false
}
```

### Get Generator Status

```http
GET /api/v1/nodes/{node_id}/devices/{device_id}/generator
```

Response:
```json
{
    "device_id": "VASIO-IEM",
    "channels": [
        {"channel": 1, "enabled": true, "type": "sine", "frequency": 1000, "level_db": -20},
        {"channel": 2, "enabled": false},
        {"channel": 3, "enabled": true, "type": "channel_id", "level_db": -20}
    ]
}
```

### Generator Types

| Type | Description | Parameters |
|------|-------------|------------|
| `sine` | Pure sine wave | frequency (20-20000 Hz) |
| `pink_noise` | Pink noise (equal energy per octave) | None |
| `channel_id` | Unique frequency per channel (440Hz×2^(n/12)) | None |
| `sweep` | Linear frequency sweep 20Hz→20kHz | duration_ms |
| `click` | Periodic impulse (4 Hz) | None |

---

## Health Endpoint

```http
GET /api/v1/health
```

Response:
```json
{
    "status": "ok",
    "version": "0.1.0-dev.8",
    "hostname": "stagebox1"
}
```

---

## WebSocket Events

Connect to: `ws://{node}:{port}/ws`

### Client Commands

```json
// Subscribe to metering for specific device
{"type": "subscribe_metering", "device": "Focusrite USB ASIO"}

// Unsubscribe from metering
{"type": "unsubscribe_metering", "device": "Focusrite USB ASIO"}

// Ping
{"type": "ping"}
```

### Server Events

```json
// Route changed
{
    "type": "route_changed",
    "action": "added",  // "added", "removed", "modified"
    "route_id": "develbox:Focusrite:1>stagebox1:VASIO-IEM:1"
}

// Device status changed
{
    "type": "device_status",
    "node": "stagebox1",
    "device_id": "Focusrite USB ASIO",
    "old_status": "available",
    "new_status": "attached"
}

// Metering data (30 Hz when subscribed)
{
    "type": "metering",
    "node": "stagebox1",
    "device": "Focusrite USB ASIO",
    "direction": "input",
    "levels": [-12.0, -18.0, -24.0, ...],
    "peaks": [-6.0, -9.0, -12.0, ...]
}

// Node status changed
{
    "type": "node_status",
    "node_id": "develbox",
    "online": true
}

// Virtual device created
{
    "type": "virtual_device_created",
    "node": "stagebox1",
    "device": { ... device info ... }
}

// Channel label changed
{
    "type": "channel_label",
    "node": "stagebox1",
    "device_id": "Focusrite USB ASIO",
    "direction": "input",
    "channel": 4,
    "label": "Hi-Hat"
}

// Pong
{"type": "pong"}

// Error
{"type": "error", "message": "Invalid command"}
```

---

## Error Responses

| Code | Meaning |
|------|---------|
| 400 | Invalid request (bad JSON, validation error) |
| 404 | Resource not found |
| 409 | Conflict (already exists, in use) |
| 422 | Unprocessable (device not attached) |
| 503 | Service unavailable (audio engine not ready) |

Error response format:
```json
{
    "error": "device_not_attached",
    "message": "Device must be attached before creating routes",
    "details": {
        "device_id": "Focusrite USB ASIO",
        "current_status": "available"
    }
}
```

---

## mDNS Service

Service type: `_audiomatrix._tcp.local`

### TXT Records

| Key | Value |
|-----|-------|
| version | API version (e.g., "1.0") |
| port | API port |
| hostname | Node name |

---

## Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| Health endpoint | Complete | Basic health check |
| Node endpoints | Complete | List, get |
| Device list endpoint | Complete | Basic listing |
| Device attach/detach | Not Started | User-controlled attachment |
| Device rename | Not Started | Display name/alias |
| Virtual device CRUD | Not Started | Create, update, delete |
| Channel label CRUD | Not Started | Per-channel labels |
| Route endpoints | Complete | CRUD working |
| Stream endpoints | Complete | List, count |
| Subscription endpoints | Complete | Cross-node VBAN |
| Latency endpoint | Complete | Per-route latency |
| WebSocket events | Partial | Route/metering events |
| WebSocket device events | Not Started | Status, label changes |
| Sync Wave Generator | Not Started | Per-output test signals |
| mDNS announce | Complete | Full TXT records |
| mDNS browse | Complete | Node discovery |
