#!/usr/bin/env python3
import argparse
import csv
import math
import sys
import wave
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Tuple
from simd_common import (
    cache_path_for_wav,
    load_samples,
    read_feature_extension,
    resolve_resampler_cmd,
    run_resampler,
)


@dataclass
class AudioStats:
    sample_rate: int
    channels: int
    length_samples: int
    rms: float
    peak: float


@dataclass
class Case:
    name: str
    cache_on: bool
    simd: str

def pcm16_stats(wav_path: Path) -> AudioStats:
    with wave.open(str(wav_path), "rb") as wf:
        channels = wf.getnchannels()
        sample_rate = wf.getframerate()
        sampwidth = wf.getsampwidth()
        frames = wf.getnframes()
        if sampwidth != 2:
            raise RuntimeError(f"Only PCM16 is supported: {wav_path}")
        raw = wf.readframes(frames)

    total = frames * channels
    if total == 0:
        return AudioStats(sample_rate, channels, 0, 0.0, 0.0)

    import array

    arr = array.array("h")
    arr.frombytes(raw)

    sq = 0.0
    peak = 0.0
    for s in arr:
        v = s / 32768.0
        av = abs(v)
        if av > peak:
            peak = av
        sq += v * v

    rms = math.sqrt(sq / len(arr)) if arr else 0.0
    return AudioStats(sample_rate, channels, frames, rms, peak)


def null_stats(a_wav: Path, b_wav: Path) -> Tuple[float, float]:
    import array

    with wave.open(str(a_wav), "rb") as wa, wave.open(str(b_wav), "rb") as wb:
        if wa.getsampwidth() != 2 or wb.getsampwidth() != 2:
            raise RuntimeError("Only PCM16 is supported for null test")
        if wa.getnchannels() != wb.getnchannels():
            raise RuntimeError("Channel mismatch in null test")

        na = wa.getnframes() * wa.getnchannels()
        nb = wb.getnframes() * wb.getnchannels()
        n = min(na, nb)

        aa = array.array("h")
        bb = array.array("h")
        aa.frombytes(wa.readframes(wa.getnframes()))
        bb.frombytes(wb.readframes(wb.getnframes()))

    if n == 0:
        return 0.0, 0.0

    sq = 0.0
    peak = 0.0
    for i in range(n):
        d = (aa[i] - bb[i]) / 32768.0
        ad = abs(d)
        if ad > peak:
            peak = ad
        sq += d * d
    rms = math.sqrt(sq / n)
    return rms, peak


def main() -> int:
    p = argparse.ArgumentParser(description="Run SIMD A/B/C/D validation matrix")
    p.add_argument("--samples", required=True, help="Path to samples.txt")
    p.add_argument("--out-dir", default="simd-validation", help="Output directory")
    p.add_argument("--length-ms", type=int, default=500, help="Render length_req in ms")
    p.add_argument(
        "--resampler-cmd",
        default="target/release/organum-resampler",
        help="Resampler command prefix (example: 'target/release/organum-resampler' or 'cargo run --release --bin organum-resampler --')",
    )
    p.add_argument("--length-max", type=int, default=2)
    p.add_argument("--rms-rel-max", type=float, default=0.005)
    p.add_argument("--peak-abs-max", type=float, default=0.005)
    p.add_argument("--null-rms-max", type=float, default=0.005)
    args = p.parse_args()

    root = Path.cwd()
    out_dir = (root / args.out_dir).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    samples = load_samples(root, Path(args.samples))

    config_path = root / "organum.yaml"
    ext = read_feature_extension(config_path)

    cmd = resolve_resampler_cmd(root, args.resampler_cmd)

    cases = [
        Case("A_cache_off_simd_off", cache_on=False, simd="off"),
        Case("B_cache_off_simd_on", cache_on=False, simd="on"),
        Case("C_cache_on_simd_off", cache_on=True, simd="off"),
        Case("D_cache_on_simd_on", cache_on=True, simd="on"),
    ]

    stats: Dict[Tuple[str, str], AudioStats] = {}
    out_wavs: Dict[Tuple[str, str], Path] = {}

    for sample in samples:
        cache_path = cache_path_for_wav(sample, ext)
        for case in cases:
            if not case.cache_on and cache_path.exists():
                cache_path.unlink()

            if case.cache_on and not cache_path.exists():
                warmup_out = out_dir / "warmup" / f"{sample.stem}.wav"
                run_resampler(
                    cmd,
                    sample,
                    warmup_out,
                    {"ORGANUM_AP_SIMD": "off"},
                    args.length_ms,
                )

            out_wav = out_dir / case.name / f"{sample.stem}.wav"
            run_resampler(
                cmd,
                sample,
                out_wav,
                {"ORGANUM_AP_SIMD": case.simd},
                args.length_ms,
            )

            out_wavs[(sample.name, case.name)] = out_wav
            stats[(sample.name, case.name)] = pcm16_stats(out_wav)

    csv_path = out_dir / "metrics.csv"
    md_path = out_dir / "metrics_summary.md"

    with csv_path.open("w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(
            [
                "sample",
                "case",
                "length_samples",
                "rms",
                "peak",
                "delta_length_vs_A",
                "rms_rel_err_vs_A",
                "peak_abs_err_vs_A",
                "null_rms_vs_A",
                "null_peak_vs_A",
                "pass",
            ]
        )

        for sample in samples:
            base_case = "A_cache_off_simd_off"
            base = stats[(sample.name, base_case)]
            base_wav = out_wavs[(sample.name, base_case)]

            for case in cases:
                cur = stats[(sample.name, case.name)]
                cur_wav = out_wavs[(sample.name, case.name)]

                dlen = abs(cur.length_samples - base.length_samples)
                rms_rel = (
                    abs(cur.rms - base.rms) / max(base.rms, 1e-12)
                    if case.name != base_case
                    else 0.0
                )
                peak_abs = abs(cur.peak - base.peak)
                nrms, npeak = null_stats(base_wav, cur_wav)

                ok = (
                    dlen <= args.length_max
                    and rms_rel <= args.rms_rel_max
                    and peak_abs <= args.peak_abs_max
                    and nrms <= args.null_rms_max
                )
                w.writerow(
                    [
                        sample.name,
                        case.name,
                        cur.length_samples,
                        f"{cur.rms:.8f}",
                        f"{cur.peak:.8f}",
                        dlen,
                        f"{rms_rel:.8f}",
                        f"{peak_abs:.8f}",
                        f"{nrms:.8f}",
                        f"{npeak:.8f}",
                        "PASS" if ok else "FAIL",
                    ]
                )

    rows = csv_path.read_text(encoding="utf-8").splitlines()
    with md_path.open("w", encoding="utf-8") as out:
        out.write("# SIMD Validation Summary\n\n")
        out.write(f"- Samples: {len(samples)}\n")
        out.write(f"- Matrix cases: {len(cases)} (A/B/C/D)\n")
        out.write(f"- Metrics CSV: `{csv_path}`\n")
        out.write("- Thresholds:\n")
        out.write(f"  - length_max: {args.length_max}\n")
        out.write(f"  - rms_rel_max: {args.rms_rel_max}\n")
        out.write(f"  - peak_abs_max: {args.peak_abs_max}\n")
        out.write(f"  - null_rms_max: {args.null_rms_max}\n\n")
        out.write("## Quick Note\n\n")
        out.write("- This script validates length/RMS/peak/null metrics against case A baseline.\n")
        out.write("- F0 stats and listening-test results should be recorded separately.\n")

    print(f"DONE: {md_path}")
    print(f"DONE: {csv_path}")
    print(f"CSV rows: {len(rows) - 1}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
