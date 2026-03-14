<div align="center">
  <img src="../../assets/ORGANUM.png" width="500" />
  <h1>Organum</h1>
  <p>UTAU resampler engine written in Rust</p>

  <img src="https://img.shields.io/github/v/release/KakouLabs/Organum?style=flat-square" alt="Latest Release">
</div>

---

[한국어](../../README.md) | [English](README.md) | [日本語](../ja/README.md)

Organum is a resampler engine for UTAU and OpenUtau. It implements a WORLD vocoder-based analysis/synthesis pipeline in Rust.

## Features

- WORLD vocoder-based spectral analysis & synthesis
- Parallel processing using Rayon
- Configuration customization via `organum.yaml`
- Skip repeated analysis with Zstd compressed cache (`.ogc`)

## 🎤 Voicebank Release
The **Tetsu Kokuno(코쿠노 테츠, 虚空の 鉄)** voice bank has been released! You can download it at [https://utau.sapo.dev](https://utau.sapo.dev).

## GPU Route Notice

- The `gpu-warp` route is an **experimental feature**.
- Current implementation may be **slower** than the CPU path (SIMD/Parallel) in practical sections due to GPU round-trip overhead (upload/readback), and is therefore not recommended.
- Default is disabled (`gpu_warp_enabled: false`). We recommend using the CPU path unless for specific benchmarking or experimental purposes.

## Installation

1. Download binaries from the [Releases](https://github.com/KakouLabs/Organum/releases) page.
   - Each platform provides `*-cpu` (standard CPU build) and `*-cpu-gpu` (includes `gpu-warp` feature) archives.
   - `organum-wavtool` is currently provided for Windows only. Linux/macOS archives only include `organum-resampler` and `caching-tool`.
   - Latest releases may include benchmark summaries/log assets.
2. Place them in OpenUtau's `Resamplers` directory.

## Usage

In OpenUtau or UTAU:

1. Set `organum-resampler` as the Resampler.
2. On Windows, set `organum-wavtool` as the Wavtool.
3. On Linux/macOS, use one of the following:
   - Run Windows wavtool via `wine organum-wavtool.exe`.
   - Use OpenUtau's default wavtool.

### Logging

All three binaries support structured logging.

- `--verbose`: Enable debug level logs.
- `--log-format pretty|json`: Choose log output format.

Example:

```powershell
./organum-resampler --verbose --log-format json ...
./organum-wavtool --log-format json ...
./caching-tool.exe --verbose --log-format json "C:\Path\To\Your\Voicebank"
```

```bash
wine organum-wavtool.exe --log-format json ...
```

### Voicebank Caching

Pre-analyzing voicebanks with the caching tool skips the analysis step during rendering.

```powershell
./caching-tool.exe "C:\Path\To\Your\Voicebank"
```

## Configuration

If `organum.yaml` is missing during execution, it's automatically generated with default values.

```yaml
feature_extension: "ogc"
sample_rate: 44100
frame_period: 5.0
zstd_compression_level: 3
compressor_threshold: 0.85
compressor_limit: 0.99
gpu_warp_enabled: false
gpu_warp_min_frames: 2048
```

The cache is automatically invalidated and regenerated if the format/schema/engine version or key settings (`sample_rate`, `frame_period`) differ from the current execution values.

See the [Configuration Guide](CONFIGURATION.md) for details.

See [SIMD Validation](SIMD_VALIDATION.md) for the SIMD validation/benchmark guide.

## Build

Organum uses a single release profile (`release`).

```bash
cargo build --workspace --release
```

```powershell
./build.bat
```

```bash
./build.sh
```

## Comparison

Based on Kasane Teto UTAU voicebank, processing time for approx. 500ms segment.

| Engine | Language | Multithreading | Avg. Time |
| :--- | :--- | :--- | :--- |
| Organum | Rust | Yes (Rayon) | ~25ms |
| straycat-rs | Rust | Yes | ~35ms |
| tn_fnds | C++ | No | ~110ms |

| Feature | Organum | straycat-rs | tn_fnds |
| :--- | :--- | :--- | :--- |
| Acoustic Model | WORLD | WORLD | WORLD/Classic |
| Configuration | YAML | TOML | CLI Only |
| License | MIT | MIT | GPL |

See [Comparison](COMPARISON.md) for audio sample comparisons.

## Flags

Rendering parameters can be controlled via flags. `P` and `y` are identical Peak parameter aliases. Detailed reference: [Flags](FLAGS.md)

## License

MIT
