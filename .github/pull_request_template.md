## Summary
<!-- Brief description of changes (1-2 sentences) -->

## Type of Change
<!-- Check all that apply -->
- [ ] Feature (new functionality)
- [ ] Bug fix (non-breaking fix)
- [ ] Refactor (no functional changes)
- [ ] Performance (optimization)
- [ ] Documentation
- [ ] CI/CD
- [ ] Dependencies

## Changes Made
<!-- Bullet points of specific changes -->
-

## Testing
<!-- Describe testing performed -->
- [ ] Unit tests added/updated
- [ ] Integration tests added/updated
- [ ] Manual testing performed
- [ ] Benchmarks run (if performance-related)

## Audio Safety Checklist
<!-- Required for audio path changes -->
- [ ] No mutex/locks in audio callbacks
- [ ] No heap allocations in hot path
- [ ] No panic paths (unwrap, expect, assert)
- [ ] N/A (not audio path code)

## Code Quality Checklist
- [ ] Code follows project style (`cargo fmt`)
- [ ] No clippy warnings (`cargo clippy -- -D warnings`)
- [ ] No files exceed 1000 lines
- [ ] Documentation updated (if public API changed)
- [ ] CHANGELOG updated (if user-facing change)

## Related Issues
<!-- Link related issues: Closes #123, Relates to #456 -->

## Screenshots/Output
<!-- If applicable, add screenshots or command output -->
