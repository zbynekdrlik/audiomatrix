# Proposal: Implement Windows ASIO Core

## Summary

Implement the core Windows ASIO functionality that enables AudioMatrix to:
1. Connect to hardware ASIO devices (Dante, RME, Focusrite, etc.)
2. Create multiple virtual ASIO devices for DAW connections
3. Route audio between physical and virtual ASIO devices
4. Route audio over network between multiple Windows PCs

## Motivation

AudioMatrix's primary use case is professional audio production on Windows where:
- DAWs (Ableton, Pro Tools, etc.) require ASIO for low-latency audio
- Multiple DAW instances need separate virtual ASIO devices
- Audio must route between hardware interfaces and virtual devices
- Audio must route between multiple production PCs over LAN

Without ASIO support, AudioMatrix cannot serve its core purpose.

## Scope

### In Scope
- Hardware ASIO device enumeration and connection
- Virtual ASIO driver creation (Windows kernel driver or user-mode)
- Real-time audio routing between ASIO devices
- VBAN network transport integration
- Multi-PC audio routing

### Out of Scope (this proposal)
- macOS CoreAudio support
- Linux ALSA/JACK improvements
- Web UI implementation
- Advanced DSP processing

## Technical Approach

### Option A: cpal + Custom Virtual ASIO Driver
- Use cpal with `asio` feature for hardware ASIO access
- Create custom virtual ASIO driver (like VB-Audio approach)
- Pros: Leverages existing cpal infrastructure
- Cons: Requires kernel driver development for virtual devices

### Option B: Direct ASIO SDK Integration
- Bypass cpal, integrate ASIO SDK directly in ram-asio
- Build virtual ASIO driver from scratch
- Pros: Full control, optimized for our use case
- Cons: More development work, no cross-platform code reuse

### Recommended: Option A with Virtual Driver Investigation
Start with cpal ASIO for hardware access while investigating virtual driver approaches (WDM, virtual audio cable patterns).

## Dependencies

- Steinberg ASIO SDK (required for any ASIO work)
- Windows Driver Kit (WDK) if kernel-mode virtual driver needed
- Existing ram-core routing infrastructure

## Success Criteria

1. Enumerate and connect to hardware ASIO devices on Windows
2. Create at least one virtual ASIO device visible to DAWs
3. Route audio from hardware ASIO input to virtual ASIO output
4. Route audio between two Windows PCs over VBAN
5. Achieve <5ms latency for local routing at 64-sample buffer
