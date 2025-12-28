# AudioMatrix Virtual ASIO Driver

This directory contains the C++ COM DLL implementation of a virtual ASIO driver
that allows DAWs (Ableton, Cubase, Reaper, etc.) to route audio through AudioMatrix.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              DAW (Ableton, etc.)                             │
└─────────────────────────────────────────────────────────────────────────────┘
                                       │
                                       │ ASIO API calls
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                    AudioMatrix Virtual ASIO Driver (COM DLL)                 │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────────────┐  │
│  │ ASIO Interface  │  │ Buffer Manager  │  │ Shared Memory IPC           │  │
│  │ (IASIO impl)    │  │ (Ring Buffers)  │  │ (Audio + Control)           │  │
│  └─────────────────┘  └─────────────────┘  └─────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
                                       │
                                       │ Shared Memory
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         AudioMatrix Service (Rust)                           │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────────────┐  │
│  │ Virtual Device  │  │ Audio Router    │  │ VBAN/Network               │  │
│  │ Manager         │  │                 │  │                             │  │
│  └─────────────────┘  └─────────────────┘  └─────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Components

### virtual_asio.h / virtual_asio.cpp
Main ASIO driver implementation:
- Implements IASIO interface from Steinberg ASIO SDK
- Handles ASIOInit, ASIOStart, ASIOStop, ASIOGetBufferSize, etc.
- Manages audio callback thread

### shared_memory.h / shared_memory.cpp
Inter-process communication with AudioMatrix:
- Creates/opens shared memory regions for audio data
- Lock-free ring buffers for real-time audio transfer
- Control channel for configuration (sample rate, buffer size, channels)

### registry.cpp
Windows registry integration:
- Registers the COM DLL as an ASIO driver
- Stores configuration in HKLM\SOFTWARE\ASIO\AudioMatrix

## Building

### Requirements
- Visual Studio 2022 or later
- CMake 3.20+
- ASIO SDK (GPLv3 from https://github.com/audiosdk/asio)
- Windows SDK

### Build Steps
```powershell
# Clone ASIO SDK
git clone https://github.com/audiosdk/asio.git external/asio_sdk

# Configure
cmake -B build -G "Visual Studio 17 2022" -A x64

# Build
cmake --build build --config Release

# Register driver (requires admin)
regsvr32 build/Release/AudioMatrixASIO.dll
```

## Integration with Rust

The Rust side (`ram-asio` crate) provides:
- `VirtualDevice` struct for managing virtual ASIO instances
- Shared memory client for receiving/sending audio
- Configuration via API (`POST /api/devices/virtual`)

### Shared Memory Layout

```
┌────────────────────────────────────────────────────────────────┐
│ Header (64 bytes)                                               │
│ ┌──────────────┬──────────────┬──────────────┬───────────────┐ │
│ │ Magic (8B)   │ Version (4B) │ Channels (4B)│ SampleRate(4B)│ │
│ ├──────────────┼──────────────┼──────────────┼───────────────┤ │
│ │ BufferSize   │ ReadPos (8B) │ WritePos (8B)│ Flags (8B)    │ │
│ └──────────────┴──────────────┴──────────────┴───────────────┘ │
├────────────────────────────────────────────────────────────────┤
│ Input Ring Buffer (configurable size)                          │
│ ┌────────────────────────────────────────────────────────────┐ │
│ │ Audio samples (f32 interleaved)                            │ │
│ └────────────────────────────────────────────────────────────┘ │
├────────────────────────────────────────────────────────────────┤
│ Output Ring Buffer (configurable size)                         │
│ ┌────────────────────────────────────────────────────────────┐ │
│ │ Audio samples (f32 interleaved)                            │ │
│ └────────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────┘
```

## License

This driver uses the ASIO SDK under GPLv3 license.
The AudioMatrix project is also licensed under compatible terms.
