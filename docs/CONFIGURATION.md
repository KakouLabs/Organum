# Configuration

Organum은 YAML 설정 파일로 동작을 커스터마이즈할 수 있습니다. 첫 실행 시 `organum.yaml`이 실행 파일과 같은 디렉토리에 자동 생성됩니다.

## Parameters

| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `feature_extension` | string | `"ogc"` | 캐시 파일 확장자 (예: `ogc`, `llsm`) |
| `sample_rate` | integer | `44100` | 분석/합성 샘플레이트 (Hz) |
| `frame_period` | float | `5.0` | WORLD 프레임 주기 (ms) |
| `zstd_compression_level` | integer | `3` | 캐시 압축 레벨 (1-22) |
| `gpu_warp_enabled` | boolean | `false` | `warp_spectrum` 실험 GPU 경로 사용 여부 |
| `gpu_warp_min_frames` | integer | `2048` | GPU 경로를 시도할 최소 렌더 프레임 길이 |

## Example

```yaml
feature_extension: "ogc"
sample_rate: 44100
frame_period: 5.0
zstd_compression_level: 3
gpu_warp_enabled: false
gpu_warp_min_frames: 2048
```

## Cache Compatibility Policy

Organum 캐시(`.ogc`/사용자 지정 확장자)는 아래 메타데이터가 현재 실행 환경과 일치할 때만 재사용됩니다.

- cache format version
- cache schema version
- engine version (`CARGO_PKG_VERSION`)
- `sample_rate`
- `frame_period`

위 값이 하나라도 다르면 캐시는 비호환으로 판단되어 자동 재생성됩니다.
