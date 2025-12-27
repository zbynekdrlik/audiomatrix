# Discovery Capability

## Purpose

Discovery enables automatic network service discovery using mDNS/DNS-SD,
allowing AudioMatrix nodes to find each other and available VBAN streams.

## Requirements

### Requirement: Service Announcement

The system SHALL announce its presence via mDNS.

#### Scenario: Announce on startup
- **WHEN** the service starts
- **THEN** mDNS record is published for _audiomatrix._tcp.local
- **AND** TXT records include version, node ID, and capabilities

#### Scenario: De-announce on shutdown
- **WHEN** the service shuts down gracefully
- **THEN** mDNS record is removed
- **AND** other nodes detect departure

### Requirement: Service Browsing

The system SHALL discover other AudioMatrix nodes on the network.

#### Scenario: Discover peer nodes
- **WHEN** service browsing is active
- **THEN** all AudioMatrix nodes on local network are discovered
- **AND** node list is updated in real-time

#### Scenario: Node metadata
- **WHEN** a node is discovered
- **THEN** its hostname, IP address, port, and capabilities are extracted

### Requirement: VBAN Stream Discovery

The system SHALL discover VBAN streams from other sources.

#### Scenario: Discover VBAN senders
- **WHEN** VBAN traffic is detected on the network
- **THEN** sender IP, stream name, and format are recorded

#### Scenario: Stream timeout
- **WHEN** no VBAN packets from a stream for 5 seconds
- **THEN** stream is marked as inactive

### Requirement: Node Health Monitoring

The system SHALL monitor discovered node health.

#### Scenario: Heartbeat reception
- **WHEN** heartbeat is received from a discovered node
- **THEN** node is marked as healthy
- **AND** last-seen timestamp is updated

#### Scenario: Node timeout
- **WHEN** no heartbeat from a node for 30 seconds
- **THEN** node is marked as unreachable
- **AND** dependent connections are flagged

### Requirement: Subscription Propagation

The system SHALL propagate subscription requests to discovered nodes.

#### Scenario: Subscribe to remote source
- **WHEN** a destination subscribes to a source on another node
- **THEN** SUBSCRIBE message is sent to that node
- **AND** source begins transmitting to subscriber

#### Scenario: Subscription cleanup
- **WHEN** a connection is deleted
- **THEN** UNSUBSCRIBE message is sent to source node

### Requirement: Network Interface Selection

The system SHALL support selecting network interfaces for discovery.

#### Scenario: Single interface mode
- **WHEN** a specific interface is configured
- **THEN** mDNS operates only on that interface

#### Scenario: All interfaces mode
- **WHEN** no interface is specified
- **THEN** mDNS operates on all available interfaces
