# Profiling with Flamegraph

This guide shows how to find CPU bottlenecks in Organum with [`cargo flamegraph`](https://github.com/flamegraph-rs/flamegraph). Use flamegraphs to choose optimization targets, then confirm improvements with benchmarks or regression tests.

[한국어](../PROFILING.md) | [English](PROFILING.md) | [日本語](../ja/PROFILING.md)

---

## 1) Install tools

```bash
cargo install flamegraph
```

On Linux, `cargo flamegraph` uses `perf`.

```bash
# Debian
sudo apt install -y linux-perf

# Ubuntu
sudo apt install -y linux-tools-common linux-tools-generic linux-tools-$(uname -r)
```

If `perf` is blocked for unprivileged users, either run `cargo flamegraph --root ...` or temporarily lower the perf restriction on a local development machine:

```bash
sysctl -n kernel.perf_event_paranoid
echo -1 | sudo tee /proc/sys/kernel/perf_event_paranoid
```

Changing `perf_event_paranoid` has security implications. Prefer doing this only on a local profiling machine or VM/container.

## 2) Build profile notes

The distribution release profile prioritizes speed and binary size, so it removes debug symbols:

```toml
[profile.release]
opt-level = 3
lto = "fat"
strip = "symbols"
debug = false
```

That is good for shipped binaries, but not for flamegraphs. Use the `profiling` profile instead; it inherits release optimizations while keeping symbol/debug information.

```toml
[profile.profiling]
inherits = "release"
debug = true
strip = false
```

If `perf` shows mostly `[unknown]` stacks on a newer Linux/Rust toolchain using `lld` or `mold`, add the linker flag recommended by `flamegraph-rs` for that profiling run:

```bash
RUSTFLAGS="-Clink-arg=-Wl,--no-rosegment" cargo flamegraph --profile profiling ...
```

## 3) Profile the WORLD analysis benchmark

Use this first when investigating analysis-stage bottlenecks (`DIO`, `StoneMask`, `CheapTrick`, `D4C`). It uses deterministic synthetic audio and does not require a voicebank sample.

```bash
mkdir -p bench-results/flamegraphs
WORLD_BENCH_WARMUP=1 \
WORLD_BENCH_ITERS=10 \
WORLD_BENCH_REPEATS=1 \
cargo flamegraph \
  --profile profiling \
  --bin world-bench \
  --freq 997 \
  --output bench-results/flamegraphs/world-bench.svg
```

Open `bench-results/flamegraphs/world-bench.svg` in a browser. Wide boxes are where CPU time is being spent. For this binary, expect useful stacks under `generate_features`, `world::native::analysis`, `cheaptrick`, `d4c`, and FFT helpers.

## 4) Profile micro-benchmarks

Use these when investigating post-processing paths rather than full WORLD analysis.

```bash
mkdir -p bench-results/flamegraphs

cargo flamegraph \
  --profile profiling \
  --bin ap-bench \
  --output bench-results/flamegraphs/ap-bench.svg

cargo flamegraph \
  --profile profiling \
  --bin warp-bench \
  --output bench-results/flamegraphs/warp-bench.svg
```

For GPU-route experiments, pass the feature flag and route thresholds explicitly:

```bash
WARP_BENCH_GPU_MIN_FRAMES=1 \
AP_BENCH_GPU_MIN_FRAMES=1 \
cargo flamegraph \
  --profile profiling \
  --features gpu-warp \
  --bin warp-bench \
  --output bench-results/flamegraphs/warp-bench-gpu.svg
```

## 5) Profile the actual resampler path

Use this to confirm bottlenecks on a real UTAU/OpenUtau-style render. Run once first to populate caches, then profile a second run if you want synthesis/resampling costs without first-time analysis noise.

```bash
cargo build --profile profiling --bin organum-resampler

# Warm cache / sanity run
target/profiling/organum-resampler input.wav /tmp/organum-profile-warmup.wav C4 100 - 0 480 0 0 100 0 !120

mkdir -p bench-results/flamegraphs
cargo flamegraph \
  --profile profiling \
  --bin organum-resampler \
  --output bench-results/flamegraphs/resampler.svg \
  -- input.wav /tmp/organum-profile.wav C4 100 - 0 480 0 0 100 0 !120
```

If you want to profile analysis/cache-miss behavior instead, delete the matching `.ogc` cache or use a fresh input copy before profiling.

## 6) Read the result

- **Width matters**: wider frames account for more samples and are better optimization targets.
- **Top boxes are leaf work**: expensive loops or library calls usually appear near the top.
- **Compare with timings**: flamegraphs show proportions. Always pair them with wall-clock benchmark output before and after a change.
- **Separate workloads**: analysis (`world-bench`), aperiodicity post-processing (`ap-bench`), warp (`warp-bench`), and full resampling (`organum-resampler`) answer different questions.

Generated flamegraphs should stay under `bench-results/`, which is ignored by git.
