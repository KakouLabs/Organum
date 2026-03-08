#!/usr/bin/env python3
import argparse
import csv
import platform
import statistics
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Tuple
from simd_common import (
    cache_path_for_wav,
    load_samples,
    read_feature_extension,
    resolve_resampler_cmd,
    run_resampler_capture_total_ms,
)


@dataclass
class Case:
    name: str
    cache_on: bool
    simd: str

def p95(values: List[float]) -> float:
    if not values:
        return 0.0
    xs = sorted(values)
    idx = int((len(xs) * 0.95) // 1)
    idx = min(idx, len(xs) - 1)
    return xs[idx]

def main() -> int:
    p = argparse.ArgumentParser(description="Benchmark ORGANUM_AP_SIMD on/off and emit markdown summary")
    p.add_argument("--samples", default="samples.txt", help="Path to samples.txt")
    p.add_argument("--out-dir", default="simd-bench", help="Output directory")
    p.add_argument("--length-ms", type=int, default=500, help="Render length_req in ms")
    p.add_argument("--repeats", type=int, default=3, help="Runs per sample/case")
    p.add_argument(
        "--resampler-cmd",
        default="target/release/organum-resampler",
        help="Resampler command prefix",
    )
    args = p.parse_args()

    root = Path.cwd()
    out_dir = (root / args.out_dir).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    samples = load_samples(root, Path(args.samples))

    ext = read_feature_extension(root / "organum.yaml")
    cmd = resolve_resampler_cmd(root, args.resampler_cmd)

    cases = [
        Case("A_cache_off_simd_off", cache_on=False, simd="off"),
        Case("B_cache_off_simd_on", cache_on=False, simd="on"),
        Case("C_cache_on_simd_off", cache_on=True, simd="off"),
        Case("D_cache_on_simd_on", cache_on=True, simd="on"),
    ]

    raw_csv = out_dir / "perf_raw.csv"
    summary_csv = out_dir / "perf_summary.csv"
    md_path = out_dir / "perf_summary.md"

    rows: List[Tuple[str, str, int, float]] = []
    case_totals: Dict[str, List[float]] = {c.name: [] for c in cases}

    for sample in samples:
        cache_path = cache_path_for_wav(sample, ext)

        for case in cases:
            per_case_times: List[float] = []

            for rep in range(args.repeats):
                if not case.cache_on and cache_path.exists():
                    cache_path.unlink()

                if case.cache_on and not cache_path.exists():
                    warmup_out = out_dir / "warmup" / f"{sample.stem}.wav"
                    _ = run_resampler_capture_total_ms(
                        cmd,
                        sample,
                        warmup_out,
                        {"ORGANUM_AP_SIMD": "off"},
                        args.length_ms,
                    )

                out_wav = out_dir / case.name / f"{sample.stem}.r{rep + 1}.wav"
                t_ms = run_resampler_capture_total_ms(
                    cmd,
                    sample,
                    out_wav,
                    {"ORGANUM_AP_SIMD": case.simd},
                    args.length_ms,
                )
                rows.append((sample.name, case.name, rep + 1, t_ms))
                per_case_times.append(t_ms)
                case_totals[case.name].append(t_ms)

            print(
                f"{sample.name} {case.name}: median={statistics.median(per_case_times):.2f}ms p95={p95(per_case_times):.2f}ms"
            )

    with raw_csv.open("w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["sample", "case", "run", "total_ms"])
        w.writerows(rows)

    case_stats: Dict[str, Tuple[float, float, int]] = {}
    with summary_csv.open("w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["case", "runs", "median_ms", "p95_ms"])
        for case in cases:
            vals = case_totals[case.name]
            med = statistics.median(vals) if vals else 0.0
            p95v = p95(vals)
            case_stats[case.name] = (med, p95v, len(vals))
            w.writerow([case.name, len(vals), f"{med:.6f}", f"{p95v:.6f}"])

    a_med, a_p95, _ = case_stats["A_cache_off_simd_off"]
    b_med, b_p95, _ = case_stats["B_cache_off_simd_on"]
    c_med, c_p95, _ = case_stats["C_cache_on_simd_off"]
    d_med, d_p95, _ = case_stats["D_cache_on_simd_on"]

    def speedup(off: float, on: float) -> float:
        if off <= 0.0:
            return 0.0
        return (off - on) / off * 100.0

    off_med_speed = speedup(a_med, b_med)
    off_p95_speed = speedup(a_p95, b_p95)
    on_med_speed = speedup(c_med, d_med)
    on_p95_speed = speedup(c_p95, d_p95)

    with md_path.open("w", encoding="utf-8") as out:
        out.write("# SIMD Performance Summary\n\n")
        out.write(f"- Samples: {len(samples)}\n")
        out.write(f"- Repeats per sample/case: {args.repeats}\n")
        out.write(f"- Length(ms): {args.length_ms}\n")
        out.write(f"- Host: {platform.platform()}\n")
        out.write(f"- Python: {platform.python_version()}\n")
        out.write(f"- Raw CSV: `{raw_csv}`\n")
        out.write(f"- Summary CSV: `{summary_csv}`\n\n")

        out.write("## Case Stats\n\n")
        out.write("| case | runs | median_ms | p95_ms |\n")
        out.write("|---|---:|---:|---:|\n")
        for case in cases:
            med, p95v, n = case_stats[case.name]
            out.write(f"| {case.name} | {n} | {med:.3f} | {p95v:.3f} |\n")

        out.write("\n## SIMD On vs Off\n\n")
        out.write(f"- cache OFF median speedup (B vs A): {off_med_speed:+.2f}%\n")
        out.write(f"- cache OFF p95 speedup (B vs A): {off_p95_speed:+.2f}%\n")
        out.write(f"- cache ON median speedup (D vs C): {on_med_speed:+.2f}%\n")
        out.write(f"- cache ON p95 speedup (D vs C): {on_p95_speed:+.2f}%\n")

        off_reco = "SIMD ON" if (b_med <= a_med and b_p95 <= a_p95) else "SIMD OFF"
        on_reco = "SIMD ON" if (d_med <= c_med and d_p95 <= c_p95) else "SIMD OFF"
        out.write("\n## Recommendation\n\n")
        out.write(f"- cache OFF: {off_reco}\n")
        out.write(f"- cache ON: {on_reco}\n")

    print(f"DONE: {md_path}")
    print(f"DONE: {raw_csv}")
    print(f"DONE: {summary_csv}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
