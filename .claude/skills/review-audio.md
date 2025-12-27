---
name: review-audio
description: Review audio code for real-time safety and performance
---

Analyze the specified audio code for real-time safety. This is CRITICAL for audio applications.

## Checks to Perform

### 1. Lock-Free Safety (CRITICAL)
- [ ] No `Mutex`, `RwLock`, or other blocking primitives in audio callbacks
- [ ] No `async`/`.await` in audio path
- [ ] Only atomic operations for shared state
- [ ] Proper memory ordering (Relaxed for counters, Acquire/Release for synchronization)

### 2. Allocation-Free (CRITICAL)
- [ ] No `Vec::push`, `Box::new`, or heap allocations in hot path
- [ ] No `String` operations
- [ ] No `format!` or `println!` macros
- [ ] Pre-allocated buffers only

### 3. Panic-Free (CRITICAL)
- [ ] No `.unwrap()` or `.expect()` in audio path
- [ ] Bounds checking via `.get()` instead of `[]` indexing
- [ ] No `assert!` that could panic

### 4. Deterministic Timing
- [ ] No system calls (file I/O, network, etc.)
- [ ] No unbounded loops
- [ ] Constant-time operations preferred

### 5. SIMD Optimization (if applicable)
- [ ] Consider `#[target_feature(enable = "avx2")]` for hot loops
- [ ] Proper alignment for SIMD operations

## Report Format

```
## Audio Code Review: {file/function name}

### CRITICAL Issues
- Issue description with line number and fix suggestion

### WARNINGS
- Potential issues that should be addressed

### SUGGESTIONS
- Performance improvements

### Estimated Impact
- Latency addition: ~X.Xms
- CPU usage: LOW/MEDIUM/HIGH
```

## Example Issues

**CRITICAL**: `Mutex::lock()` at line 42
- Fix: Use `AtomicF32` for gain value instead

**WARNING**: `Vec::with_capacity()` called in constructor
- OK if called once during setup, not in audio callback

**SUGGESTION**: Consider using `unchecked_get()` with proper invariants
- Current bounds check adds ~5ns per sample
