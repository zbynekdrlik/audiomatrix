## ADDED Requirements

### Requirement: Connection Data Model

The system SHALL identify connections using a composite ID format: `{src_computer}:{src_device}:{src_ch}>{dst_computer}:{dst_device}:{dst_ch}`.

#### Scenario: Connection ID generation
- **WHEN** a connection is created from STUDIO-PC:Focusrite:1 to LIVE-PC:VASIO-DAW:1
- **THEN** the connection ID is "STUDIO-PC:Focusrite:1>LIVE-PC:VASIO-DAW:1"

#### Scenario: Local connection ID
- **WHEN** source and destination are on the same computer
- **THEN** destination computer in subscriptions.toml is "LOCAL"

### Requirement: Connection Enable Control

The system SHALL provide an enabled flag separate from mute that removes connections from the audio path entirely.

#### Scenario: Disabled connection saves CPU
- **WHEN** enabled is set to false on a connection
- **THEN** the connection is skipped in audio processing
- **AND** no samples are read from the source buffer

#### Scenario: Enabled vs muted semantics
- **WHEN** enabled=true and mute=true
- **THEN** audio is processed but output is silenced (zero samples)

### Requirement: Headroom Management

The system SHALL provide configurable headroom modes for N:1 mixing destinations.

#### Scenario: Clip mode (default)
- **WHEN** headroom mode is "clip" and mixed output exceeds 1.0
- **THEN** samples are hard-clipped to [-1.0, 1.0]

#### Scenario: AutoGain mode
- **WHEN** headroom mode is "auto_gain" and peak exceeds 1.0
- **THEN** all samples are scaled proportionally to prevent clipping

#### Scenario: Limiter mode
- **WHEN** headroom mode is "limiter"
- **THEN** soft-knee limiting is applied with approximately 0.5ms added latency

### Requirement: Solo Behavior

The system SHALL provide UI-only solo functionality per destination.

#### Scenario: Solo mutes other sources
- **WHEN** source A is soloed to destination X
- **THEN** all other sources to destination X are temporarily muted
- **AND** other destinations are unaffected

#### Scenario: Multiple solos
- **WHEN** sources A and B are both soloed to destination X
- **THEN** only A and B are heard at destination X

### Requirement: Bulk Operations

The system SHALL support batch create, update, and delete of connections.

#### Scenario: Batch create connections
- **WHEN** multiple connections are created in one request
- **THEN** all are processed in a single operation
- **AND** partial success is reported if some fail

#### Scenario: Relative gain adjustment
- **WHEN** multiple connections are selected for bulk gain change
- **THEN** gain adjustment is applied relative to each connection's current gain

### Requirement: Connection Version Control

The system SHALL use version numbers for optimistic locking on concurrent modifications.

#### Scenario: Concurrent update conflict
- **WHEN** two controllers update the same connection with the same version
- **THEN** the first write succeeds with incremented version
- **AND** the second write returns a version conflict error
