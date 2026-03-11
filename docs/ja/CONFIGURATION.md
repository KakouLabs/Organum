# 設定

OrganumはYAML設定ファイルを使用して動作をカスタマイズできます。初回実行時に、実行ファイルと同じディレクトリに`organum.yaml`が自動生成されます。

[한국어](../CONFIGURATION.md) | [English](../en/CONFIGURATION.md) | [日本語](CONFIGURATION.md)

---

## パラメータ

| パラメータ | 型 | デフォルト | 説明 |
| :--- | :--- | :--- | :--- |
| `feature_extension` | 文字列 | `"ogc"` | キャッシュファイルの拡張子 (例: `ogc`, `llsm`) |
| `sample_rate` | 整数 | `44100` | 分析/合成のサンプリングレート (Hz) |
| `frame_period` | 浮動小数点 | `5.0` | WORLDのフレーム周期 (ms) |
| `zstd_compression_level` | 整数 | `3` | キャッシュの圧縮レベル (1-22) |
| `gpu_warp_enabled` | 真偽値 | `false` | `warp_spectrum`の実験的なGPUパスを使用するかどうか |
| `gpu_warp_min_frames` | 整数 | `2048` | GPUパスを試行する最小レンダリングフレーム長 |
| `output_dither` | 真偽値 | `true` | WAV出力時にディザリング/ノイズシェーピングを使用するかどうか |

> 注: GPUパスは実験的な機能です。現在の実装特性（アップロード/リードバックのオーバーヘッド）により、一般的なレンダリングではCPU（SIMD/並列）パスよりも遅くなる可能性があります。デフォルト値（`false`）のままにすることを推奨します。

## 例

```yaml
feature_extension: "ogc"
sample_rate: 44100
frame_period: 5.0
zstd_compression_level: 3
gpu_warp_enabled: false
gpu_warp_min_frames: 2048
output_dither: true
```

## キャッシュ互換性ポリシー

Organumのキャッシュ（`.ogc`またはカスタム拡張子）は、以下のメタデータが現在の実行環境と一致する場合にのみ再利用されます。

- キャッシュフォーマットバージョン
- キャッシュスキーマバージョン
- エンジンバージョン (`CARGO_PKG_VERSION`)
- `sample_rate`
- `frame_period`

上記のいずれかの値が異なる場合、キャッシュは非互換と判断され、自動的に再生成されます。
