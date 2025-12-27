---
name: lint
description: Run linting checks (fmt, clippy) and fix issues
---

Run all linting checks and provide fixes for any issues.

## Steps

1. Check formatting:
   ```bash
   cargo fmt --check --all
   ```

2. Run clippy with pedantic:
   ```bash
   cargo clippy --all-targets --all-features -- -D warnings -D clippy::pedantic -A clippy::module_name_repetitions
   ```

3. For each issue found:
   - Show the warning/error
   - Provide the fix
   - Apply the fix if requested

## Auto-Fix Mode

If user says "fix" or "apply":
```bash
cargo fmt --all
cargo clippy --fix --allow-dirty --allow-staged
```

## Report Format

```
## Lint Results

### Format Issues
- [file:line] Issue description

### Clippy Warnings
- [file:line] `warning_name`: Description
  - Fix: <code suggestion>

### Summary
- Format: X issues (auto-fixable)
- Clippy: X warnings, Y errors
```
