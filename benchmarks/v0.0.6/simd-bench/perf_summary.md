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
| A_cache_off_simd_off | 30 | 696.098 | 1680.752 |
| B_cache_off_simd_on | 30 | 697.239 | 1672.368 |
| C_cache_on_simd_off | 30 | 47.301 | 50.587 |
| D_cache_on_simd_on | 30 | 46.438 | 50.096 |

## SIMD On vs Off

- cache OFF median speedup (B vs A): -0.16%
- cache OFF p95 speedup (B vs A): +0.50%
- cache ON median speedup (D vs C): +1.82%
- cache ON p95 speedup (D vs C): +0.97%

## Recommendation

- cache OFF: SIMD OFF
- cache ON: SIMD ON
