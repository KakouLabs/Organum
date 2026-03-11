# Engine Comparison

Comparison of Organum with other UTAU/OpenUtau engines.

[한국어](../COMPARISON.md) | [English](COMPARISON.md) | [日本語](../ja/COMPARISON.md)

---

## Resampling Speed

Based on Kasane Teto UTAU voicebank, processing time for approx. 500ms segment.

> [!NOTE]
> The figures below are representative examples, and actual performance may vary depending on hardware, settings, and input length. Please refer to the benchmark summary assets in the releases for the latest figures.

| Engine | Language | Multithreading | Avg. Time |
| :--- | :--- | :--- | :--- |
| Organum | Rust | Yes (Rayon) | ~25ms |
| straycat-rs | Rust | Yes | ~35ms |
| tn_fnds | C++ | No | ~110ms |

## Audio Samples

Comparison of samples rendered with Kasane Teto (UTAU).

### Test 1 — Vowel /a/
| Engine | Sample |
| :--- | :--- |
| Organum | [Listen](../../audios/test1_Organum.mp3) |
| straycat-rs | [Listen](../../audios/test1_straycat-rs.mp3) |
| tn_fnds | [Listen](../../audios/test1_tn_fnds.mp3) |

### Test 2 — Consonant /k/
| Engine | Sample |
| :--- | :--- |
| Organum | [Listen](../../audios/test2_Organum.mp3) |
| straycat-rs | [Listen](../../audios/test2_straycat-rs.mp3) |
| tn_fnds | [Listen](../../audios/test2_tn_fnds.mp3) |

### Test 3 — Extreme Pitch Bend
| Engine | Sample |
| :--- | :--- |
| Organum | [Listen](../../audios/test3_Organum.mp3) |
| straycat-rs | [Listen](../../audios/test3_straycat-rs.mp3) |
| tn_fnds | [Listen](../../audios/test3_tn_fnds.mp3) |

### Test 4 — Gender / Breath Flags
| Engine | Gender (g+15) | Breath (B50) |
| :--- | :--- | :--- |
| Organum | [Listen](../../audios/test4_gender15_Organum_gender15.mp3) | [Listen](../../audios/test4_breath50_Organum_breath50.mp3) |
| straycat-rs | [Listen](../../audios/test4_gender15_straycat-rs_gender15.mp3) | [Listen](../../audios/test4_breath50_straycat-rs_breath50.mp3) |
| tn_fnds | [Listen](../../audios/test4_gender15_tn_fnds_gender15.mp3) | [Listen](../../audios/test4_breath50_tn_fnds_breath50.mp3) |

### Test 5 — Concatenation
| Engine | Sample |
| :--- | :--- |
| Organum | [Listen](../../audios/test5_Organum.mp3) |
| straycat-rs | [Listen](../../audios/test5_straycat-rs.mp3) |
| tn_fnds | [Listen](../../audios/test5_tn_fnds.mp3) |

---

## Feature Comparison

| Feature | Organum | straycat-rs | tn_fnds |
| :--- | :--- | :--- | :--- |
| Resampler | organum-resampler | straycat-rs | tn_fnds |
| Wavtool | organum-wavtool | convergence | convergence |
| Acoustic Model | WORLD | WORLD | WORLD/Classic |
| Configuration | YAML | TOML | CLI Only |
| License | MIT | MIT | GPL |

| Feature | Organum-Wavtool | Convergence |
| :--- | :--- | :--- |
| Interpolation | Cubic | Linear |
| Compression | Soft-knee Limiter | None |

> [!NOTE]
> Values may vary depending on hardware and settings.
