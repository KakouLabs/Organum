# Benchmark Artifacts

This directory contains checked-in benchmark snapshots for released Organum versions.

These files are intentionally versioned as release evidence, not as build inputs. They document historical SIMD validation and performance measurements that are referenced by release notes or validation work.

Guidelines:

- Keep per-release benchmark snapshots under `benchmarks/vX.Y.Z/`.
- Do not write ad-hoc local benchmark output directly into this directory.
- Use `/bench-results/`, `/simd-bench/`, or `/simd-validation/` for local generated output; those paths are ignored by git.
- If a benchmark snapshot is included in a release, include enough metadata for reproduction, such as date, command, target platform, and relevant feature flags.

If these files are no longer needed as release evidence, move them to release assets instead of keeping generated data in the source tree.
