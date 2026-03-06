<div align="center">
  <h1>Organum</h1>
  <p>UTAU resampler engine written in Rust</p>

  <img src="https://img.shields.io/github/v/release/KakouLabs/Organum?style=flat-square" alt="Latest Release">
</div>

---

Organum is a resampler engine for UTAU and OpenUtau. WORLD vocoder 기반의 분석/합성 파이프라인을 Rust로 구현했습니다.

## Features

- WORLD vocoder 기반 spectral analysis & synthesis
- Rayon을 이용한 병렬 처리
- `organum.yaml`을 통한 설정 커스터마이징
- Zstd 압축 캐시 (`.ogc`)로 반복 분석 생략

## Installation

1. [Releases](https://github.com/KakouLabs/Organum/releases) 페이지에서 바이너리를 다운로드합니다.
2. OpenUtau의 `Resamplers` 디렉토리에 배치합니다.

## Usage

OpenUtau 또는 UTAU에서:

1. `organum-resampler`를 Resampler로 설정
2. `organum-wavtool`을 Wavtool로 설정

### Logging

세 바이너리 모두 구조화 로그를 지원합니다.

- `--verbose`: 디버그 레벨 로그 활성화
- `--log-format pretty|json`: 로그 출력 형식 선택

예시:

```powershell
./organum-resampler --verbose --log-format json ...
./organum-wavtool --log-format json ...
./caching-tool.exe --verbose --log-format json "C:\Path\To\Your\Voicebank"
```

### Voicebank 캐싱

캐싱 툴로 voicebank를 미리 분석해두면 렌더링 시 분석 단계를 건너뜁니다.

```powershell
./caching-tool.exe "C:\Path\To\Your\Voicebank"
```

## Configuration

실행 시 `organum.yaml`이 없으면 기본값으로 자동 생성됩니다.

```yaml
feature_extension: "ogc"
sample_rate: 44100
frame_period: 5.0
zstd_compression_level: 3
```

자세한 내용은 [Configuration Guide](docs/CONFIGURATION.md) 참고.

## Build

Organum은 단일 릴리스 프로파일(`release`)을 사용합니다.

```bash
cargo build --workspace --release
```

```powershell
./build.bat
```

```bash
./build.sh
```

> [!IMPORTANT]
> OpenUtau는 `.ogc` 확장자를 기본 지원하지 않습니다. `feature_extension`을 `llsm`으로 변경하면 OpenUtau의 캐시 관리와 호환됩니다.

## Comparison

Kasane Teto UTAU voicebank 기준, 약 500ms 세그먼트 처리 시간.

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

오디오 샘플 비교는 [Comparison](docs/COMPARISON.md) 참고.

## Flags

렌더링 파라미터를 플래그로 제어할 수 있습니다. 상세 레퍼런스: [Flags](docs/FLAGS.md)

## License

MIT
