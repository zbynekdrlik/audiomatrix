# Audio Routing Capability

## Purpose

Audio routing manages the flow of audio samples between sources and destinations,
providing per-connection volume control, mute functionality, and multi-source mixing.

## Requirements

### Requirement: Source-to-Destination Routing

The system SHALL route audio from any source endpoint to any destination endpoint.

#### Scenario: Local device to local device routing
- **WHEN** a connection exists from hardware input to hardware output
- **THEN** audio samples are copied with correct channel mapping
- **AND** latency does not exceed the configured buffer size

#### Scenario: VBAN source to local destination
- **WHEN** a connection exists from a VBAN stream to a hardware output
- **THEN** incoming network packets are decoded and routed to the output
- **AND** sample rate conversion is applied if rates differ

### Requirement: Per-Connection Volume Control

The system SHALL provide independent volume control for each connection point.

#### Scenario: Apply volume gain to connection
- **WHEN** volume is set to 0.5 (-6dB) on a connection
- **THEN** output samples are attenuated by 50%
- **AND** volume changes apply smoothly without clicks

#### Scenario: Volume at unity gain
- **WHEN** volume is set to 1.0 (0dB)
- **THEN** samples pass through unchanged

### Requirement: Per-Connection Mute Control

The system SHALL provide independent mute control for each connection point.

#### Scenario: Mute a connection
- **WHEN** mute is enabled on a connection
- **THEN** zero samples are sent to the destination
- **AND** source continues to produce audio (other connections unaffected)

#### Scenario: Unmute transition
- **WHEN** mute is disabled on a previously muted connection
- **THEN** audio resumes with smooth fade-in to prevent clicks

### Requirement: Multi-Source Mixing

The system SHALL mix multiple sources into a single destination.

#### Scenario: Sum two sources
- **WHEN** two sources route to the same destination channel
- **THEN** their samples are summed with proper normalization
- **AND** clipping protection prevents overflow

### Requirement: Channel Mapping

The system SHALL support flexible channel mapping within connections.

#### Scenario: Stereo to mono downmix
- **WHEN** a stereo source routes to a mono destination
- **THEN** left and right channels are summed and normalized

#### Scenario: Individual channel routing
- **WHEN** source channel 2 maps to destination channel 5
- **THEN** only channel 2 samples appear at channel 5

### Requirement: Lock-Free Audio Path

The system SHALL process audio without locks or blocking operations.

#### Scenario: Audio callback execution
- **WHEN** the audio callback is invoked
- **THEN** all operations complete within the buffer period
- **AND** no mutex locks are acquired
- **AND** no heap allocations occur
