# Configuration

Organum은 YAML 설정 파일로 동작을 커스터마이즈할 수 있습니다. `organum.yaml`이 없으면 파일을 생성하지 않고 내장 기본값으로 즉시 실행됩니다. 설정을 바꾸려면 실행 파일과 같은 디렉토리에 `organum.yaml`을 직접 만들면 됩니다.

[한국어](CONFIGURATION.md) | [English](en/CONFIGURATION.md) | [日本語](ja/CONFIGURATION.md)

---

| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `feature_extension` | string | `"ogc"` | 캐시 파일 확장자 (예: `ogc`, `llsm`) |
| `sample_rate` | integer | `44100` | 분석/합성 샘플레이트 (Hz) |
| `frame_period` | float | `5.0` | WORLD 프레임 주기 (ms) |
| `zstd_compression_level` | integer | `3` | 캐시 압축 레벨 (1-22) |
| `gpu_warp_enabled` | boolean | `false` | `warp_spectrum` 실험 GPU 경로 사용 여부 |
| `gpu_warp_min_frames` | integer | `usize::MAX` | GPU 경로를 시도할 최소 렌더 프레임 길이 |
| `gpu_ap_min_frames` | integer | `usize::MAX` | GPU aperiodicity 경로를 시도할 최소 렌더 프레임 길이 |
| `output_dither` | boolean | `true` | WAV 출력 시 디더링/노이즈 쉐이핑 사용 여부 |
| `memory_cache_enabled` | boolean | `true` | 프로세스 내 feature 메모리 캐시 사용 여부 |
| `memory_cache_max_mb` | integer | `512` | 프로세스 내 feature 메모리 캐시 최대 크기(MiB) |
| `quality_preset` | string | `"balanced"` | 음질 보정 프리셋 (`classic`, `balanced`, `clear`, `breathy-safe`) |

> 참고: GPU 경로는 실험적 기능이며, 현재 구현 특성상(업로드/리드백 오버헤드) 일반적인 렌더링에서는 CPU(SIMD/병렬)보다 느릴 수 있습니다. 기본값(`false`) 유지 권장.

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
memory_cache_max_mb: 512
quality_preset: "balanced"
```

## Quality Presets

### `classic`
- 최근 추가된 자음/숨소리/고음 안정화 보정을 최소화합니다.
- 기존 Organum / WORLD 계열 느낌을 최대한 유지하고 싶을 때 추천합니다.

### `balanced`
- 기본값입니다.
- 기존 질감을 크게 바꾸지 않으면서 `sh/j/ch` 노이즈, breath/h 실패, 고음 과긴장 완화를 목표로 합니다.

### `clear`
- 중고역의 답답함을 조금 더 줄이고 고음의 명료도를 높이는 방향입니다.
- 음원에 따라 자음이 더 또렷하게 느껴질 수 있습니다.

### `breathy-safe`
- breath / `h` / 약한 무성 계열을 좀 더 안전하게 다루는 프리셋입니다.
- 숨소리 음소가 깨지거나 노이즈화되는 음원에서 먼저 시도해볼 만합니다.

## Cache Compatibility Policy

Organum 캐시(`.ogc`/사용자 지정 확장자)는 아래 메타데이터가 현재 실행 환경과 일치할 때만 재사용됩니다.

- cache format version
- engine version (`CARGO_PKG_VERSION`)
- `sample_rate`
- `frame_period`

위 값이 하나라도 다르면 캐시는 비호환으로 판단되어 자동 재생성됩니다.
