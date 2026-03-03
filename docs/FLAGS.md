# Flags Reference

Organum에서 지원하는 리샘플러 플래그 목록. 대소문자 구분 없이 사용 가능합니다.

## Flag List

| Flag | Name | Range | Neutral | Description |
| :--- | :--- | :--- | :--- | :--- |
| `g` | Gender | -100 ~ 100 | 0 | 포먼트 시프트. 양수면 낮게, 음수면 높게. |
| `B` | Breathiness | 0 ~ 100 | 50 | 노이즈(비주기성) 정도. 50이 원본, 100이면 위스퍼, 0이면 메탈릭. |
| `M` | Modulation | 0 ~ 100 | 100 | 원본 피치 변동(비브라토)의 보존 비율. |
| `t` | Tone Offset | -∞ ~ ∞ | 0 | 피치 미세 조정 (cent 단위, 100 = 1 반음) |
| `A` | Amplitude | 0 ~ 200 | 100 | 볼륨. 100이 unity gain. |
| `P` | Peak | 0 ~ 100 | 100 | 피크 노멀라이제이션 레벨. 미지정 시 0.99. |
| `C` | Clarity | 0 ~ 100 | 0 | 무성 구간의 노이즈 플로어 감소. |
| `H` | Harmonics | 0 ~ 100 | 0 | 유성 프레임에서 비주기성을 줄여 배음 강화. |
| `D` | Dynamics | 0 ~ 100 | 0 | 다이나믹 레인지 압축. |
| `F` | Formant Shift | -24 ~ 24 | 0 | 반음 단위 포먼트 시프트. `g`와 독립. |

---

## Details

### B (Breathiness)

`B50`이 중립. `B > 50`이면 노이즈를 1.0 쪽으로 추가, `B < 50`이면 원본 노이즈를 축소.

### g (Gender)

주파수 도메인 워핑: `factor = 2^(g/100)`

### F (Formant Shift)

반음 기반: `factor = 2^(F/12)`. `F12`는 포먼트 1옥타브 상승.

### H (Harmonics)

유성 프레임의 비주기성을 이차 곡선으로 감소: `ap *= 1 - (H/100)²`

### C (Clarity)

무성/무음 구간의 노이즈 플로어 감소: `ap *= 1 - (C/100)`

### D (Dynamics)

소프트-니 컴프레서 적용. D가 올라갈수록 threshold 하강(1.0→0.2), ratio 상승(1:1→1:4). 볼륨/피크 노멀라이제이션 전에 적용.

### Auto-Breath

`B` 플래그와 별개로, 자음 onset에 50ms breath fade-in을 자동 적용.

---

## Usage

OpenUtau의 Flags 필드에 직접 입력. 여러 플래그를 조합할 수 있음 (예: `g+10B60A120C30H50`).
