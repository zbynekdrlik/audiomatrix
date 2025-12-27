# Tasks: Implement Windows ASIO Core

## Phase 1: Hardware ASIO Device Access

### 1.1 ASIO SDK Setup
- [ ] Download and integrate Steinberg ASIO SDK
- [ ] Configure Cargo.toml with `asio` feature flag for cpal
- [ ] Set up Windows CI/CD with ASIO SDK
- [ ] Test ASIO device enumeration on Windows test machine

### 1.2 ASIO Device Enumeration
- [ ] Implement ASIO host enumeration in ram-core
- [ ] Detect hardware ASIO devices (Dante, RME, etc.)
- [ ] Expose ASIO devices through existing DeviceManager API
- [ ] Add device capabilities query (channels, sample rates, buffer sizes)

### 1.3 ASIO Device Connection
- [ ] Implement ASIO stream opening in ram-asio crate
- [ ] Handle ASIO buffer callbacks (`bufferSwitch`)
- [ ] Implement lock-free sample transfer to/from ring buffers
- [ ] Add proper error handling and device recovery

## Phase 2: Virtual ASIO Devices

### 2.1 Virtual Driver Research
- [ ] Research Windows virtual audio driver approaches:
  - VB-Audio Cable pattern (kernel driver)
  - Windows Audio Session API (WASAPI) loopback
  - WDM virtual driver
- [ ] Evaluate existing open-source virtual audio drivers
- [ ] Document chosen approach with rationale

### 2.2 Virtual ASIO Driver Implementation
- [ ] Create virtual ASIO driver skeleton
- [ ] Implement driver registration with Windows
- [ ] Make virtual device appear in ASIO device list
- [ ] Support multiple virtual device instances
- [ ] Implement configurable channel counts (2-64 channels per device)

### 2.3 Virtual Device API
- [ ] Add REST API endpoints for virtual device management
- [ ] Implement create/delete virtual device operations
- [ ] Add channel count configuration
- [ ] Store virtual device config in persistence layer

## Phase 3: Audio Routing Engine

### 3.1 Real-time Audio Loop
- [ ] Implement main audio processing loop in ram-core
- [ ] Connect ASIO callbacks to routing matrix
- [ ] Implement lock-free sample routing between devices
- [ ] Add per-channel gain and mute application

### 3.2 Internal Routing
- [ ] Route audio from hardware ASIO inputs to virtual ASIO outputs
- [ ] Route audio from virtual ASIO inputs to hardware ASIO outputs
- [ ] Support channel-level routing (any channel to any channel)
- [ ] Implement zero-latency local routing path

### 3.3 Metering
- [ ] Calculate peak/RMS levels for all channels
- [ ] Expose metering via WebSocket events
- [ ] Optimize metering for minimal CPU impact

## Phase 4: Network Audio (VBAN)

### 4.1 VBAN Integration
- [ ] Connect existing VBAN protocol to routing engine
- [ ] Send local audio to remote nodes via VBAN
- [ ] Receive remote audio from VBAN streams
- [ ] Handle sample rate conversion if needed

### 4.2 Multi-Node Coordination
- [ ] Use mDNS discovery to find other AudioMatrix nodes
- [ ] Implement subscription model for network streams
- [ ] Handle node connect/disconnect gracefully
- [ ] Add network latency compensation

### 4.3 Cross-PC Routing
- [ ] Route audio from PC1 hardware to PC2 virtual device
- [ ] Route audio from PC1 virtual to PC2 hardware
- [ ] Test with multiple Ableton instances across PCs

## Phase 5: Integration & Testing

### 5.1 End-to-End Testing
- [ ] Test with Ableton Live on single PC
- [ ] Test with multiple Ableton instances
- [ ] Test cross-PC routing with real DAWs
- [ ] Latency measurement and optimization

### 5.2 Stability
- [ ] Test device hot-plug scenarios
- [ ] Test network disconnect/reconnect
- [ ] Memory leak testing
- [ ] CPU usage optimization

## Validation Criteria

- Hardware ASIO device works with Dante Yamaha PCIe
- Virtual ASIO device visible and usable by Ableton
- Audio routes from Dante input to virtual output (single PC)
- Audio routes from PC1 to PC2 via network
- Latency <5ms local, <10ms network at 64-sample buffer
