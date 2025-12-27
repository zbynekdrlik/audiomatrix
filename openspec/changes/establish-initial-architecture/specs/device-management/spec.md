## ADDED Requirements

### Requirement: Virtual ASIO Lifecycle

The system SHALL manage virtual ASIO device lifecycle including COM registration on Windows.

#### Scenario: Create virtual device
- **WHEN** a virtual ASIO device is created
- **THEN** a unique CLSID is generated
- **AND** COM object is registered in Windows Registry (HKLM\SOFTWARE\ASIO)
- **AND** device appears in ASIO device lists after DAW rescan

#### Scenario: Connect DAW
- **WHEN** DAW opens the virtual device
- **THEN** ASIOInit() connects to AudioMatrix service via shared memory
- **AND** audio buffers are allocated based on DAW's requested buffer size

#### Scenario: Delete virtual device
- **WHEN** a virtual device is deleted
- **THEN** connected DAWs receive ASIOStop() + ASIODisposeBuffers()
- **AND** all routes using the device are disconnected
- **AND** COM object is unregistered

### Requirement: Live Channel Resize

The system SHALL support resizing virtual device channels without recreation.

#### Scenario: Expand channels
- **WHEN** channel count is increased
- **THEN** new channels are immediately available for routing
- **AND** DAW receives ASIOResetRequest to re-query channel count

#### Scenario: Shrink channels
- **WHEN** channel count is decreased
- **THEN** highest-numbered channels are removed
- **AND** affected routes are disconnected
- **AND** DAW receives ASIOResetRequest

### Requirement: Channel Naming

The system SHALL support custom names for device channels.

#### Scenario: Named channel
- **WHEN** channel 1 has custom name "DAW Out L"
- **THEN** API returns name "DAW Out L" for that channel
- **AND** default name (channel index as string) is used for unnamed channels

#### Scenario: Persist channel names
- **WHEN** channel names are set
- **THEN** names are stored in device TOML file
- **AND** names survive service restart

### Requirement: Device Identification

The system SHALL identify devices using computer:device format.

#### Scenario: Device ID format
- **WHEN** device "Focusrite 18i20" is on computer "STUDIO-PC"
- **THEN** device ID is "STUDIO-PC:Focusrite 18i20"

#### Scenario: Local device
- **WHEN** querying local devices
- **THEN** computer name matches configured service computer_name

### Requirement: Sample Rate Mismatch Handling

The system SHALL handle sample rate mismatches between source and destination.

#### Scenario: Automatic resampling
- **WHEN** sample_rate_mismatch config is "resample"
- **THEN** destination applies resampling transparently

#### Scenario: Block mismatched connection
- **WHEN** sample_rate_mismatch config is "block"
- **THEN** connection creation fails with sample rate error

#### Scenario: Warn on mismatch
- **WHEN** sample_rate_mismatch config is "warn"
- **THEN** connection is allowed but warning is logged
