<!-- OPENSPEC:START -->
# OpenSpec Instructions

These instructions are for AI assistants working in this project.

Always open `@/openspec/AGENTS.md` when the request:
- Mentions planning or proposals (words like proposal, spec, change, plan)
- Introduces new capabilities, breaking changes, architecture shifts, or big performance/security work
- Sounds ambiguous and you need the authoritative spec before coding

Use `@/openspec/AGENTS.md` to learn:
- How to create and apply change proposals
- Spec format and conventions
- Project structure and guidelines

Keep this managed block so 'openspec update' can refresh the instructions.

<!-- OPENSPEC:END -->

# AudioMatrix - Claude Code Project Guidelines

## Project Identity

**Name:** AudioMatrix
**Type:** Professional audio routing system (Dante-like)
**Language:** Rust (2021 edition, MSRV 1.75+)
**Architecture:** See `ARCHITECTURE.md`

## Development Philosophy

### Development Phase Flexibility

**We are in active development phase.** You are free to:
- Redesign components and APIs
- Change architecture patterns
- Fully rework code sections
- Refactor for better patterns
- Update ARCHITECTURE.md to reflect changes

This flexibility allows rapid iteration toward the best solution.

### Core Principles

1. **Architecture-First**: `ARCHITECTURE.md` is the single source of truth. Code follows architecture, never the reverse.
2. **Architecture-Always-Updated**: When design or architectural changes are discovered during development, `ARCHITECTURE.md` MUST be updated immediately to reflect the change. Never let implementation diverge from documentation.
3. **Test-Driven Development (TDD)**: Write tests before implementation. No PR merges without tests.
4. **Audio-Grade Quality**: Lock-free audio paths, zero allocations in hot paths, deterministic latency.
5. **SOTA 2025 Rust**: Use modern idioms, async/await, zero-cost abstractions, compile-time guarantees.

### Senior Developer Mindset

- **Think before coding**: Understand the problem deeply before writing a single line
- **Design for change**: Anticipate future requirements, use trait-based abstractions
- **Fail fast, fail loud**: Errors should be caught early and be informative
- **Measure everything**: Performance claims must be backed by benchmarks

### Audio Expert Considerations

- **Latency is king**: Every microsecond matters in real-time audio
- **Lock-free or die**: Mutexes in audio callbacks cause glitches
- **Buffer management**: Pre-allocate everything, use ring buffers
- **Sample rate handling**: Always consider resampling implications
- **Thread priority**: Audio threads need real-time scheduling

---

## OpenSpec Workflow

This project uses **OpenSpec** for spec-driven development. All significant changes go through a proposal process.

### When to Create a Proposal

Create a change proposal (`openspec/changes/<change-id>/`) for:
- New audio routing features
- API endpoint additions/modifications
- VBAN protocol changes
- WebSocket event schema changes
- Architecture pattern changes
- Breaking changes of any kind

### When to Skip Proposals

Proceed directly with code for:
- Bug fixes restoring documented behavior
- Test additions for existing functionality
- Documentation updates
- Dependency updates (non-breaking)
- Internal refactoring without behavior change

### Proposal Workflow

```bash
# 1. Check existing specs and active changes
openspec list --specs
openspec list

# 2. Create proposal structure
mkdir -p openspec/changes/add-feature-name/specs/capability-name
# Create: proposal.md, tasks.md, specs/capability/spec.md

# 3. Write spec deltas with ADDED/MODIFIED/REMOVED sections
# Each requirement needs at least one #### Scenario:

# 4. Validate before requesting approval
openspec validate add-feature-name --strict

# 5. Request approval before implementation
# 6. Implement per tasks.md
# 7. Archive after deployment
openspec archive add-feature-name --yes
```

### Key Files

- `openspec/project.md` - Project context and conventions
- `openspec/AGENTS.md` - Full OpenSpec instructions
- `openspec/specs/` - Current truth (what IS built)
- `openspec/changes/` - Proposals (what SHOULD change)

---

## Keeping ARCHITECTURE.md Up-to-Date

`ARCHITECTURE.md` is the **single source of truth** for system design. It must always reflect reality.

### When to Update ARCHITECTURE.md

Update immediately when:
- Discovering a design flaw that requires a different approach
- Implementation reveals a better pattern than documented
- Adding new components, APIs, or data structures
- Changing thread models, synchronization, or data flow
- Modifying network protocols or message formats
- Updating configuration file schemas
- Finding edge cases that change expected behavior

### Update Process

1. **Before coding the change**: Update ARCHITECTURE.md with the new design
2. **Create OpenSpec proposal if significant**: Major changes need spec deltas
3. **Reference the section**: Note which ARCHITECTURE.md section was updated in commit message
4. **Keep diagrams current**: ASCII diagrams and tables must match implementation

### What NOT to Change Without Discussion

These sections require team review before modification:
- Core threading model (ASIO callback → Router → Network)
- Destination-owned subscription model
- VBAN packet format (protocol compatibility)
- Connection ID format
- API versioning strategy

### Consistency Checks

During code review, verify:
- [ ] New structs/enums match ARCHITECTURE.md data models
- [ ] API endpoints match documented routes
- [ ] Config file fields match documented schema
- [ ] Error handling follows documented patterns

---

## Code Organization

### File Size Limits

**Maximum 1000 lines per file.** Split larger files by:
- Extracting types to separate modules
- Moving implementations to dedicated files
- Using feature-based module organization

### Module Structure

```
crates/{crate_name}/
├── src/
│   ├── lib.rs              # Public API, re-exports only (< 100 lines)
│   ├── error.rs            # Error types for this crate
│   ├── types.rs            # Shared types and traits
│   ├── {feature}/
│   │   ├── mod.rs          # Feature module (< 200 lines)
│   │   ├── {component}.rs  # Individual components (< 500 lines)
│   │   └── tests.rs        # Feature tests
│   └── tests/              # Integration tests
├── benches/                # Benchmarks (criterion)
└── Cargo.toml
```

### Naming Conventions

```rust
// Types: PascalCase
pub struct AudioBuffer { }
pub trait StreamProcessor { }
pub enum ConnectionState { }

// Functions/methods: snake_case
fn process_audio_block() { }
impl AudioBuffer {
    pub fn read_samples(&self) -> &[f32] { }
}

// Constants: SCREAMING_SNAKE_CASE
const MAX_CHANNELS: usize = 256;
const BUFFER_SIZE_MS: f32 = 1.33;

// Modules: snake_case
mod audio_buffer;
mod vban_protocol;
```

---

## Testing Strategy

### Test Pyramid

```
         /\
        /  \     E2E Tests (10%)
       /    \    - Full system integration
      /      \   - Real audio device tests
     /--------\
    /          \ Integration Tests (30%)
   /            \ - Cross-crate communication
  /              \ - Network protocol tests
 /----------------\
/                  \ Unit Tests (60%)
                    - Pure function tests
                    - Component isolation
```

### Test Requirements

1. **Every PR must have tests** - No exceptions
2. **Coverage target: 80%+** - Enforced by CI
3. **Audio path tests**: Must verify lock-free behavior
4. **Benchmark regressions**: CI fails on performance degradation

### Test File Organization

```rust
// src/buffer/tests.rs - Unit tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_write_read_roundtrip() {
        // Arrange
        let buffer = RingBuffer::new(1024);
        let samples = [0.5f32; 64];

        // Act
        let written = buffer.write(&samples);
        let mut output = [0.0f32; 64];
        let read = buffer.read(&mut output);

        // Assert
        assert_eq!(written, 64);
        assert_eq!(read, 64);
        assert_eq!(output, samples);
    }
}

// tests/integration/vban_roundtrip.rs - Integration test
#[tokio::test]
async fn vban_sender_receiver_roundtrip() {
    // Full VBAN send/receive cycle
}

// tests/e2e/full_routing.rs - E2E test
#[tokio::test]
#[ignore] // Requires real audio hardware
async fn route_audio_between_virtual_devices() {
    // Full system test with virtual ASIO
}
```

### Property-Based Testing

Use `proptest` for complex invariants:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn ring_buffer_never_loses_samples(
        samples in prop::collection::vec(any::<f32>(), 0..1024)
    ) {
        let buffer = RingBuffer::new(2048);
        buffer.write(&samples);
        let mut output = vec![0.0; samples.len()];
        let read = buffer.read(&mut output);
        prop_assert_eq!(read, samples.len());
    }
}
```

### Miri for Unsafe Code

```bash
# Run under Miri for undefined behavior detection
cargo +nightly miri test -p ram-core
```

---

## GitHub Workflow

### Branch Strategy

```
main (protected)
├── develop (integration)
│   ├── feature/xxx-description
│   ├── fix/xxx-description
│   └── refactor/xxx-description
└── release/v1.x.x
```

### Git Workflow Rules (CRITICAL)

**ALWAYS commit and push your work:**
- Commit frequently with meaningful messages
- Push to remote after every significant change (backup + visibility)
- NEVER skip pushing - remote serves as backup and enables CI validation

**NEVER push directly to `main`:**
- All work happens on `develop` or feature branches
- Create PRs from `develop` → `main`
- Only the user merges PRs via GitHub web interface
- Claude creates PRs but does NOT merge them

**Monitor GitHub Actions:**
- After every push, verify CI pipeline succeeds
- If CI fails, fix issues immediately before continuing
- Never leave the repository in a broken state

**Keep repository clean:**
- Every file in git MUST have a purpose for current implementation
- NO temporary files, experiments, or obsolete code
- NO archiving old versions in git - use git history instead
- Delete files that are no longer needed
- If unsure whether to keep a file, ask the user

### Branch Protection Rules (main)

- **Require PR reviews**: Minimum 1 approval
- **Require status checks**:
  - `test` (all platforms)
  - `lint` (clippy, fmt)
  - `coverage` (>= 80%)
  - `audit` (security vulnerabilities)
  - `benchmark` (no regression)
- **Require linear history**: Squash or rebase only
- **No direct pushes**: All changes via PR

### Versioning Rules (STRICT)

**Single Source of Truth:** Version is defined ONLY in the root `Cargo.toml` workspace section. All crates inherit via `version.workspace = true`.

**Develop Branch (MANDATORY):**
- Version MUST be in format: `X.Y.Z-dev.N` (e.g., `0.1.0-dev.1`, `0.2.0-dev.5`)
- The `-dev.N` suffix is REQUIRED - CI will REJECT versions without it
- Increment `N` for each significant change (features, fixes)
- NEVER have a release version (without `-dev`) on develop branch

**Main Branch (RELEASES ONLY):**
- Version MUST be in format: `X.Y.Z` (e.g., `0.1.0`, `1.0.0`)
- NO `-dev` suffix allowed - CI will REJECT dev versions
- Version must match the release tag exactly

**Release Process:**
1. On develop: Version is `X.Y.Z-dev.N`
2. Create PR from develop → main
3. In PR: Update version from `X.Y.Z-dev.N` → `X.Y.Z`
4. Merge PR to main
5. Tag main with `vX.Y.Z`
6. Release workflow builds and publishes binaries + IRM installer
7. Immediately after: On develop, bump to next dev version (e.g., `0.2.0-dev.1`)

**Version Bump Examples:**
```bash
# Current: 0.1.0-dev.3 → Adding a feature
# New: 0.1.0-dev.4

# Current: 0.1.0-dev.15 → Ready for release
# PR to main: 0.1.0
# After release on develop: 0.2.0-dev.1
```

**CI Enforcement:**
- `version-check` job validates version format per branch
- Develop push with `0.1.0` (no -dev) → CI FAILS
- Main push with `0.1.0-dev.1` → CI FAILS
- Release tag `v0.1.0-dev.1` → Release workflow REJECTS

### IRM Installer (Windows)

Releases include a PowerShell installer script that displays version on startup:
```powershell
irm https://github.com/zbynekdrlik/audiomatrix/releases/latest/download/install.ps1 | iex
```

The installer:
1. Prints `AudioMatrix Installer vX.Y.Z` on start
2. Downloads the correct version binary
3. Installs to `$env:LOCALAPPDATA\AudioMatrix`
4. Adds to PATH

### PR Template

```markdown
## Summary
<!-- Brief description of changes -->

## Type
- [ ] Feature
- [ ] Bug fix
- [ ] Refactor
- [ ] Documentation
- [ ] CI/CD

## Testing
- [ ] Unit tests added/updated
- [ ] Integration tests added/updated
- [ ] Manual testing performed

## Checklist
- [ ] Code follows project style guidelines
- [ ] No files exceed 1000 lines
- [ ] All tests pass locally
- [ ] Documentation updated
- [ ] CHANGELOG updated (if applicable)

## Related Issues
Closes #XXX
```

### Commit Message Format

```
<type>(<scope>): <subject>

<body>

<footer>
```

Types: `feat`, `fix`, `refactor`, `test`, `docs`, `ci`, `perf`, `chore`

Example:
```
feat(vban): implement adaptive jitter buffer

- Add dynamic buffer sizing based on network conditions
- Implement packet reordering within jitter window
- Add metrics for buffer underrun/overrun tracking

Closes #42
```

---

## Versioning and Release Management

### Single Version Source

The project version is defined **ONLY** in the workspace `Cargo.toml`:

```toml
[workspace.package]
version = "0.1.0"  # or "0.1.0-dev.1" on develop
```

All crates inherit this version:

```toml
[package]
version.workspace = true
```

**NEVER** hardcode versions in individual crate Cargo.toml files.

### Version Format Rules (STRICT)

| Branch | Version Format | Example |
|--------|---------------|---------|
| `develop` | `X.Y.Z-dev.N` | `0.1.0-dev.1`, `0.1.0-dev.2` |
| `main` | `X.Y.Z` | `0.1.0`, `0.2.0`, `1.0.0` |
| `release/*` | `X.Y.Z` | `0.1.0` |

**CI Enforcement:**
- Push to `develop` with non-dev version → **CI FAILS**
- Push to `main` with dev version → **CI FAILS**
- PR from `develop` to `main` must bump version to release format
- Version must increment (never go backwards)

### Dev Branch Workflow

```bash
# On develop, version must be -dev.N
# After each significant change, increment dev number:
# 0.1.0-dev.1 → 0.1.0-dev.2 → 0.1.0-dev.3

# When ready for release:
# 1. Create PR from develop → main
# 2. In PR, update version: 0.1.0-dev.5 → 0.1.0
# 3. After merge, tag: git tag v0.1.0
# 4. CI creates release with binaries
# 5. Back on develop, bump to next dev: 0.2.0-dev.1
```

### Release Artifacts

Each GitHub Release includes:
- `audiomatrix-x.y.z-windows-x64.exe` - Windows installer
- `audiomatrix-x.y.z-windows-x64.zip` - Windows portable
- `audiomatrix-x.y.z-linux-x64.tar.gz` - Linux binary
- `audiomatrix-x.y.z-macos-x64.tar.gz` - macOS binary
- `audiomatrix-x.y.z-macos-arm64.tar.gz` - macOS Apple Silicon
- `SHA256SUMS.txt` - Checksums for verification

### Windows Installation (irm)

Install on Windows using PowerShell:

```powershell
irm https://raw.githubusercontent.com/zbynekdrlik/audiomatrix/main/scripts/install.ps1 | iex
```

The installer script:
1. **Always prints version** at start
2. Downloads latest release binary
3. Verifies SHA256 checksum
4. Installs to `%LOCALAPPDATA%\AudioMatrix`
5. Adds to PATH
6. Creates Start Menu shortcut

### Version Validation in CI

```yaml
# .github/workflows/ci.yml - version check job
version-check:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - name: Check version format
      run: |
        VERSION=$(grep -m1 'version = ' Cargo.toml | cut -d'"' -f2)
        BRANCH="${GITHUB_REF#refs/heads/}"

        if [[ "$BRANCH" == "develop" ]]; then
          if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+-dev\.[0-9]+$ ]]; then
            echo "::error::Develop branch must have -dev.N version, got: $VERSION"
            exit 1
          fi
        elif [[ "$BRANCH" == "main" ]]; then
          if [[ "$VERSION" =~ -dev ]]; then
            echo "::error::Main branch cannot have dev version, got: $VERSION"
            exit 1
          fi
        fi
        echo "Version $VERSION is valid for branch $BRANCH"
```

---

## CI/CD Pipeline

### GitHub Actions Workflow

```yaml
# .github/workflows/ci.yml
name: CI

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main, develop]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  # Format check - fast fail
  fmt:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all -- --check

  # Clippy lints
  clippy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --all-targets --all-features -- -D warnings -D clippy::pedantic

  # Tests on all platforms
  test:
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --all-features --workspace

  # Coverage enforcement
  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: taiki-e/install-action@cargo-llvm-cov
      - run: cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info
      - uses: codecov/codecov-action@v4
        with:
          files: lcov.info
          fail_ci_if_error: true
      - name: Check coverage threshold
        run: |
          COVERAGE=$(cargo llvm-cov --all-features --workspace --json | jq '.data[0].totals.lines.percent')
          if (( $(echo "$COVERAGE < 80" | bc -l) )); then
            echo "Coverage $COVERAGE% is below 80% threshold"
            exit 1
          fi

  # Security audit
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: rustsec/audit-check@v1
        with:
          token: ${{ secrets.GITHUB_TOKEN }}

  # Benchmark regression check
  benchmark:
    runs-on: ubuntu-latest
    if: github.event_name == 'pull_request'
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Run benchmarks
        run: cargo bench --workspace -- --save-baseline pr
      - name: Compare with main
        run: |
          git fetch origin main
          git checkout origin/main
          cargo bench --workspace -- --save-baseline main
          git checkout -
          cargo bench --workspace -- --baseline main --load-baseline pr
          # Fail if any benchmark regressed by more than 10%

  # Miri for unsafe code
  miri:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@nightly
        with:
          components: miri
      - run: cargo miri test -p ram-core

  # Documentation build
  docs:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo doc --no-deps --all-features
        env:
          RUSTDOCFLAGS: -D warnings
```

### Required Status Checks

All PRs to `main` must pass:
- `fmt`
- `clippy`
- `test` (all matrix entries)
- `coverage`
- `audit`
- `benchmark` (if performance-sensitive)

---

## Claude Code Integration

### MCP Servers (Recommended)

Configure in `~/.config/claude-code/settings.json`:

```json
{
  "mcpServers": {
    "rust-analyzer": {
      "command": "rust-analyzer",
      "args": ["--lsp"]
    },
    "github": {
      "command": "npx",
      "args": ["-y", "@anthropic/mcp-server-github"],
      "env": {
        "GITHUB_TOKEN": "${GITHUB_TOKEN}"
      }
    },
    "memory": {
      "command": "npx",
      "args": ["-y", "@anthropic/mcp-server-memory"]
    }
  }
}
```

### Custom Skills (`.claude/skills/`)

Create project-specific skills:

```markdown
# .claude/skills/run-tests.md
---
name: test
description: Run project tests with coverage
---

Run the following test workflow:

1. Run unit tests: `cargo test --workspace`
2. Check coverage: `cargo llvm-cov --workspace`
3. Report any failing tests with their error messages
4. Summarize coverage percentage by crate
```

```markdown
# .claude/skills/review-audio.md
---
name: review-audio
description: Review audio code for real-time safety
---

Analyze the provided audio code for:

1. **Lock-free safety**: No mutex/rwlock in audio callbacks
2. **Allocation-free**: No heap allocations in hot paths
3. **Atomic correctness**: Proper memory ordering
4. **Buffer safety**: No panics, proper bounds checking
5. **Latency impact**: Estimate additional latency

Report issues as: CRITICAL, WARNING, or INFO
```

### Hooks (`.claude/hooks/`)

```json
// .claude/hooks/pre-commit.json
{
  "event": "pre-commit",
  "command": "cargo fmt --check && cargo clippy -- -D warnings",
  "failOnError": true
}
```

### Memory/Context Optimization

For long sessions, use structured memory:

```
/mem-search "VBAN protocol implementation"
/mem-search "ring buffer design decisions"
```

Key patterns to remember:
- Connection ID format: `{src}:{dev}:{ch}>{dst}:{dev}:{ch}`
- 1-based channel indexing in API/UI
- Destination-owned subscription model
- LOCAL placeholder in subscriptions.toml

---

## Performance Requirements

### Latency Budgets

| Operation | Target | Max Acceptable |
|-----------|--------|----------------|
| Local route (buffer copy) | < 0.1ms | 0.5ms |
| Full local path | < 3ms | 5ms |
| Network path (LAN) | < 5ms | 10ms |
| ASIO callback processing | < 0.5ms | 1ms |

### Benchmarking

Every performance-critical component needs benchmarks:

```rust
// benches/ring_buffer.rs
use criterion::{criterion_group, criterion_main, Criterion, Throughput};

fn bench_ring_buffer_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("ring_buffer");

    for size in [64, 256, 1024, 4096] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_function(format!("write_{}", size), |b| {
            let buffer = RingBuffer::new(8192);
            let samples = vec![0.5f32; size];
            b.iter(|| buffer.write(&samples));
        });
    }

    group.finish();
}

criterion_group!(benches, bench_ring_buffer_throughput);
criterion_main!(benches);
```

---

## Code Quality Gates

### Pre-commit Checks (Local)

```bash
#!/bin/bash
# .git/hooks/pre-commit

set -e

echo "Running format check..."
cargo fmt --check

echo "Running clippy..."
cargo clippy --all-targets -- -D warnings

echo "Running tests..."
cargo test --workspace

echo "All checks passed!"
```

### Clippy Configuration

```toml
# clippy.toml
avoid-breaking-exported-api = false
cognitive-complexity-threshold = 15
too-many-arguments-threshold = 6
type-complexity-threshold = 200
```

### rustfmt Configuration

```toml
# rustfmt.toml
edition = "2021"
max_width = 100
use_small_heuristics = "Default"
imports_granularity = "Module"
group_imports = "StdExternalCrate"
reorder_imports = true
```

---

## Documentation Requirements

### Code Documentation

```rust
/// Processes incoming audio samples through the routing matrix.
///
/// This function is called from the audio thread and MUST be lock-free.
/// Any blocking operation here will cause audio glitches.
///
/// # Arguments
///
/// * `input` - Input samples from source device (interleaved if multi-channel)
/// * `output` - Output buffer to fill (same format as input)
///
/// # Returns
///
/// Number of samples processed, which may be less than `input.len()` if
/// the internal buffer is full.
///
/// # Safety
///
/// This function uses unsafe internally for performance but maintains
/// the following invariants:
/// - No heap allocations
/// - No system calls
/// - No blocking operations
///
/// # Example
///
/// ```
/// let router = AudioRouter::new(config);
/// let input = [0.5f32; 64];
/// let mut output = [0.0f32; 64];
/// let processed = router.process(&input, &mut output);
/// assert_eq!(processed, 64);
/// ```
pub fn process(&self, input: &[f32], output: &mut [f32]) -> usize {
    // Implementation
}
```

### Architecture Decision Records (ADRs)

Store in `docs/adr/`:

```markdown
# ADR-001: Use VBAN for Network Audio Transport

## Status
Accepted

## Context
We need a network audio protocol that is:
- Low latency (< 5ms)
- Compatible with existing software
- Well-documented

## Decision
Use VBAN protocol as the transport layer.

## Consequences
- Good: Compatible with VB-Audio ecosystem
- Good: Simple UDP-based protocol
- Bad: No built-in authentication
- Mitigation: Document security requirements
```

---

## Development Workflow

### Starting New Feature

```bash
# 1. Create feature branch
git checkout develop
git pull
git checkout -b feature/xxx-description

# 2. Write failing tests first (TDD)
# ... write tests ...
cargo test --workspace  # Should fail

# 3. Implement feature
# ... implement ...
cargo test --workspace  # Should pass

# 4. Run full check
cargo fmt
cargo clippy -- -D warnings
cargo test --workspace
cargo bench --workspace

# 5. Create PR
git push -u origin feature/xxx-description
gh pr create --base develop
```

### Reviewing PRs

Checklist for reviewers:

- [ ] Tests cover new functionality
- [ ] No files exceed 1000 lines
- [ ] Audio code is lock-free
- [ ] Error handling is appropriate
- [ ] Documentation is updated
- [ ] Benchmarks added for perf-critical code
- [ ] No clippy warnings
- [ ] Clean commit history

---

## Troubleshooting

### Common Issues

**Clippy too strict?**
```bash
# Allow specific lint temporarily (add comment explaining why)
#[allow(clippy::too_many_arguments)]  // Builder pattern pending refactor
fn complex_function(/* 7 args */) { }
```

**Coverage too low?**
```bash
# Find uncovered lines
cargo llvm-cov --html
open target/llvm-cov/html/index.html
```

**Benchmark flaky?**
```bash
# Run with more samples
cargo bench -- --sample-size 100
```

---

## Target Machines for Testing

### MANDATORY: 3-Point Network Testing

**ALWAYS test across all 3 machines to verify both local and network features:**

### SSH Key Setup (REQUIRED)

**Before testing, set up SSH keys to avoid repeated password prompts.** Credentials are in `TARGETS.md`. Run once:

```bash
# For Windows targets (use sshpass initially, then copy key)
ssh-copy-id -i ~/.ssh/id_rsa.pub user@hostname

# If ssh-copy-id doesn't work on Windows, use PowerShell:
# Get-Content ~/.ssh/id_rsa.pub | ssh user@hostname "mkdir -p ~/.ssh && cat >> ~/.ssh/authorized_keys"
```

**CRITICAL:** Always use SSH keys for target machine access. Do NOT rely on sshpass with passwords in commands as this causes repeated failures and wastes time.

| Machine | Hostname | IP | OS | User | Role |
|---------|----------|----|----|------|------|
| **stagebox1** | stagebox1.lan | 10.77.9.237 | Windows | newlevel | PRIMARY Windows testing |
| **develbox** | develbox | 10.77.9.21 | Linux | newlevel | Development + Linux testing |
| **iem** | iem | 10.77.9.231 | Windows | iem | Windows test target |

**ALL THREE MACHINES ARE MANDATORY DEPLOYMENT TARGETS. NEVER SKIP ANY.**

### stagebox1.lan - PRIMARY WINDOWS TEST MACHINE

- Full software installation allowed
- Service installation allowed
- Driver testing allowed
- ASIO device testing allowed
- Network routing testing allowed

### develbox - Linux Development Machine

- Primary development environment
- Linux binary testing
- API/UI development and testing
- Network routing endpoint

### iem - WINDOWS TEST TARGET (MANDATORY)

**THIS IS A MANDATORY DEPLOYMENT TARGET. ALWAYS DEPLOY AND RUN HERE.**

- Full software installation allowed
- Must run with TRAY ICON visible (interactive desktop session)
- Use scheduled task to ensure GUI access
- Credentials: user=`iem`, password=`iem`

### 3-Point Network Test Checklist

**Every feature release MUST be tested across all 3 points:**

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│ stagebox1   │◄───►│  develbox   │◄───►│    iem      │
│  (Windows)  │     │   (Linux)   │     │  (Windows)  │
│  PRIMARY    │     │   DEV BOX   │     │ PRODUCTION  │
└─────────────┘     └─────────────┘     └─────────────┘
```

**Basic Connectivity:**
- [ ] Local audio routing on stagebox1 (Windows)
- [ ] Local audio routing on develbox (Linux)
- [ ] Network routing: stagebox1 → develbox
- [ ] Network routing: develbox → stagebox1
- [ ] Web UI accessible from all machines
- [ ] API health check from all machines
- [ ] Cross-platform device discovery

**Deep Feature Testing (per ARCHITECTURE.md):**
- [ ] Generate sine waves to virtual ASIO devices
- [ ] Add multiple virtual ASIO devices
- [ ] Connect to physical ASIO devices
- [ ] Route audio between virtual ↔ physical devices
- [ ] Cross-node network routing with audio verification
- [ ] WebSocket metering updates
- [ ] Route creation/deletion via API
- [ ] Route persistence across restarts

### Self-Hosted GitHub Actions Runner (Future)

**stagebox1.lan can be configured as a self-hosted runner** for automated testing:
- Runs Windows-specific tests (ASIO, Virtual ASIO driver)
- Executes full E2E tests with real audio hardware
- Verifies network routing to develbox
- Automates the 3-point network test as part of CI/CD

This enables fully automated testing of ALL features before merge.

### CRITICAL: Build via GitHub Actions, NOT on Target Machines

**ALL building MUST happen via GitHub Actions CI/CD:**
- Windows binaries: Built by `windows-latest` runner in GitHub Actions
- C++ components (Virtual ASIO driver): Built by GitHub Actions with MSVC
- NEVER attempt to install compilers, build tools, or SDKs on target machines
- NEVER run `cargo build` on target machines
- Target machines are for TESTING PRE-BUILT BINARIES ONLY

### Testing Workflow - STRICT SELF-TESTING RULE

**CRITICAL: Claude MUST perform all testing autonomously.**

You have SSH access to all test machines with credentials in TARGETS.md. You are REQUIRED to:
- Deploy binaries yourself via SSH/SCP
- Run tests yourself on all target machines
- Verify functionality yourself before reporting completion
- NEVER ask the user to test - that is YOUR job

**Testing is NOT complete until YOU have verified it works on ALL THREE MACHINES: stagebox1.lan, develbox, AND iem.**

### Automated Testing Steps - ALL 3 MACHINES (MANDATORY)

**Deploy to ALL test machines after EVERY CI build. NEVER SKIP ANY MACHINE.**

1. **Wait for CI**: Monitor until dev release is available

2. **Deploy to stagebox1.lan** (Windows):
   ```bash
   # Stop existing process
   sshpass -p "newlevel" ssh newlevel@stagebox1.lan "powershell -Command \"Get-Process audiomatrix* -ErrorAction SilentlyContinue | Stop-Process -Force\""
   # Download and start with tray icon
   sshpass -p "newlevel" ssh newlevel@stagebox1.lan "powershell -Command \"irm https://github.com/zbynekdrlik/audiomatrix/releases/download/dev/install-dev.ps1 | iex\""
   # Start via scheduled task for tray icon visibility
   sshpass -p "newlevel" ssh newlevel@stagebox1.lan "schtasks /Create /TN AudioMatrix /TR 'C:\\Users\\newlevel\\AppData\\Local\\AudioMatrix\\audiomatrix.exe -n stagebox1' /SC ONCE /ST 00:00 /RL HIGHEST /F && schtasks /Run /TN AudioMatrix"
   ```

3. **Deploy to develbox** (Linux):
   ```bash
   curl -LO https://github.com/zbynekdrlik/audiomatrix/releases/download/dev/audiomatrix-dev-linux-x64
   chmod +x audiomatrix-dev-linux-x64
   # Restart service with new binary
   ```

4. **Deploy to iem** (Windows - MANDATORY WITH TRAY ICON):
   ```bash
   # Stop existing process
   sshpass -p "iem" ssh iem@10.77.9.231 "powershell -Command \"Get-Process audiomatrix* -ErrorAction SilentlyContinue | Stop-Process -Force\""
   # Download binary
   sshpass -p "iem" ssh iem@10.77.9.231 "powershell -Command \"New-Item -ItemType Directory -Force -Path 'C:\\Users\\iem\\AppData\\Local\\AudioMatrix'; Invoke-WebRequest -Uri 'https://github.com/zbynekdrlik/audiomatrix/releases/download/dev/audiomatrix-dev-windows-x64.exe' -OutFile 'C:\\Users\\iem\\AppData\\Local\\AudioMatrix\\audiomatrix.exe'\""
   # Start via scheduled task for tray icon visibility (REQUIRED!)
   sshpass -p "iem" ssh iem@10.77.9.231 "schtasks /Create /TN AudioMatrix /TR 'C:\\Users\\iem\\AppData\\Local\\AudioMatrix\\audiomatrix.exe -n iem' /SC ONCE /ST 00:00 /RL HIGHEST /F && schtasks /Run /TN AudioMatrix"
   # Verify running with tray icon
   sshpass -p "iem" ssh iem@10.77.9.231 "powershell -Command \"Get-Process audiomatrix* | Select-Object Name,Id\""
   ```

**CRITICAL FOR ALL WINDOWS MACHINES:**
- For tray icon visibility: The logged-in desktop user must run AudioMatrix from their session
- SSH-started processes run in session 0 (Services) = no tray icon visible
- Scheduled tasks only show tray icon if run as the same user logged into desktop
- The service WILL still work via SSH - just no tray icon (API, metering, routing all functional)
- If tray icon is required: User must manually run or set up auto-start for their desktop session

**To verify service is running regardless of tray icon:**
```bash
sshpass -p "PASSWORD" ssh user@host "tasklist /FI \"IMAGENAME eq audiomatrix.exe\""
curl http://HOST:8080/api/v1/health
```

### Windows GUI Session for Tray Icon (Task Scheduler)

**IMPORTANT:** When deploying to Windows machines, use Task Scheduler to run the service under a GUI session so the tray icon is visible. Running via SSH or as a background service won't show the tray icon.

**Create a scheduled task via SSH:**
```powershell
# Create scheduled task that runs at logon with GUI access
schtasks /Create /TN "AudioMatrix" /TR "C:\path\to\audiomatrix.exe -n NODENAME" /SC ONLOGON /RL HIGHEST /F

# Or run immediately under current user's GUI session:
schtasks /Create /TN "AudioMatrix" /TR "C:\path\to\audiomatrix.exe -n NODENAME" /SC ONCE /ST 00:00 /RL HIGHEST /F
schtasks /Run /TN "AudioMatrix"
```

**Alternative: Use PowerShell to start in GUI session:**
```powershell
# This starts the process in the interactive desktop session
$trigger = New-ScheduledTaskTrigger -Once -At (Get-Date).AddSeconds(1)
$action = New-ScheduledTaskAction -Execute "C:\path\to\audiomatrix.exe" -Argument "-n NODENAME"
$principal = New-ScheduledTaskPrincipal -UserId "$env:USERNAME" -LogonType Interactive -RunLevel Highest
Register-ScheduledTask -TaskName "AudioMatrix" -Trigger $trigger -Action $action -Principal $principal -Force
Start-ScheduledTask -TaskName "AudioMatrix"
```

**Key points:**
- `LogonType Interactive` ensures the process runs with desktop access
- `-RunLevel Highest` gives admin privileges if needed for ASIO
- The tray icon ONLY appears when running in an interactive desktop session
- SSH-started processes don't have desktop access, hence no tray icon

### Deep Configuration Tests (Automated)

Run these tests via SSH after deployment:

**1. Health Check (all machines):**
```bash
curl http://stagebox1.lan:8080/api/v1/health
curl http://10.77.9.21:8080/api/v1/health
curl http://10.77.9.231:8080/api/v1/health  # if iem running
```

**2. Device Enumeration (verify ASIO on Windows):**
```bash
curl http://stagebox1.lan:8080/api/v1/nodes/local/devices | jq '.[] | .name'
```

**3. Route CRUD Test:**
```bash
# Create route
curl -X POST http://stagebox1.lan:8080/api/v1/routes -H "Content-Type: application/json" -d '{"source_node":"LOCAL","source_device":"device1","source_channel":1,"destination_node":"LOCAL","destination_device":"device2","destination_channel":1,"volume":1.0,"muted":false}'
# List routes
curl http://stagebox1.lan:8080/api/v1/routes
# Delete route
curl -X DELETE "http://stagebox1.lan:8080/api/v1/routes/ROUTE_ID"
```

**4. Cross-Node Discovery:**
```bash
curl http://stagebox1.lan:8080/api/v1/nodes | jq '.[] | .name'
# Should show both stagebox1 and develbox
```

**5. Network Route Test (stagebox1 ↔ develbox):**
```bash
# Create cross-node route
curl -X POST http://stagebox1.lan:8080/api/v1/routes -H "Content-Type: application/json" -d '{"source_node":"develbox","source_device":"device1","source_channel":1,"destination_node":"LOCAL","destination_device":"device2","destination_channel":1,"volume":1.0,"muted":false}'
```

### E2E Test Implementation Requirements

Before asking user to test anything, implement and run E2E tests that cover:
- UI loads with real devices displayed
- Route creation via API works
- Route deletion via API works
- WebSocket metering (when implemented)
- Cross-node discovery
- Audio routing between devices (verified with test tones when possible)

### TARGETS.md Management

The file `TARGETS.md` (gitignored, local only) contains additional details:
- SSH/remote access credentials (user/password)
- Current deployment status
- Additional test machines

**Important:**
- TARGETS.md is gitignored - never commit credentials
- Always verify target availability before testing
- stagebox1.lan is the PRIMARY Windows test target

---

## Contact & Resources

- **Architecture**: `ARCHITECTURE.md`
- **API Reference**: `cargo doc --open`
- **Issues**: GitHub Issues
- **Discussions**: GitHub Discussions

---

*This document is the development contract. All contributors must follow these guidelines.*
