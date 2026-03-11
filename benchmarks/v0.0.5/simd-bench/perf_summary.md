# SIMD Performance Summary

- Samples: 10
- Repeats per sample/case: 3
- Length(ms): 500
- Host: Linux-6.18.13-zen1-1-zen-x86_64-with-glibc2.43
- Python: 3.14.3
- Raw CSV: `/home/KimKyuRae/Desktop/Organum/simd-bench/perf_raw.csv`
- Summary CSV: `/home/KimKyuRae/Desktop/Organum/simd-bench/perf_summary.csv`

## Case Stats

| case | runs | median_ms | p95_ms |
|---|---:|---:|---:|
| A_cache_off_simd_off | 30 | 682.801 | 1671.931 |
| B_cache_off_simd_on | 30 | 697.773 | 1649.850 |
| C_cache_on_simd_off | 30 | 45.615 | 49.512 |
| D_cache_on_simd_on | 30 | 45.516 | 50.400 |

## SIMD On vs Off

- cache OFF median speedup (B vs A): -2.19%
- cache OFF p95 speedup (B vs A): +1.32%
- cache ON median speedup (D vs C): +0.22%
- cache ON p95 speedup (D vs C): -1.80%

## Recommendation

- cache OFF: SIMD OFF
- cache ON: SIMD OFF
