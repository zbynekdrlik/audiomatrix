# Implementation Tasks

## 1. Core Audio Engine (ram-core)

- [ ] 1.1 Implement AtomicF32 wrapper for lock-free gain control
- [ ] 1.2 Implement AudioRingBuffer with CachePadded positions
- [ ] 1.3 Implement SourceConnection with gain/mute/enabled atomics
- [ ] 1.4 Implement DestinationChannel with N:1 mixing
- [ ] 1.5 Implement HeadroomMode (Clip, AutoGain, Limiter, Manual)
- [ ] 1.6 Implement routing matrix with RwLock-protected routing table
- [ ] 1.7 Add resampler integration using rubato

## 2. VBAN Transport (ram-vban)

- [ ] 2.1 Implement VBAN header parsing (28-byte format)
- [ ] 2.2 Implement VbanSender with packet pool pre-allocation
- [ ] 2.3 Implement VbanReceiver with adaptive jitter buffer
- [ ] 2.4 Implement sequence number tracking and packet reordering
- [ ] 2.5 Implement stream lifecycle management

## 3. Device Management (ram-asio on Windows, ram-device on Linux)

- [ ] 3.1 Enumerate physical audio devices via cpal
- [ ] 3.2 Implement device state machine (starting, running, error)
- [ ] 3.3 Implement device cache with TTL
- [ ] 3.4 Implement hot-plug detection
- [ ] 3.5 (Windows) Implement virtual ASIO COM registration
- [ ] 3.6 (Windows) Implement virtual ASIO live resize via ASIOResetRequest

## 4. Service Discovery (ram-discovery)

- [ ] 4.1 Implement mDNS service announcement (_audiomatrix._tcp.local)
- [ ] 4.2 Implement mDNS service browsing
- [ ] 4.3 Implement heartbeat protocol (AMHB packets)
- [ ] 4.4 Implement computer cache persistence
- [ ] 4.5 Implement SUBSCRIBE/UNSUBSCRIBE message handling

## 5. API Layer (ram-api)

- [ ] 5.1 Implement REST endpoints for devices CRUD
- [ ] 5.2 Implement REST endpoints for connections CRUD
- [ ] 5.3 Implement batch operations endpoint
- [ ] 5.4 Implement WebSocket connection with topic subscription
- [ ] 5.5 Implement real-time event broadcasting
- [ ] 5.6 Implement meters endpoint with level data

## 6. State Management

- [ ] 6.1 Implement subscriptions.toml persistence
- [ ] 6.2 Implement connection state machine (intent→requested→pending→active)
- [ ] 6.3 Implement offline configuration (pending subscriptions)
- [ ] 6.4 Implement preset save/load with distribution logic

## 7. Security

- [ ] 7.1 Implement authentication modes (none, pin, token, full)
- [ ] 7.2 Implement WebSocket authentication via query parameter
- [ ] 7.3 Implement permission enforcement on API endpoints

## 8. Service Executable (ram-service)

- [ ] 8.1 Implement main coordinator orchestrating all components
- [ ] 8.2 Implement Windows service integration
- [ ] 8.3 Implement startup sequence with subscription recovery
- [ ] 8.4 Implement graceful shutdown with mDNS de-announcement
