# VBAN Transport Capability

## Purpose

VBAN transport handles network audio streaming using the VB-Audio Network protocol,
enabling real-time audio transmission over IP networks with low latency.

## Requirements

### Requirement: VBAN Packet Transmission

The system SHALL transmit audio as VBAN-compliant UDP packets.

#### Scenario: Send audio packet
- **WHEN** audio samples are ready for transmission
- **THEN** a VBAN packet is constructed with 28-byte header
- **AND** samples are packed in network byte order
- **AND** packet is sent via UDP to target address

#### Scenario: Stream naming
- **WHEN** a VBAN stream is created
- **THEN** stream name is included in packet header (max 16 chars)
- **AND** name is null-terminated if shorter than 16 chars

### Requirement: VBAN Packet Reception

The system SHALL receive and decode VBAN audio packets.

#### Scenario: Receive valid packet
- **WHEN** a UDP packet arrives with valid VBAN header
- **THEN** header magic ("VBAN") is verified
- **AND** samples are extracted and queued for playback

#### Scenario: Reject invalid packets
- **WHEN** a packet has invalid VBAN magic or version
- **THEN** packet is discarded silently
- **AND** error counter is incremented

### Requirement: Sample Rate Configuration

The system SHALL support VBAN sample rate indices.

#### Scenario: 48kHz stream configuration
- **WHEN** sample rate index 3 is specified
- **THEN** stream operates at 48000 Hz

#### Scenario: Sample rate mismatch handling
- **WHEN** received stream rate differs from device rate
- **THEN** resampling is applied transparently

### Requirement: Jitter Buffer

The system SHALL buffer incoming packets to handle network jitter.

#### Scenario: Packet reordering
- **WHEN** packets arrive out of sequence (within jitter window)
- **THEN** packets are reordered before playback

#### Scenario: Buffer underrun
- **WHEN** jitter buffer empties before packets arrive
- **THEN** silence is output
- **AND** underrun metric is incremented

### Requirement: Sequence Number Handling

The system SHALL track packet sequence numbers.

#### Scenario: Detect packet loss
- **WHEN** sequence number gap is detected
- **THEN** loss counter is incremented
- **AND** gap is filled with interpolated or silent samples

#### Scenario: Sequence wraparound
- **WHEN** sequence number wraps from 255 to 0
- **THEN** continuity is maintained without disruption

### Requirement: Multi-Channel Support

The system SHALL support up to 8 channels per VBAN stream.

#### Scenario: 8-channel stream
- **WHEN** an 8-channel VBAN stream is configured
- **THEN** all 8 channels are transmitted in each packet
- **AND** channel count is correctly encoded in header
