## ADDED Requirements

### Requirement: Service Configuration

The system SHALL load configuration from config.toml on startup.

#### Scenario: Service section
- **WHEN** service starts
- **THEN** computer_name, api_port, and auto_start are read from [service]

#### Scenario: Audio section
- **WHEN** audio configuration is read
- **THEN** default_sample_rate, default_buffer_size, and max_channels_per_device are applied

### Requirement: Network Configuration

The system SHALL configure VBAN network parameters.

#### Scenario: Port allocation
- **WHEN** destination needs to receive VBAN stream
- **THEN** port is allocated from vban_base_port to vban_base_port + max_vban_streams

#### Scenario: Jitter buffer tuning
- **WHEN** jitter_buffer_ms is configured
- **THEN** initial jitter buffer size is set accordingly
- **AND** adaptive tuning operates within configured range

### Requirement: Resampler Configuration

The system SHALL allow resampler algorithm selection.

#### Scenario: Algorithm selection
- **WHEN** audio.resampler.algorithm is "sinc"
- **THEN** high-quality sinc resampling is used with ~0.5ms latency

#### Scenario: Fastest mode
- **WHEN** audio.resampler.algorithm is "linear"
- **THEN** lowest CPU linear resampling is used

### Requirement: Storage Layout

The system SHALL organize files according to platform conventions.

#### Scenario: Windows storage
- **WHEN** running on Windows
- **THEN** files are stored under %PROGRAMDATA%\AudioMatrix\

#### Scenario: Linux storage
- **WHEN** running on Linux
- **THEN** files are stored under /var/lib/audiomatrix/

### Requirement: Virtual Device Files

The system SHALL store virtual device definitions in separate files.

#### Scenario: Device file format
- **WHEN** virtual device is created
- **THEN** devices/<device-name>.toml contains name, channels, sample_rate, buffer_size

#### Scenario: Channel names in device file
- **WHEN** channels have custom names
- **THEN** names are stored in [channels.inputs] and [channels.outputs] sections

### Requirement: Preset File Format

The system SHALL store presets in TOML format.

#### Scenario: Local preset
- **WHEN** preset is saved locally
- **THEN** destination.computer is omitted (implicit LOCAL)

#### Scenario: Global preset
- **WHEN** preset is saved globally
- **THEN** destination.computer is included for cross-computer routing
