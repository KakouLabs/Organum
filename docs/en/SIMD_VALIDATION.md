# SIMD Validation Guide

You can use `scripts/run-simd-validation.py` to automatically execute the A/B/C/D validation matrix.

[한국어](../SIMD_VALIDATION.md) | [English](SIMD_VALIDATION.md) | [日本語](../ja/SIMD_VALIDATION.md)

---

## Prerequisites

- `samples.txt` (One WAV path per line)
- `target/release/organum-resampler` build completed

Example:

```text
audios/test01.wav
audios/test02.wav
```

## 1) Build

```bash
cargo build --release
```

## 2) Execute Validation

```bash
python scripts/run-simd-validation.py --samples samples.txt
```

To specify explicitly on Windows:

```powershell
python scripts/run-simd-validation.py --samples samples.txt --resampler-cmd "target/release/organum-resampler.exe"
```

Default output paths:

- `benchmarks/<current version>/simd-validation/metrics.csv`
- `benchmarks/<current version>/simd-validation/metrics_summary.md`

## 3) Check Results

- Detailed Numerical CSV: `benchmarks/<current version>/simd-validation/metrics.csv`
- Summary Document: `benchmarks/<current version>/simd-validation/metrics_summary.md`

## Notes

- SIMD toggle uses `ORGANUM_AP_SIMD=on|off|auto`.
- This script automatically measures length/RMS/Peak/null-test.
- F0 statistics/perceptual evaluation should be conducted separately according to the project release validation procedure.
