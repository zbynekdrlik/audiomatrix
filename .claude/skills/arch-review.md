---
name: arch-review
description: Deep review of ARCHITECTURE.md vs actual implementation, update docs to match reality
---

# Architecture Review Skill

## Description
Periodic check to ensure code aligns with architecture documentation and vice versa.
This is critical for maintaining consistency between design and implementation.

## When to Use
- After significant implementation work
- Before creating PRs for architectural changes
- When unsure if implementation matches documented design
- Periodically (weekly recommended)
- At the start of new development sessions

## Review Process

### 1. Check Implementation Status

Read `ARCHITECTURE.md` and compare the Implementation Status table against actual crate contents:

```
crates/
├── ram-core/           # Core audio engine (~5000+ LOC)
│   ├── buffer.rs           # SPSC ring buffer
│   ├── ring_buffer_pool.rs # Pre-allocated buffer pool (256 x 2048)
│   ├── routing_table.rs    # RCU routing table
│   ├── routing_snapshot.rs # Immutable routing snapshots
│   ├── callbacks.rs        # Lock-free audio callbacks (with metering)
│   ├── metering.rs         # Lock-free level meters (peak/RMS)
│   ├── active_stream.rs    # Stream types and stats
│   ├── stream_registry.rs  # Stream lifecycle management
│   ├── connection.rs       # Connection state machine
│   ├── device.rs           # Device enumeration
│   ├── resampler.rs        # Sample rate conversion (Rubato)
│   ├── subscription.rs     # Cross-node subscription protocol
│   ├── subscription_manager.rs # Subscription lifecycle management
│   ├── latency.rs          # Latency calculation and tracking
│   ├── engine.rs           # Audio engine coordinator
│   ├── atomic.rs           # AtomicF32 for lock-free gain
│   ├── destination.rs      # Output channel mixing
│   ├── mixer.rs            # N:1 audio mixing
│   ├── persistence.rs      # Config persistence (JSON)
│   └── error.rs            # Error types
├── ram-asio/           # Windows ASIO support
│   ├── cpp/                # C++ COM driver
│   └── *.rs                # Rust ASIO bindings
├── ram-vban/           # Network audio (~3000 LOC)
│   ├── protocol.rs         # VBAN packet format (28-byte header)
│   ├── jitter.rs           # Adaptive jitter buffer
│   ├── sender.rs           # UDP sender
│   ├── receiver.rs         # UDP receiver
│   ├── stream.rs           # Stream management
│   └── pool.rs             # Packet memory pooling
├── ram-discovery/      # Service discovery (~950 LOC)
│   ├── announce.rs         # mDNS announcer
│   └── browse.rs           # mDNS browser
├── ram-api/            # REST/WebSocket API (~1500 LOC)
│   ├── router.rs           # Axum route definitions
│   ├── handlers.rs         # Route handlers
│   ├── websocket.rs        # WS event broadcast
│   ├── state.rs            # Shared app state (3 TODOs!)
│   ├── models.rs           # API request/response types
│   ├── auth.rs             # Authentication (stub)
│   └── error.rs            # Error types
└── ram-service/        # Main binary (~2100 LOC)
    ├── main.rs             # Entry point, CLI args
    ├── service.rs          # Service lifecycle
    ├── config.rs           # Configuration loading
    └── audio_processor.rs  # Audio coordinator (1160 lines!)
```

For each component in the status table:
- If marked "Complete" but code is incomplete: Update to "Partial"
- If marked "Not Started" but code exists: Update to appropriate status
- If code is fully working with tests: Ensure marked "Complete"

### 2. Critical Wiring Check (API ↔ AudioProcessor)

**This is the most common issue!** Verify these connections work end-to-end:

| API Operation | Should Call | Currently Status |
|---------------|-------------|------------------|
| POST /routes | AudioProcessor.add_route() | CHECK state.rs |
| DELETE /routes/:id | AudioProcessor.remove_route() | CHECK state.rs |
| GET /streams | StreamRegistry.all() | CHECK state.rs - should NOT return empty |
| GET /subscriptions | SubscriptionManager.all() | CHECK state.rs - should NOT return empty |

Look for these TODO patterns in ram-api/state.rs:
```rust
// TODO: Wire to actual AudioProcessor
// TODO: Wire to actual SubscriptionManager
```

### 3. Check Key Architectural Patterns

Verify these patterns are correctly implemented:

| Pattern | Files | What to Check |
|---------|-------|---------------|
| **RCU (Read-Copy-Update)** | `routing_table.rs` | Lock-free reads via atomic snapshot swapping |
| **SPSC Ring Buffers** | `buffer.rs`, `ring_buffer_pool.rs` | Single-producer single-consumer, no locks |
| **Lock-free Callbacks** | `callbacks.rs` | No mutexes, no allocations in audio path |
| **Atomic Controls** | `routing_snapshot.rs` | `AtomicF32`/`AtomicBool` for gain/mute |
| **Pre-allocation** | `ring_buffer_pool.rs` | Buffers created at startup (256 x 2048) |
| **Metering in Callbacks** | `callbacks.rs`, `metering.rs` | MeterBank updated in audio callback |
| **Subscription Protocol** | `subscription.rs` | Message types match ARCHITECTURE.md |
| **Event Broadcasting** | `websocket.rs` | Tokio broadcast for async event distribution |

### 4. File Size Check

Per CLAUDE.md, max 1000 lines per file. Check for violations:
```bash
wc -l crates/*/src/*.rs | sort -n | tail -10
```

Known violators to track:
- `audio_processor.rs` - Should be split if > 1000 lines

### 5. Check Architecture Documents

Review each file in `docs/architecture/`:
- `devices.md` - Does device model match ram-core/device.rs?
- `routing.md` - Does routing model match routing_table.rs and routing_snapshot.rs?
- `network.md` - Does VBAN description match ram-vban implementation?
- `threading.md` - Does thread model match actual thread usage?
- `state.md` - Does persistence match ram-core/persistence.rs?
- `api.md` - Do endpoints match ram-api/handlers.rs?

### 6. Identify Gaps

Create three lists:
1. **Documented but not implemented** - Architecture describes feature that doesn't exist in code
2. **Implemented but not documented** - Code exists that isn't in architecture docs
3. **Mismatched** - Both exist but don't align

### 7. Output Report

Format findings as:

```markdown
## Architecture Review Report

### Date: YYYY-MM-DD

### Implementation Status Verification
| Component | Doc Status | Actual Status | Action |
|-----------|-----------|---------------|--------|
| Audio Routing Matrix | Complete | Complete | None |
| REST API | Complete | Partial | Update doc - not wired to AudioProcessor |
| ... | ... | ... | ... |

### Critical Wiring Issues
- [ ] Routes not applying to AudioProcessor
- [ ] Streams endpoint returns empty
- [ ] Subscriptions not reading from manager

### New Untracked Components
- List modules not in ARCHITECTURE.md

### Stale Documentation
- List documented features that don't match code

### File Size Violations
- audio_processor.rs: XXXX lines (limit: 1000)

### Architectural Pattern Compliance
| Pattern | Status | Issues |
|---------|--------|--------|
| RCU | OK | None |
| Lock-free callbacks | OK | None |
| API ↔ AudioProcessor | FAIL | 3 TODOs in state.rs |

### Recommendations
1. ARCHITECTURE.md changes needed
2. Code changes to match architecture
3. New documentation needed

### Next Steps Priority
1. (From ARCHITECTURE.md "Next Steps" section)
2. (Any new urgent items found)
```

### 8. Apply Fixes

After review:
1. Update ARCHITECTURE.md status table (code is truth for what IS)
2. Update "Recently Completed" section with new completions
3. Update "Next Steps" section if priorities changed
4. Update "Known Technical Debt" section
5. Update detailed docs in docs/architecture/
6. Add TODO comments in code for missing implementations

### 9. Verify After Updates

```bash
cargo test --workspace
cargo build -p ram-service
cargo clippy --workspace
```

## Example Usage

User: `/arch-review`

Assistant should:
1. Read ARCHITECTURE.md
2. Use Task tool with Explore agent to scan implementation
3. Compare status table to reality
4. Check API ↔ AudioProcessor wiring (most common issue!)
5. Check file size limits
6. Generate structured report
7. Update ARCHITECTURE.md if discrepancies found
8. Update "Known Technical Debt" section
9. Offer to create issues for gaps

## Quick Check Commands

```bash
# Count lines in largest files
wc -l crates/*/src/*.rs | sort -n | tail -10

# Find TODO markers
grep -r "TODO" crates/ram-api/src/

# Check for allow(dead_code)
grep -r "allow(dead_code)" crates/

# Run tests
cargo test --workspace

# Check for clippy warnings
cargo clippy --workspace -- -D warnings
```
