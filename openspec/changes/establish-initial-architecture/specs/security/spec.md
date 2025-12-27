## ADDED Requirements

### Requirement: Authentication Modes

The system SHALL support multiple authentication modes for different deployment scenarios.

#### Scenario: No authentication (default)
- **WHEN** security.mode is "none"
- **THEN** all LAN users can control the system without credentials

#### Scenario: PIN authentication
- **WHEN** security.mode is "pin"
- **THEN** 4-6 digit PIN is required for control operations

#### Scenario: Token authentication
- **WHEN** security.mode is "token"
- **THEN** API token is required in Authorization: Bearer header

#### Scenario: Full authentication
- **WHEN** security.mode is "full"
- **THEN** username/password with role-based access is required

### Requirement: WebSocket Authentication

The system SHALL authenticate WebSocket connections via query parameter.

#### Scenario: Token in query string
- **WHEN** client connects to /ws?auth=<token>
- **THEN** token is validated at connection time

#### Scenario: Invalid token rejection
- **WHEN** auth token is invalid or missing (when required)
- **THEN** connection is rejected with 401 status

### Requirement: Permission Enforcement

The system SHALL enforce permissions on API operations.

#### Scenario: Read-only without auth
- **WHEN** read_only_without_auth is true
- **THEN** GET requests are allowed without authentication

#### Scenario: Permission denied
- **WHEN** user lacks required permission for operation
- **THEN** HTTP 403 Forbidden is returned

#### Scenario: Role-based permissions
- **WHEN** user has "operator" role
- **THEN** connect, disconnect, mute, and gain operations are allowed
- **AND** config and device.create operations are denied

### Requirement: Token Generation

The system SHALL generate secure tokens automatically.

#### Scenario: Auto-generate token
- **WHEN** api_token is empty and mode is "token"
- **THEN** 32-byte random token is generated on first startup
- **AND** token is saved to config and written to api_token.txt

#### Scenario: Token regeneration
- **WHEN** api_token value is deleted
- **THEN** new token is generated on next restart

### Requirement: VBAN Security Warning

The system SHALL document VBAN security limitations.

#### Scenario: VBAN has no encryption
- **WHEN** VBAN streams are used
- **THEN** documentation warns that protocol has no authentication or encryption

#### Scenario: Mitigation recommendations
- **WHEN** deploying AudioMatrix
- **THEN** dedicated VLAN for audio network is recommended
