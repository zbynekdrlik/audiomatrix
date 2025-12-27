## ADDED Requirements

### Requirement: VBAN Header Format

The system SHALL use the standard 28-byte VBAN header format for all audio packets.

#### Scenario: Header structure
- **WHEN** a VBAN packet is constructed
- **THEN** bytes 0-3 contain magic "VBAN" (0x56 0x42 0x41 0x4E)
- **AND** byte 4 contains sample rate index (5 bits) + sub-protocol (3 bits)
- **AND** byte 5 contains samples per frame
- **AND** byte 6 contains channels minus 1
- **AND** byte 7 contains format code
- **AND** bytes 8-23 contain null-padded stream name
- **AND** bytes 24-27 contain 32-bit frame counter

### Requirement: Optimized Packet Transmission

The system SHALL use pre-allocated buffers and avoid allocation in the audio path.

#### Scenario: Packet pool usage
- **WHEN** a VBAN packet needs to be sent
- **THEN** a buffer is acquired from the pre-allocated packet pool
- **AND** no heap allocation occurs

#### Scenario: Immediate send
- **WHEN** audio data is ready for transmission
- **THEN** packets are sent immediately without Nagle buffering

### Requirement: SUBSCRIBE Protocol

The system SHALL use a REST-based SUBSCRIBE protocol for stream establishment.

#### Scenario: Subscribe request
- **WHEN** destination needs audio from source
- **THEN** POST /api/streams/subscribe is sent to source with stream_id, channels, dest_ip, dest_port

#### Scenario: Subscribe response
- **WHEN** source accepts subscription
- **THEN** response includes stream_id, status, and 16-char vban_stream_name

#### Scenario: Unsubscribe
- **WHEN** connection is removed
- **THEN** DELETE /api/streams/subscribe/{stream_id} is sent to source

### Requirement: Adaptive Jitter Buffer

The system SHALL dynamically adjust jitter buffer size based on network conditions.

#### Scenario: Jitter adaptation
- **WHEN** packet jitter increases
- **THEN** buffer size increases within configured range (1.0-5.0ms default)

#### Scenario: Low jitter optimization
- **WHEN** consistent low jitter is detected
- **THEN** buffer size decreases to minimize latency

### Requirement: Packet Loss Handling

The system SHALL handle missing packets gracefully without audio glitches.

#### Scenario: Fill missing samples
- **WHEN** a sequence gap is detected within jitter window
- **THEN** gap is filled with silence or interpolated samples
- **AND** loss counter is incremented

#### Scenario: Packet timeout
- **WHEN** no packets received for configured timeout (default 100ms)
- **THEN** stream is marked as offline
