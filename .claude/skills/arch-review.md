# Architecture Review Skill

## Description
Periodic check to ensure code aligns with architecture documentation and vice versa.

## When to Use
- After significant implementation work
- Before creating PRs for architectural changes
- When unsure if implementation matches documented design
- Periodically (weekly recommended)

## Review Process

### 1. Check Implementation Status

Read `ARCHITECTURE.md` and compare the Implementation Status table against actual crate contents:

```
crates/
├── ram-core/     - Check: routing.rs, engine.rs, device.rs, connection.rs
├── ram-asio/     - Check: host.rs, device.rs, stream.rs, virtual_device.rs
├── ram-vban/     - Check: protocol.rs, sender.rs, receiver.rs, jitter.rs
├── ram-discovery/- Check: announce.rs, browse.rs
├── ram-api/      - Check: handlers.rs, websocket.rs
└── ram-service/  - Check: service.rs, config.rs
```

For each component in the status table:
- If marked "Complete" but code is incomplete: Update to "Partial"
- If marked "Not Started" but code exists: Update to appropriate status
- If code is fully working with tests: Ensure marked "Complete"

### 2. Check Architecture Documents

Review each file in `docs/architecture/`:
- `devices.md` - Does device model match ram-asio and ram-core/device.rs?
- `routing.md` - Does routing model match ram-core/routing.rs and engine.rs?
- `network.md` - Does VBAN description match ram-vban implementation?
- `threading.md` - Does thread model match actual thread usage?
- `state.md` - Does persistence match ram-core/persistence.rs?
- `api.md` - Do endpoints match ram-api/handlers.rs?

### 3. Identify Gaps

Create lists:
1. **Documented but not implemented** - Architecture describes feature that doesn't exist in code
2. **Implemented but not documented** - Code exists that isn't in architecture docs
3. **Mismatched** - Both exist but don't align

### 4. Output Report

Format findings as:

```
## Architecture Review Report

### Date: YYYY-MM-DD

### Status Updates Needed
| Component | Current Status | Should Be | Reason |
|-----------|---------------|-----------|--------|
| ... | ... | ... | ... |

### Documentation Gaps
- [ ] Item needing documentation

### Code Gaps
- [ ] Item needing implementation

### Recommendations
1. ...
2. ...
```

### 5. Apply Fixes

After review:
1. Update ARCHITECTURE.md status table
2. Update detailed docs in docs/architecture/
3. Add TODO comments in code for missing implementations
4. Commit changes with message: `docs: architecture review sync YYYY-MM-DD`

## Example Usage

User: `/arch-review`

Assistant should:
1. Read ARCHITECTURE.md
2. Scan key implementation files
3. Compare status table to reality
4. Generate report
5. Offer to apply fixes
