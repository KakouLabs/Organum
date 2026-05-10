# Flamegraph プロファイリングガイド

この文書では、[`cargo flamegraph`](https://github.com/flamegraph-rs/flamegraph) を使って Organum の CPU ボトルネックを調べる方法を説明します。Flamegraph は最適化対象を選ぶために使い、実際の改善は benchmark や regression test で確認してください。

[한국어](../PROFILING.md) | [English](../en/PROFILING.md) | [日本語](PROFILING.md)

---

## 1) ツールのインストール

```bash
cargo install flamegraph
```

Linux では `cargo flamegraph` が `perf` を使用します。

```bash
# Debian
sudo apt install -y linux-perf

# Ubuntu
sudo apt install -y linux-tools-common linux-tools-generic linux-tools-$(uname -r)
```

権限のため `perf` が使えない場合は、`cargo flamegraph --root ...` を使うか、ローカル開発マシンで一時的に perf 制限を下げます。

```bash
sysctl -n kernel.perf_event_paranoid
echo -1 | sudo tee /proc/sys/kernel/perf_event_paranoid
```

`perf_event_paranoid` の変更にはセキュリティ上の影響があります。ローカルの profiling 用マシンや VM/container で一時的に使うことを推奨します。

## 2) ビルドプロファイル

配布用 release profile は速度とバイナリサイズを優先するため、debug symbol を削除します。

```toml
[profile.release]
opt-level = 3
lto = "fat"
strip = "symbols"
debug = false
```

これは配布バイナリには適していますが、flamegraph には向きません。release 最適化を継承しつつ symbol/debug info を保持する `profiling` profile を使用してください。

```toml
[profile.profiling]
inherits = "release"
debug = true
strip = false
```

新しい Linux/Rust toolchain で `lld` または `mold` により stack がほとんど `[unknown]` になる場合は、`flamegraph-rs` の推奨に従い、その profiling 実行に linker flag を追加してください。

```bash
RUSTFLAGS="-Clink-arg=-Wl,--no-rosegment" cargo flamegraph --profile profiling ...
```

## 3) WORLD 解析 benchmark の profiling

解析段階のボトルネック（`DIO`, `StoneMask`, `CheapTrick`, `D4C`）を見るときは、まずこれを使います。`world-bench` は deterministic synthetic audio を生成するため、voicebank sample は不要です。

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

`bench-results/flamegraphs/world-bench.svg` をブラウザで開いてください。幅の広い box が CPU 時間を多く使っている箇所です。この binary では `generate_features`, `world::native::analysis`, `cheaptrick`, `d4c`, FFT helper などが主な確認対象になります。

## 4) Micro-benchmark の profiling

WORLD 解析全体ではなく後処理経路を見る場合に使います。

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

GPU route を試す場合は feature flag と threshold を明示します。

```bash
WARP_BENCH_GPU_MIN_FRAMES=1 \
AP_BENCH_GPU_MIN_FRAMES=1 \
cargo flamegraph \
  --profile profiling \
  --features gpu-warp \
  --bin warp-bench \
  --output bench-results/flamegraphs/warp-bench-gpu.svg
```

## 5) 実際の resampler 経路の profiling

実際の UTAU/OpenUtau 形式の render でボトルネックを確認するときに使います。cache 済み状態の合成/リサンプリングコストを見たい場合は、先に一度実行してから二度目を profiling してください。

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

解析/cache-miss のコストも見たい場合は、profiling 前に該当する `.ogc` cache を削除するか、新しい input copy を使ってください。

## 6) 結果の読み方

- **幅が重要です**: 幅の広い frame ほど sample の比率が大きく、最適化候補になります。
- **上側の box は leaf work です**: 高コストの loop や library call は通常 stack の上側に出ます。
- **時間測定と併用してください**: flamegraph は比率を示します。変更前後の wall-clock benchmark も比較してください。
- **workload を分けてください**: 解析（`world-bench`）、aperiodicity 後処理（`ap-bench`）、warp（`warp-bench`）、全体 resampler（`organum-resampler`）は別々の質問に答えます。

生成した flamegraph は git で無視される `bench-results/` 以下に置いてください。
