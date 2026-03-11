# Flags Reference

List of resampler flags supported by Organum. These can be used case-insensitively.

[한국어](../FLAGS.md) | [English](FLAGS.md) | [日本語](../ja/FLAGS.md)

---

## Flag List

| Flag | Name | Range | Neutral | Description |
| :--- | :--- | :--- | :--- | :--- |
| `g` | Gender | -100 ~ 100 | 0 | Formant shift. Positive for lower, negative for higher. |
| `B` | Breathiness | 0 ~ 100 | 50 | Degree of noise (aperiodicity). 50 is original, 100 is whisper, 0 is metallic. |
| `M` | Modulation | 0 ~ 100 | 100 | Preservation ratio of original pitch fluctuation (vibrato). |
| `t` | Tone Offset | -∞ ~ ∞ | 0 | Pitch fine-tuning (in cents, 100 = 1 semitone). |
| `A` | Amplitude | 0 ~ 200 | 100 | Volume. 100 is unity gain. |
| `P` | Peak | 0 ~ 100 | 100 | Peak normalization level. Defaults to 0.99 if unspecified. |
| `y` | Peak (alias) | 0 ~ 100 | 100 | Alias flag for Peak, same as `P`. |
| `C` | Clarity | 0 ~ 100 | 0 | Reduces noise floor in unvoiced sections. |
| `H` | Harmonics | 0 ~ 100 | 0 | Enhances harmonics by reducing aperiodicity in voiced frames. |
| `D` | Dynamics | 0 ~ 100 | 0 | Dynamic range compression. |
| `F` | Formant Shift | -24 ~ 24 | 0 | Formant shift in semitones. Independent from `g`. |

---

## Details

### B (Breathiness)

`B50` is neutral. `B > 50` adds noise towards 1.0, `B < 50` reduces original noise.

### g (Gender)

Frequency domain warping: `factor = 2^(g/100)`

### F (Formant Shift)

Semitone-based: `factor = 2^(F/12)`. `F12` raises formants by one octave.

### H (Harmonics)

Reduces aperiodicity in voiced frames using a quadratic curve: `ap *= 1 - (H/100)²`

### C (Clarity)

Reduces noise floor in unvoiced/silent sections: `ap *= 1 - (C/100)`

### D (Dynamics)

Applies a soft-knee compressor. As D increases, the threshold decreases (1.0 → 0.2) and the ratio increases (1:1 → 1:4). Applied before volume/peak normalization.

### P / y (Peak)

`P` and `y` set the same Peak parameter. The last parsed value is applied as the final value.

### Auto-Breath

Regardless of the `B` flag, a 50ms breath fade-in is automatically applied to consonant onsets.

---

## Usage

Enter directly into the Flags field in OpenUtau. Multiple flags can be combined (e.g., `g+10B60A120C30H50`).
