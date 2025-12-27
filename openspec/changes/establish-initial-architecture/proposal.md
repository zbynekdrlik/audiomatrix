# Change: Establish Initial AudioMatrix Architecture

## Why

ARCHITECTURE.md defines the complete AudioMatrix system specification including:
- Lock-free audio routing with per-connection controls
- VBAN network transport with jitter buffering
- Virtual ASIO device lifecycle management
- Destination-owned subscription model
- REST/WebSocket API for control
- mDNS service discovery
- Security and authentication framework

The existing OpenSpec specs contain basic requirements but lack the detailed specifications
from ARCHITECTURE.md needed to drive implementation. This proposal establishes the full
architectural specification as the authoritative source for development.

## What Changes

- **audio-routing**: Add connection data model, headroom modes, bulk operations, solo behavior
- **vban-transport**: Add packet format details, sender optimization, SUBSCRIBE protocol
- **device-management**: Add virtual ASIO lifecycle, live resize, channel naming
- **api-control**: Add batch operations, computers/monitoring endpoints, WebSocket events
- **discovery**: Add heartbeat protocol, state sync protocol, cache structure
- **security** (NEW): Authentication modes, permissions, VBAN security considerations
- **state-management** (NEW): Destination-owned subscriptions, connection state machine, offline configuration
- **configuration** (NEW): config.toml structure, subscription storage format

## Impact

- Affected specs: All 5 existing + 3 new capabilities
- Affected code: This establishes requirements before implementation begins
- Breaking changes: None (greenfield project)
