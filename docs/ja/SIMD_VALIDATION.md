# SIMD検証実行ガイド

`scripts/run-simd-validation.py`を使用して、A/B/C/D検証マトリックスを自動実行できます。

[한국어](../SIMD_VALIDATION.md) | [English](../en/SIMD_VALIDATION.md) | [日本語](SIMD_VALIDATION.md)

---

## 準備するもの

- `samples.txt` (1行に1つのwavパス)
- `target/release/organum-resampler` ビルド完了

例:

```text
audios/test01.wav
audios/test02.wav
```

## 1) ビルド

```bash
cargo build --release
```

## 2) 検証の実行

```bash
python scripts/run-simd-validation.py --samples samples.txt
```

Windowsで明示的に指定する場合:

```powershell
python scripts/run-simd-validation.py --samples samples.txt --resampler-cmd "target/release/organum-resampler.exe"
```

デフォルトの出力パス:

- `benchmarks/<現在のバージョン>/simd-validation/metrics.csv`
- `benchmarks/<現在のバージョン>/simd-validation/metrics_summary.md`

## 3) 結果の確認

- 数値詳細 CSV: `benchmarks/<現在のバージョン>/simd-validation/metrics.csv`
- 要約ドキュメント: `benchmarks/<現在のバージョン>/simd-validation/metrics_summary.md`

## 参考

- SIMDの切り替えは `ORGANUM_AP_SIMD=on|off|auto` を使用します。
- 本スクリプトは、長さ/RMS/Peak/null-testを自動測定します。
- F0統計/聴感評価は、プロジェクトのリリース検証手順に合わせて別途実施してください。
