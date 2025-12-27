---
name: bench
description: Run benchmarks and analyze performance
---

Run performance benchmarks for AudioMatrix and analyze results.

## Steps

1. Run criterion benchmarks:
   ```bash
   cargo bench --workspace 2>&1
   ```

2. Parse and analyze results:
   - Extract timing for each benchmark
   - Identify any regressions (if baseline exists)
   - Calculate throughput metrics

3. For audio-specific benchmarks, calculate:
   - Samples per second
   - Latency per buffer
   - CPU usage estimate

## Report Format

```
## Benchmark Results

### Ring Buffer Performance
| Operation | Time | Throughput |
|-----------|------|------------|
| write_64  | X.Xus | XX MB/s |
| read_64   | X.Xus | XX MB/s |

### Latency Budget Analysis
| Component | Measured | Budget | Status |
|-----------|----------|--------|--------|
| Buffer copy | 0.1ms | 0.5ms | OK |
| Matrix route | 0.05ms | 0.1ms | OK |

### Recommendations
- Any optimizations suggested based on results
```

## Performance Targets (from ARCHITECTURE.md)

- Local route (buffer copy): < 0.1ms target, 0.5ms max
- Full local path: < 3ms target, 5ms max
- Network path (LAN): < 5ms target, 10ms max
- ASIO callback: < 0.5ms target, 1ms max
