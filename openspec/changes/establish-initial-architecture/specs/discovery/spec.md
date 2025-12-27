## ADDED Requirements

### Requirement: Heartbeat Protocol

The system SHALL use lightweight UDP heartbeat packets for connection health monitoring.

#### Scenario: Heartbeat format
- **WHEN** a heartbeat is sent
- **THEN** packet is 16 bytes: magic "AMHB" + timestamp + sequence + flags

#### Scenario: Heartbeat frequency
- **WHEN** subscription is active
- **THEN** destination sends heartbeat every 1 second to source

#### Scenario: Missed heartbeat detection
- **WHEN** 3 consecutive heartbeats are missed (3 seconds)
- **THEN** source stops sending and marks stream as idle

### Requirement: State Synchronization Protocol

The system SHALL synchronize state between discovered computers.

#### Scenario: Discovery handshake
- **WHEN** computer discovers another via mDNS
- **THEN** GET /api/devices is called to retrieve device information

#### Scenario: Subscription request
- **WHEN** destination needs audio from discovered source
- **THEN** SUBSCRIBE message is sent via REST API

### Requirement: Computer Cache Persistence

The system SHALL cache discovered computer information for faster startup.

#### Scenario: Cache structure
- **WHEN** computers are discovered
- **THEN** computer ID, last_seen, address, and devices are stored in cache/computers.toml

#### Scenario: Cache TTL
- **WHEN** computer has not been seen for longer than device_cache_ttl_hours
- **THEN** computer is removed from cache

### Requirement: mDNS TXT Records

The system SHALL include metadata in mDNS TXT records.

#### Scenario: Announce with TXT
- **WHEN** service announces via mDNS
- **THEN** TXT records include version, computer name, and capabilities

### Requirement: Reconnection with Backoff

The system SHALL use exponential backoff for reconnection attempts.

#### Scenario: Initial reconnect
- **WHEN** source goes offline and comes back
- **THEN** first reconnection attempt is after reconnect_initial_ms (default 1s)

#### Scenario: Exponential backoff
- **WHEN** reconnection fails repeatedly
- **THEN** delay doubles up to reconnect_max_ms (default 30s)
- **AND** random jitter of ±20% is applied
