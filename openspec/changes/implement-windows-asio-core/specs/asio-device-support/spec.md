# Spec: ASIO Device Support

## ADDED Requirements

### Requirement: Hardware ASIO Enumeration
The system SHALL enumerate all hardware ASIO devices available on Windows.

#### Scenario: Detect Dante ASIO device
- **Given** a Windows PC with Dante Virtual Soundcard or Dante PCIe card installed
- **When** AudioMatrix starts or refreshes devices
- **Then** the Dante ASIO device appears in the device list with correct channel count

#### Scenario: Detect multiple ASIO devices
- **Given** a Windows PC with RME Fireface and Focusrite ASIO devices
- **When** AudioMatrix enumerates devices
- **Then** both devices appear with their respective channel configurations

### Requirement: Hardware ASIO Connection
The system SHALL connect to hardware ASIO devices and access audio streams.

#### Scenario: Open ASIO device for input
- **Given** a hardware ASIO device with 16 input channels
- **When** the device is activated for routing
- **Then** all 16 input channels are available for routing
- **And** audio samples flow from device to ring buffers

#### Scenario: Open ASIO device for output
- **Given** a hardware ASIO device with 16 output channels
- **When** the device is activated for routing
- **Then** all 16 output channels are available as destinations
- **And** audio samples flow from ring buffers to device

### Requirement: Virtual ASIO Device Creation
The system SHALL create virtual ASIO devices visible to Windows applications.

#### Scenario: Create virtual device via API
- **Given** AudioMatrix is running
- **When** POST /api/v1/virtual-devices with name="VASIO-DAW1" channels=32
- **Then** a new virtual ASIO device appears in Windows ASIO device list
- **And** Ableton can select "AudioMatrix VASIO-DAW1" as audio device

#### Scenario: Multiple virtual devices
- **Given** two virtual devices created (VASIO-DAW1, VASIO-DAW2)
- **When** Ableton instance 1 opens VASIO-DAW1
- **And** Ableton instance 2 opens VASIO-DAW2
- **Then** both DAWs operate independently without conflict

### Requirement: Virtual Device Configuration
The system SHALL allow configuration of virtual ASIO device properties.

#### Scenario: Configure channel count
- **Given** a virtual device request with channels=64
- **When** the device is created
- **Then** the device exposes 64 input and 64 output channels to applications

#### Scenario: Delete virtual device
- **Given** a virtual device "VASIO-OLD" exists and is not in use
- **When** DELETE /api/v1/virtual-devices/vasio-old
- **Then** the device is removed from Windows ASIO list
- **And** the device no longer appears in DAW device selectors

### Requirement: Real-time Audio Routing
The system SHALL route audio between ASIO devices with minimal latency.

#### Scenario: Route hardware input to virtual output
- **Given** hardware ASIO device with audio on input channel 1
- **And** virtual ASIO device VASIO-DAW1
- **And** a route from hardware:ch1 to VASIO-DAW1:ch1
- **When** audio plays on hardware input
- **Then** the same audio appears on VASIO-DAW1 output channel 1
- **And** latency is less than 3ms at 64-sample buffer

#### Scenario: Route virtual input to hardware output
- **Given** Ableton playing audio to VASIO-DAW1 input
- **And** a route from VASIO-DAW1:ch1 to hardware:ch1
- **When** Ableton plays audio
- **Then** audio appears on hardware output channel 1

### Requirement: Network Audio Routing
The system SHALL route audio between nodes over the network.

#### Scenario: Route from PC1 hardware to PC2 virtual device
- **Given** PC1 with hardware ASIO device receiving audio
- **And** PC2 with virtual ASIO device VASIO-Main
- **And** a route from PC1:hardware:ch1 to PC2:VASIO-Main:ch1
- **When** audio plays on PC1 hardware input
- **Then** the audio appears on PC2 VASIO-Main output
- **And** network latency is less than 10ms

#### Scenario: Bidirectional network routing
- **Given** PC1 and PC2 both running AudioMatrix
- **When** routes configured in both directions
- **Then** audio flows bidirectionally without feedback loops

## MODIFIED Requirements

### Requirement: Device Enumeration (Modified)
The existing device enumeration SHALL prioritize ASIO devices on Windows.

#### Scenario: ASIO devices listed first
- **Given** a Windows PC with WASAPI and ASIO devices
- **When** devices are enumerated
- **Then** ASIO devices appear before WASAPI devices in the list
- **And** ASIO devices are marked as "preferred" for routing

## Performance Requirements

### Requirement: Latency Bounds
- Local ASIO routing: <3ms at 64-sample buffer
- Network routing: <10ms at 64-sample buffer
- Virtual device overhead: <0.5ms additional

### Requirement: Lock-free Audio Path
- ASIO callbacks must not allocate memory
- ASIO callbacks must not take locks
- All audio path operations must be wait-free
