# Flamegraph 프로파일링 가이드

이 문서는 [`cargo flamegraph`](https://github.com/flamegraph-rs/flamegraph)로 Organum의 CPU 병목을 찾는 방법을 정리합니다. Flamegraph는 최적화 대상을 고르는 도구로 사용하고, 실제 개선 여부는 benchmark나 regression test로 다시 확인하세요.

[한국어](PROFILING.md) | [English](en/PROFILING.md) | [日本語](ja/PROFILING.md)

---

## 1) 도구 설치

```bash
cargo install flamegraph
```

Linux에서 `cargo flamegraph`는 `perf`를 사용합니다.

```bash
# Debian
sudo apt install -y linux-perf

# Ubuntu
sudo apt install -y linux-tools-common linux-tools-generic linux-tools-$(uname -r)
```

권한 때문에 `perf`가 막히면 `cargo flamegraph --root ...`를 사용하거나, 로컬 개발 머신에서 임시로 perf 제한을 낮출 수 있습니다.

```bash
sysctl -n kernel.perf_event_paranoid
echo -1 | sudo tee /proc/sys/kernel/perf_event_paranoid
```

`perf_event_paranoid` 변경은 보안상 영향이 있으므로, 로컬 profiling 머신이나 VM/container에서만 임시로 사용하는 것을 권장합니다.

## 2) 빌드 프로파일 참고

배포용 release profile은 속도와 용량을 우선해서 debug symbol을 제거합니다.

```toml
[profile.release]
opt-level = 3
lto = "fat"
strip = "symbols"
debug = false
```

이 설정은 배포 바이너리에는 좋지만 flamegraph에는 적합하지 않습니다. 대신 release 최적화를 상속하되 symbol/debug info를 유지하는 `profiling` profile을 사용하세요.

```toml
[profile.profiling]
inherits = "release"
debug = true
strip = false
```

최신 Linux/Rust toolchain에서 `lld` 또는 `mold` 때문에 stack이 대부분 `[unknown]`으로 보이면, `flamegraph-rs` 권장대로 해당 profiling 실행에 linker flag를 추가하세요.

```bash
RUSTFLAGS="-Clink-arg=-Wl,--no-rosegment" cargo flamegraph --profile profiling ...
```

## 3) WORLD 분석 benchmark 프로파일링

분석 단계 병목(`DIO`, `StoneMask`, `CheapTrick`, `D4C`)을 볼 때 먼저 사용하세요. `world-bench`는 deterministic synthetic audio를 만들기 때문에 별도 voicebank sample이 필요 없습니다.

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

`bench-results/flamegraphs/world-bench.svg`를 브라우저로 여세요. 넓은 박스가 CPU 시간을 많이 쓴 구간입니다. 이 binary에서는 `generate_features`, `world::native::analysis`, `cheaptrick`, `d4c`, FFT helper 쪽 stack을 주로 보게 됩니다.

## 4) Micro-benchmark 프로파일링

전체 WORLD 분석이 아니라 후처리 경로를 볼 때 사용합니다.

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

GPU route를 실험하려면 feature flag와 threshold를 명시하세요.

```bash
WARP_BENCH_GPU_MIN_FRAMES=1 \
AP_BENCH_GPU_MIN_FRAMES=1 \
cargo flamegraph \
  --profile profiling \
  --features gpu-warp \
  --bin warp-bench \
  --output bench-results/flamegraphs/warp-bench-gpu.svg
```

## 5) 실제 resampler 경로 프로파일링

실제 UTAU/OpenUtau 스타일 render에서 병목을 확인할 때 사용합니다. cache가 채워진 상태의 합성/리샘플링 비용을 보고 싶으면 먼저 한 번 실행한 뒤 두 번째 실행을 profiling하세요.

```bash
cargo build --profile profiling --bin organum-resampler

# cache warm-up / sanity run
target/profiling/organum-resampler input.wav /tmp/organum-profile-warmup.wav C4 100 - 0 480 0 0 100 0 !120

mkdir -p bench-results/flamegraphs
cargo flamegraph \
  --profile profiling \
  --bin organum-resampler \
  --output bench-results/flamegraphs/resampler.svg \
  -- input.wav /tmp/organum-profile.wav C4 100 - 0 480 0 0 100 0 !120
```

분석/cache-miss 비용까지 보고 싶다면 profiling 전에 해당 `.ogc` cache를 지우거나 fresh input copy를 사용하세요.

## 6) 결과 읽는 법

- **너비가 중요합니다**: 넓은 frame일수록 sample 비중이 크고 최적화 후보로 좋습니다.
- **위쪽 박스는 leaf work입니다**: 비싼 loop나 library call은 보통 stack 위쪽에 나타납니다.
- **시간 측정과 같이 보세요**: flamegraph는 비율을 보여줍니다. 변경 전후 wall-clock benchmark도 같이 비교해야 합니다.
- **workload를 분리하세요**: 분석(`world-bench`), aperiodicity 후처리(`ap-bench`), warp(`warp-bench`), 전체 resampler(`organum-resampler`)는 서로 다른 질문에 답합니다.

생성된 flamegraph는 git에서 무시되는 `bench-results/` 아래에 두세요.
