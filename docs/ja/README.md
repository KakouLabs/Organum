<div align="center">
  <img src="../../assets/ORGANUM.png" width="500" />
  <h1>Organum</h1>
  <p>Rustで書かれたUTAUリサンプラーエンジン</p>

  <img src="https://img.shields.io/github/v/release/KakouLabs/Organum?style=flat-square" alt="Latest Release">
</div>

---

[한국어](../../README.md) | [English](../en/README.md) | [日本語](README.md)

OrganumはUTAUおよびOpenUtau用のリサンプラーエンジンです。WORLDボコーダーベースの分析・合成パイプラインをRustで実装しました。

## 主な機能

- WORLDボコーダーベースのスペクトル分析と合成
- Rayonを利用した並列処理
- `organum.yaml`による設定のカスタマイズ
- Zstd圧縮キャッシュ(`.ogc`)による重複分析の省略

## 🎤 Voicebank Release
**虚空の 鉄(코쿠노 테츠, Tetsu Kokuno)** ボイスバンクが発売されました！ダウンロードは[https://utau.sapo.dev](https://utau.sapo.dev)から入手できます。

## GPUルートに関する注意

- `gpu-warp`パスは**実験的機能**です。
- 現在の実装は、GPUの往復オーバヘッド（アップロード/リードバック）により、実用的な区間ではCPUパス（SIMD/並列）よりも**遅くなる可能性がある**ため、推奨されません。
- デフォルトでは無効化（`gpu_warp_enabled: false`）されており、特定のベンチマークや実験目的以外ではCPUパスの使用を推奨します。

## インストール

1. [Releases](https://github.com/KakouLabs/Organum/releases)ページからバイナリをダウンロードします。
   - 各プラットフォーム向けに`*-cpu`（標準CPUビルド）と`*-cpu-gpu`（`gpu-warp`機能を含む）アーカイブが提供されています。
   - `organum-wavtool`は現在Windows版のみ提供されています。Linux/macOS版アーカイブには`organum-resampler`と`caching-tool`のみ含まれます。
   - 最新リリースにはベンチマークの概要やログアセットが含まれる場合があります。
2. OpenUtauの`Resamplers`ディレクトリに配置します。

## 使い方

OpenUtauまたはUTAUにおいて:

1. `organum-resampler`をリサンプラーとして設定
2. Windowsでは`organum-wavtool`をWavtoolとして設定
3. Linux/macOSでは以下のいずれかを使用
   - `wine organum-wavtool.exe`でWindows版wavtoolを実行
   - OpenUtau標準のwavtoolを使用

### ロギング

3つのバイナリすべてで構造化ログをサポートしています。

- `--verbose`: デバッグレベルのログを有効化
- `--log-format pretty|json`: ログ出力形式の選択

例:

```powershell
./organum-resampler --verbose --log-format json ...
./organum-wavtool --log-format json ...
./caching-tool.exe --verbose --log-format json "C:\Path\To\Your\Voicebank"
```

```bash
wine organum-wavtool.exe --log-format json ...
```

### ボイスバンクのキャッシング

キャッシングツールでボイスバンクを事前に分析しておくことで、レンダリング時の分析ステップをスキップできます。

```powershell
./caching-tool.exe "C:\Path\To\Your\Voicebank"
```

## 設定

実行時に`organum.yaml`がない場合、ファイルは生成せず、組み込みのデフォルト値ですぐに実行されます。設定を変更するには、実行ファイルと同じディレクトリに`organum.yaml`を作成してください。

```yaml
feature_extension: "ogc"
sample_rate: 44100
frame_period: 5.0
zstd_compression_level: 3
compressor_threshold: 0.85
compressor_limit: 0.99
gpu_warp_enabled: false
gpu_warp_min_frames: 2048
```

キャッシュは、形式/スキーマ/エンジンバージョン、および主要な設定（`sample_rate`, `frame_period`）が現在の実行値と異なる場合、自動的に無効化され再生成されます。

詳細は [Configuration Guide](CONFIGURATION.md) を参照してください。

SIMD検証/ベンチマークガイドは [SIMD Validation](SIMD_VALIDATION.md)、flamegraphによるボトルネック分析は [Profiling](PROFILING.md) を参照してください。

## ビルド

Organumは単一のリリースプロファイル（`release`）を使用します。
OrganumはRust-native WORLD経路（`world::native`）を使用します。

```bash
cargo build --workspace --release
```

```powershell
./build.bat
```

```bash
./build.sh
```

## 比較

重音テトUTAU音源基準、約500msセグメントの処理時間。

| エンジン | 言語 | マルチスレッド | 平均時間 |
| :--- | :--- | :--- | :--- |
| Organum | Rust | 対応 (Rayon) | ~25ms |
| straycat-rs | Rust | 対応 | ~35ms |
| tn_fnds | C++ | 非対応 | ~110ms |

| 機能 | Organum | straycat-rs | tn_fnds |
| :--- | :--- | :--- | :--- |
| 音響モデル | WORLD | WORLD | WORLD/Classic |
| 設定形式 | YAML | TOML | CLIのみ |
| ライセンス | MIT | MIT | GPL |

オーディオサンプルの比較は [Comparison](COMPARISON.md) を参照してください。

## フラグ

レンダリングパラメータはフラグで制御できます。`P`と`y`は同一のPeakパラメータエイリアスです。詳細リファレンス: [Flags](FLAGS.md)

## ライセンス

OrganumはMIT Licenseで配布されています。詳細は [LICENSE](../../LICENSE) を参照してください。

Rust-native WORLD実装には、帰属表示および再配布要件に対応するため、元のWORLD BSD-style noticeを同梱しています。詳細は [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md) および `licenses/WORLD_BSD-3-Clause.txt` を参照してください。
