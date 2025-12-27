---
name: test
description: Run project tests with coverage and report results
---

Run the full test suite for AudioMatrix:

1. First, check if the project compiles:
   ```bash
   cargo check --workspace
   ```

2. Run all unit and integration tests:
   ```bash
   cargo test --workspace --all-features
   ```

3. If cargo-llvm-cov is available, generate coverage:
   ```bash
   cargo llvm-cov --workspace --all-features
   ```

4. Summarize results:
   - Total tests passed/failed
   - Coverage percentage per crate
   - Any failing test names with brief error description

5. If tests fail, provide actionable suggestions for fixing.

Report format:
```
## Test Results

**Status**: PASS/FAIL
**Tests**: X passed, Y failed
**Coverage**: XX.X%

### Failed Tests (if any)
- `crate::module::test_name`: Brief error description

### Coverage by Crate
| Crate | Coverage |
|-------|----------|
| ram-core | XX.X% |
```
