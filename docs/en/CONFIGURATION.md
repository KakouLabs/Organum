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
| `gpu_warp_min_frames` | integer | `usize::MAX` | Minimum render frame length to attempt GPU warp path |
| `gpu_ap_min_frames` | integer | `usize::MAX` | Minimum render frame length to attempt GPU aperiodicity path |
| `output_dither` | boolean | `true` | Whether to use dithering/noise shaping for WAV output |
| `memory_cache_enabled` | boolean | `true` | Whether to use the in-process feature memory cache |
| `memory_cache_max_mb` | integer | `256` | Maximum size of the in-process feature memory cache in MiB |
| `quality_preset` | string | `"balanced"` | Quality tuning preset (`classic`, `balanced`, `clear`, `breathy-safe`) |

> Note: The GPU path is an experimental feature. Due to its current implementation (upload/readback overhead), it may be slower than the CPU (SIMD/Parallel) path in typical rendering. Keeping it at the default (`false`) is recommended.

## Example

```yaml
feature_extension: "ogc"
sample_rate: 44100
frame_period: 5.0
zstd_compression_level: 3
gpu_warp_enabled: false
gpu_warp_min_frames: 18446744073709551615
gpu_ap_min_frames: 18446744073709551615
output_dither: true
memory_cache_enabled: true
memory_cache_max_mb: 256
quality_preset: "balanced"
```

## Quality Presets

### `classic`
- Minimizes the recently added consonant/breath/high-pitch stabilization logic.
- Recommended if you want to preserve the legacy Organum / WORLD-like character as much as possible.

### `balanced`
- Default preset.
- Aims to reduce `sh/j/ch` noise, breath/h failures, and excessive high-pitch tension without changing the overall character too much.

### `clear`
- Slightly reduces muffled high-mid/high frequencies and improves perceived clarity.
- Depending on the voicebank, consonants may sound more defined.

### `breathy-safe`
- Prioritizes safer handling of breath / `h` / weak unvoiced segments.
- A good first option for voicebanks where breath phonemes tend to collapse into noise.

## Cache Compatibility Policy

Organum caches (`.ogc` or custom extension) are reused only when the following metadata matches the current execution environment:

- cache format version
- engine version (`CARGO_PKG_VERSION`)
- `sample_rate`
- `frame_period`

If any of these values differ, the cache is considered incompatible and is automatically regenerated.
