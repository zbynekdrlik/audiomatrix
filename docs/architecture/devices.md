# AudioMatrix - Device Architecture

## Device Model

### Device Identification

Every audio endpoint is uniquely identified as `{computer}:{device}`:

```
STUDIO-PC:Focusrite 18i20      # Physical ASIO device
STUDIO-PC:VASIO-DAW            # Virtual ASIO device
LIVE-PC:RME Fireface           # Physical on another computer
```

### Device Types

| Type | Description | Channel Config | Visibility |
|------|-------------|----------------|------------|
| **Physical ASIO** | Hardware audio interface | Fixed by hardware | Auto-detected |
| **Virtual ASIO** | Software device for DAW routing | User-configurable (2-256 ch) | Created on demand |

## Virtual ASIO Device Management

Virtual ASIO devices can be:
- **Created** with custom name and initial channel count
- **Resized** to expand/shrink channels without recreation
- **Deleted** when no longer needed

### Device Lifecycle

1. **CREATE** (via API or config)
   - Generate unique CLSID for COM registration
   - Register in Windows Registry (HKLM\SOFTWARE\ASIO)
   - Device appears in ASIO device lists
   - DAW must rescan to see new device

2. **CONNECT** (DAW opens device)
   - DAW calls ASIOInit() on the virtual driver
   - Driver connects to AudioMatrix service via shared memory
   - Audio buffers allocated based on DAW's requested buffer size
   - ASIOStart() begins audio callback loop

3. **RESIZE** (while DAW connected)
   - Service updates channel count in shared state
   - Driver signals ASIOResetRequest to DAW
   - DAW re-queries channel count
   - No disconnect required

4. **DELETE** (device removal)
   - Disconnect all routes using this device
   - Unregister COM object
   - Remove from Windows Registry

## Platform Support

| Platform | Primary Backend | Notes |
|----------|-----------------|-------|
| **Windows** | ASIO | Primary target for professional audio |
| **Linux** | ALSA | PipeWire via ALSA compatibility |
| **macOS** | CoreAudio | Lower priority |

## Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| Device enumeration | Complete | cpal-based, all platforms |
| Physical ASIO access | Complete | Via cpal ASIO backend |
| Virtual ASIO (Rust side) | Complete | Shared memory IPC ready |
| Virtual ASIO (C++ driver) | Partial | Skeleton created, needs build integration |
| Registry integration | Not Started | Windows admin rights required |
