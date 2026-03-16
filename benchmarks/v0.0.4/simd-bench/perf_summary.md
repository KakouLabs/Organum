# SIMD Performance Summary

- Samples: 10
- Repeats per sample/case: 3
- Length(ms): 500
- Host: Windows-10-10.0.19045-SP0
- Python: 3.11.9
- Raw CSV: `C:\Users\SapoKR\organum\simd-bench\perf_raw.csv`
- Summary CSV: `C:\Users\SapoKR\organum\simd-bench\perf_summary.csv`

## Case Stats

| case | runs | median_ms | p95_ms |
|---|---:|---:|---:|
| A_cache_off_simd_off | 30 | 757.644 | 2075.653 |
| B_cache_off_simd_on | 30 | 735.441 | 1851.669 |
| C_cache_on_simd_off | 30 | 65.511 | 83.613 |
| D_cache_on_simd_on | 30 | 64.589 | 152.059 |

## SIMD On vs Off

- cache OFF median speedup (B vs A): +2.93%
- cache OFF p95 speedup (B vs A): +10.79%
- cache ON median speedup (D vs C): +1.41%
- cache ON p95 speedup (D vs C): -81.86%

## Recommendation

- cache OFF: SIMD ON
- cache ON: SIMD OFF
