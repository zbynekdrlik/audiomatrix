---
name: Architecture Review
description: Deep review comparing ARCHITECTURE.md against actual implementation
category: project
tags: [architecture, review, alignment]
---

**Purpose**
Periodic check to ensure code aligns with architecture documentation and vice versa.
Run this after significant implementation work, before PRs, or weekly.

**Steps**

1. Read `ARCHITECTURE.md` Implementation Status table.

2. Scan the actual crate structure to verify each component's status:
   ```
   crates/ram-core/src/*.rs    # Core audio engine
   crates/ram-api/src/*.rs     # REST/WebSocket API
   crates/ram-service/src/*.rs # Main binary
   crates/ram-vban/src/*.rs    # VBAN protocol
   crates/ram-discovery/src/*.rs # mDNS
   ```

3. **Critical Wiring Check** - Verify API actually controls audio:
   | API Operation | Should Call | Check |
   |---------------|-------------|-------|
   | POST /routes | AudioProcessor.add_route() | state.rs |
   | DELETE /routes/:id | AudioProcessor.remove_route() | state.rs |
   | GET /streams | StreamRegistry.all() | state.rs |
   | GET /subscriptions | SubscriptionManager.all() | state.rs |

4. Find TODOs indicating missing wiring:
   ```bash
   grep -r "TODO" crates/ram-api/src/
   ```

5. Check file sizes (max 1000 lines):
   ```bash
   wc -l crates/*/src/*.rs | sort -n | tail -10
   ```

6. For each discrepancy found:
   - If code is truth: Update ARCHITECTURE.md status
   - If architecture is truth: Note as technical debt

7. Update ARCHITECTURE.md:
   - Fix Implementation Status table
   - Update "Known Technical Debt" section
   - Update "Next Steps" priorities
   - Update "Recently Completed" if applicable

8. Verify changes don't break build:
   ```bash
   cargo test --workspace
   ```

9. Output structured report with:
   - Status verification results
   - Critical wiring issues
   - File size violations
   - Recommendations

**Reference**
- See `.claude/skills/arch-review.md` for detailed review process
- ARCHITECTURE.md is single source of truth for design
- Code is truth for what IS implemented
