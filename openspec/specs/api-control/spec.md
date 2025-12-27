# API Control Capability

## Purpose

API control provides REST and WebSocket interfaces for system configuration,
real-time state synchronization, authentication, and preset management.

## Requirements

### Requirement: REST API Endpoints

The system SHALL expose a REST API for configuration operations.

#### Scenario: Get system status
- **WHEN** GET /api/v1/status is called
- **THEN** response includes routing state, device status, and metrics

#### Scenario: Create connection
- **WHEN** POST /api/v1/connections is called with valid source/destination
- **THEN** connection is created and confirmed in response
- **AND** WebSocket event is broadcast

#### Scenario: Delete connection
- **WHEN** DELETE /api/v1/connections/{id} is called
- **THEN** connection is removed
- **AND** audio routing stops immediately

### Requirement: WebSocket Real-Time Updates

The system SHALL provide WebSocket for real-time state synchronization.

#### Scenario: Client connection
- **WHEN** a client connects to /api/v1/ws
- **THEN** authentication is validated
- **AND** client receives current state snapshot

#### Scenario: State change broadcast
- **WHEN** routing state changes (connection created/deleted/modified)
- **THEN** all connected WebSocket clients receive event

#### Scenario: Heartbeat mechanism
- **WHEN** no activity for 30 seconds
- **THEN** server sends ping
- **AND** client must respond with pong within 10 seconds
- **OR** connection is closed

### Requirement: Authentication

The system SHALL require authentication for API access.

#### Scenario: Valid bearer token
- **WHEN** request includes valid Authorization: Bearer token
- **THEN** request is processed

#### Scenario: Invalid or missing token
- **WHEN** request lacks valid authentication
- **THEN** 401 Unauthorized is returned

#### Scenario: Token expiry
- **WHEN** token is older than 24 hours
- **THEN** 401 Unauthorized is returned with "token_expired" error

### Requirement: Preset Management

The system SHALL support named configuration presets.

#### Scenario: Save preset
- **WHEN** POST /api/v1/presets is called with name and config
- **THEN** current routing state is saved with given name

#### Scenario: Load preset
- **WHEN** POST /api/v1/presets/{name}/load is called
- **THEN** routing state is restored to saved configuration
- **AND** existing connections are replaced

#### Scenario: List presets
- **WHEN** GET /api/v1/presets is called
- **THEN** all saved preset names and timestamps are returned

### Requirement: Error Response Format

The system SHALL return errors in consistent JSON format.

#### Scenario: Validation error
- **WHEN** request contains invalid parameters
- **THEN** response is 400 with JSON body containing error code and message

#### Scenario: Not found error
- **WHEN** requested resource does not exist
- **THEN** response is 404 with JSON body

### Requirement: API Versioning

The system SHALL version its API for backwards compatibility.

#### Scenario: Versioned endpoint
- **WHEN** /api/v1/status is called
- **THEN** v1 schema is returned

#### Scenario: Missing version
- **WHEN** /api/status is called (no version)
- **THEN** redirect to latest stable version
