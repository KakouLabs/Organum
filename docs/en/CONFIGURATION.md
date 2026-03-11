# Configuration

Organum allows behavior customization through a YAML configuration file. Upon the first run, `organum.yaml` is automatically generated in the same directory as the executable.

[한국어](../CONFIGURATION.md) | [English](CONFIGURATION.md) | [日本語](../ja/CONFIGURATION.md)

---

## Parameters

| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `feature_extension` | string | `"ogc"` | Cache file extension (e.g., `ogc`, `llsm`) |
| `sample_rate` | integer | `44100` | Analysis/synthesis sample rate (Hz) |
| `frame_period` | float | `5.0` | WORLD frame period (ms) |
| `zstd_compression_level` | integer | `3` | Cache compression level (1-22) |
| `gpu_warp_enabled` | boolean | `false` | Whether to use the experimental GPU path for `warp_spectrum` |
| `gpu_warp_min_frames` | integer | `2048` | Minimum render frame length to attempt GPU path |
| `output_dither` | boolean | `true` | Whether to use dithering/noise shaping for WAV output |

> Note: The GPU path is an experimental feature. Due to its current implementation (upload/readback overhead), it may be slower than the CPU (SIMD/Parallel) path in typical rendering. Keeping it at the default (`false`) is recommended.

## Example

```yaml
feature_extension: "ogc"
sample_rate: 44100
frame_period: 5.0
zstd_compression_level: 3
gpu_warp_enabled: false
gpu_warp_min_frames: 2048
output_dither: true
```

## Cache Compatibility Policy

Organum caches (`.ogc` or custom extension) are reused only when the following metadata matches the current execution environment:

- cache format version
- cache schema version
- engine version (`CARGO_PKG_VERSION`)
- `sample_rate`
- `frame_period`

If any of these values differ, the cache is considered incompatible and is automatically regenerated.
