# エンジン比較

Organumと他のUTAU/OpenUtauエンジンとの比較。

[한국어](../COMPARISON.md) | [English](../en/COMPARISON.md) | [日本語](COMPARISON.md)

---

## リサンプリング速度

重音テトUTAU音源基準、約500msセグメントの処理時間。

> [!NOTE]
> 以下の数値は代表的な例であり、実際のパフォーマンスはハードウェア、設定、入力の長さによって異なる場合があります。最新の数値については、リリースのベン치마크 요약/로그 아셋을 참고하세요.

| エンジン | 言語 | マルチスレッド | 平均時間 |
| :--- | :--- | :--- | :--- |
| Organum | Rust | 対応 (Rayon) | ~25ms |
| straycat-rs | Rust | 対応 | ~35ms |
| tn_fnds | C++ | 非対応 | ~110ms |

## オーディオサンプル

重音テト (UTAU) でレンダリングしたサンプルの比較。

### テスト 1 — 母音 /a/
| エンジン | サンプル |
| :--- | :--- |
| Organum | [聴く](../../audios/test1_Organum.mp3) |
| straycat-rs | [聴く](../../audios/test1_straycat-rs.mp3) |
| tn_fnds | [聴く](../../audios/test1_tn_fnds.mp3) |

### テスト 2 — 子音 /k/
| エンジン | サンプル |
| :--- | :--- |
| Organum | [聴く](../../audios/test2_Organum.mp3) |
| straycat-rs | [聴く](../../audios/test2_straycat-rs.mp3) |
| tn_fnds | [聴く](../../audios/test2_tn_fnds.mp3) |

### テスト 3 — 極端なピッチベンド
| エンジン | サンプル |
| :--- | :--- |
| Organum | [聴く](../../audios/test3_Organum.mp3) |
| straycat-rs | [聴く](../../audios/test3_straycat-rs.mp3) |
| tn_fnds | [聴く](../../audios/test3_tn_fnds.mp3) |

### テスト 4 — ジェンダー / ブレスフラグ
| エンジン | ジェンダー (g+15) | ブレス (B50) |
| :--- | :--- | :--- |
| Organum | [聴く](../../audios/test4_gender15_Organum_gender15.mp3) | [聴く](../../audios/test4_breath50_Organum_breath50.mp3) |
| straycat-rs | [聴く](../../audios/test4_gender15_straycat-rs_gender15.mp3) | [聴く](../../audios/test4_breath50_straycat-rs_breath50.mp3) |
| tn_fnds | [聴く](../../audios/test4_gender15_tn_fnds_gender15.mp3) | [聴く](../../audios/test4_breath50_tn_fnds_breath50.mp3) |

### テスト 5 — 結合
| エンジン | サンプル |
| :--- | :--- |
| Organum | [聴く](../../audios/test5_Organum.mp3) |
| straycat-rs | [聴く](../../audios/test5_straycat-rs.mp3) |
| tn_fnds | [聴く](../../audios/test5_tn_fnds.mp3) |


---

## 機能比較

| 機能 | Organum | straycat-rs | tn_fnds |
| :--- | :--- | :--- | :--- |
| リサンプラー | organum-resampler | straycat-rs | tn_fnds |
| Wavtool | organum-wavtool | convergence | convergence |
| 音響モデル | WORLD | WORLD | WORLD/Classic |
| 設定形式 | YAML | TOML | CLIのみ |
| ライセンス | MIT | MIT | GPL |

| 機能 | Organum-Wavtool | Convergence |
| :--- | :--- | :--- |
| 補間方式 | 立方補間 (Cubic) | 線形補間 (Linear) |
| 圧縮 | ソフトニーリミッター | なし |

> [!NOTE]
> ハードウェアや設定によって数値が異なる場合があります。
