# Configuration

Organum은 YAML 설정 파일로 동작을 커스터마이즈할 수 있습니다. 첫 실행 시 `organum.yaml`이 실행 파일과 같은 디렉토리에 자동 생성됩니다.

## Parameters

| Parameter | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `feature_extension` | string | `"ogc"` | 캐시 파일 확장자 (예: `ogc`, `llsm`) |
| `sample_rate` | integer | `44100` | 분석/합성 샘플레이트 (Hz) |
| `frame_period` | float | `5.0` | WORLD 프레임 주기 (ms) |
| `zstd_compression_level` | integer | `3` | 캐시 압축 레벨 (1-22) |

## Example

```yaml
feature_extension: "ogc"
sample_rate: 44100
frame_period: 5.0
zstd_compression_level: 3
```

> [!TIP]
> OpenUtau와의 호환성을 위해 `feature_extension`을 `llsm`으로 설정하는 것을 권장합니다.
