# SIMD Validation 실행 가이드

`scripts/run-simd-validation.py`를 사용해 A/B/C/D 검증 매트릭스를 자동 실행할 수 있습니다.

## 준비물

- `samples.txt` (한 줄에 wav 경로 하나)
- `target/release/organum-resampler` 빌드 완료

예시:

```text
audios/test01.wav
audios/test02.wav
```

## 1) 빌드

```bash
cargo build --release
```

## 2) 검증 실행

```bash
python scripts/run-simd-validation.py --samples samples.txt
```

Windows에서 확실히 지정하려면:

```powershell
python scripts/run-simd-validation.py --samples samples.txt --resampler-cmd "target/release/organum-resampler.exe"
```

기본 출력 경로:

- `benchmarks/<현재 버전>/simd-validation/metrics.csv`
- `benchmarks/<현재 버전>/simd-validation/metrics_summary.md`

## 3) 결과 확인

- 수치 상세 CSV: `benchmarks/<현재 버전>/simd-validation/metrics.csv`
- 요약 문서: `benchmarks/<현재 버전>/simd-validation/metrics_summary.md`

## 참고

- SIMD 토글은 `ORGANUM_AP_SIMD=on|off|auto`를 사용합니다.
- 본 스크립트는 길이/RMS/Peak/null-test를 자동 측정합니다.
- F0 통계/청감 평가는 프로젝트 릴리즈 검증 절차에 맞춰 별도 진행하세요.
