# AudioMatrix - API Architecture

## REST API

Base URL: `http://{computer}:{port}/api/v1`

Default port: 8400

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | /devices | List all devices |
| GET | /devices/{id} | Device details |
| POST | /devices | Create virtual device |
| DELETE | /devices/{id} | Delete virtual device |
| GET | /routes | List active routes |
| POST | /routes | Create route |
| DELETE | /routes/{id} | Delete route |
| PATCH | /routes/{id} | Update route (gain, mute) |
| GET | /computers | List discovered computers |
| GET | /status | Service status |

### Error Responses

| Code | Meaning |
|------|---------|
| 400 | Invalid request (bad JSON, missing fields) |
| 404 | Resource not found |
| 409 | Conflict (route already exists) |
| 503 | Service unavailable (audio engine not ready) |

## WebSocket Events

Connect to: `ws://{computer}:{port}/ws`

### Event Types

| Event | Direction | Description |
|-------|-----------|-------------|
| `route.created` | Server → Client | New route established |
| `route.deleted` | Server → Client | Route removed |
| `route.updated` | Server → Client | Route parameters changed |
| `device.added` | Server → Client | Device appeared |
| `device.removed` | Server → Client | Device disappeared |
| `metering` | Server → Client | Audio levels (periodic) |
| `computer.online` | Server → Client | Remote computer discovered |
| `computer.offline` | Server → Client | Remote computer lost |

### Subscription Model

Clients subscribe to specific event categories:
- `routes` - Route changes
- `devices` - Device changes
- `metering` - Audio levels (higher bandwidth)
- `computers` - Network discovery

## mDNS Service

Service type: `_audiomatrix._tcp.local`

### TXT Records

| Key | Value |
|-----|-------|
| version | API version (e.g., "1.0") |
| port | API port |
| hostname | Computer name |

### Discovery Flow

1. Client queries for `_audiomatrix._tcp.local`
2. Receives list of instances with IP/port
3. Connects to REST API for device enumeration
4. Optionally opens WebSocket for live updates

## Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| REST endpoints | Partial | Basic CRUD working |
| WebSocket | Partial | Event types defined |
| Metering | Not Started | Infrastructure ready |
| mDNS announce | Complete | Full TXT records |
| mDNS browse | Complete | Device discovery |
| Authentication | Partial | Basic structure only |
