## ADDED Requirements

### Requirement: Batch Connection Operations

The system SHALL support batch create, update, and delete of connections in a single request.

#### Scenario: Batch request
- **WHEN** POST /api/v1/connections/batch is called with create, update, and delete arrays
- **THEN** all operations are processed
- **AND** individual success/failure status is returned for each operation

#### Scenario: Partial failure
- **WHEN** some operations in a batch fail
- **THEN** successful operations are committed
- **AND** failed operations are reported with error details

### Requirement: Computer Endpoints

The system SHALL expose endpoints for discovered computers.

#### Scenario: List computers
- **WHEN** GET /api/v1/computers is called
- **THEN** all discovered computers are returned with online status

#### Scenario: Computer devices
- **WHEN** GET /api/v1/computers/{id}/devices is called
- **THEN** all devices on that computer are returned

### Requirement: Monitoring Endpoints

The system SHALL expose real-time monitoring data via API.

#### Scenario: Get meters
- **WHEN** GET /api/v1/meters is called
- **THEN** current level meters for all channels are returned (in dBFS)

#### Scenario: Get diagnostics
- **WHEN** GET /api/v1/diagnostics is called
- **THEN** latency, jitter, packet loss, and error statistics are returned

### Requirement: WebSocket Event Topics

The system SHALL support topic-based subscription for WebSocket events.

#### Scenario: Subscribe to topics
- **WHEN** client sends { type: "subscribe", topics: ["meters", "connections"] }
- **THEN** client receives only events for subscribed topics

#### Scenario: Meters broadcast
- **WHEN** meters topic is subscribed
- **THEN** level updates are sent at approximately 10Hz

### Requirement: WebSocket Commands

The system SHALL accept commands via WebSocket as alternative to REST.

#### Scenario: Create connection via WebSocket
- **WHEN** client sends { type: "connection.create", source: "...", destination: "..." }
- **THEN** connection is created and confirmation event is broadcast

#### Scenario: Update connection via WebSocket
- **WHEN** client sends { type: "connection.update", id: "...", gain_db: -3.0 }
- **THEN** connection is updated and change event is broadcast

### Requirement: Connection Status Events

The system SHALL broadcast connection status changes via WebSocket.

#### Scenario: Status change event
- **WHEN** connection status changes from "waiting" to "active"
- **THEN** event { type: "connection.status", id: "...", status: "active" } is broadcast

#### Scenario: Clip detection event
- **WHEN** clipping is detected on a channel
- **THEN** event { type: "meters.clip", channel: "..." } is broadcast
