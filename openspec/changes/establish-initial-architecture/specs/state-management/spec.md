## ADDED Requirements

### Requirement: Destination-Owned Subscriptions

The system SHALL use a receiver-centric model where destinations own their subscriptions.

#### Scenario: Destination stores subscription
- **WHEN** a connection is created
- **THEN** subscription is stored in destination's subscriptions.toml
- **AND** source stores only ephemeral "who requested from me" state

#### Scenario: Source failure recovery
- **WHEN** source goes offline and returns
- **THEN** destination reloads subscriptions and re-requests from source
- **AND** no user intervention required

### Requirement: Connection State Machine

The system SHALL track connection states through a defined lifecycle.

#### Scenario: State transitions
- **WHEN** user clicks crosspoint
- **THEN** state progresses: INTENT → REQUESTED → PENDING → ACTIVE

#### Scenario: Source offline
- **WHEN** source becomes unavailable
- **THEN** connection transitions to WAITING state
- **AND** subscription remains persisted for auto-recovery

#### Scenario: Error state
- **WHEN** connection fails (sample rate mismatch, etc.)
- **THEN** connection transitions to ERROR state with error_reason

### Requirement: Subscription Persistence

The system SHALL persist subscriptions in TOML format.

#### Scenario: Subscription file format
- **WHEN** subscription is saved
- **THEN** subscriptions.toml contains destination, source, gain_db, mute, enabled, version

#### Scenario: Startup reload
- **WHEN** service starts
- **THEN** all subscriptions are loaded and marked WAITING
- **AND** re-connection attempts begin for online sources

### Requirement: Offline Configuration

The system SHALL allow configuring routes to offline devices.

#### Scenario: Configure offline source
- **WHEN** source computer is offline
- **THEN** destination stores subscription immediately
- **AND** connection shows WAITING state until source appears

#### Scenario: Configure offline destination
- **WHEN** destination computer is offline
- **THEN** controller stores pending subscription locally
- **AND** subscription is pushed when destination comes online

### Requirement: Preset Distribution

The system SHALL distribute global presets to multiple computers.

#### Scenario: Load global preset
- **WHEN** preset with subscriptions for multiple computers is loaded
- **THEN** subscriptions are grouped by destination computer
- **AND** each computer receives its relevant subscriptions

#### Scenario: Partial online
- **WHEN** some destination computers are offline during preset load
- **THEN** pending subscriptions are stored for later push
- **AND** online computers receive subscriptions immediately

### Requirement: Status Persistence

The system SHALL persist connection status appropriately.

#### Scenario: Active state not persisted
- **WHEN** connection is active
- **THEN** "active" status is runtime-only (not saved to file)

#### Scenario: Waiting state persisted
- **WHEN** connection is waiting for source
- **THEN** "waiting" status is persisted for startup recovery
