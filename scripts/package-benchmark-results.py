#!/usr/bin/env python3
from __future__ import annotations

import argparse
import math
import random
import shutil
import subprocess
import sys
import wave
from array import array
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, List, Tuple


def find_latest_bench_pairs(root: Path) -> List[Path]:
    if not root.exists():
        return []

    per_dir: Dict[Path, Tuple[Path, float]] = {}
    for p in root.rglob("bench-*.summary.txt"):
        stem = p.name.replace(".summary.txt", "")
        log = p.with_name(f"{stem}.txt")
        if not log.exists():
            continue
        mtime = p.stat().st_mtime
        key = p.parent
        prev = per_dir.get(key)
        if prev is None or mtime > prev[1]:
            per_dir[key] = (p, mtime)

    picked: List[Path] = []
    for base, (summary, _) in sorted(per_dir.items()):
        stem = summary.name.replace(".summary.txt", "")
        log = summary.with_name(f"{stem}.txt")
        picked.extend([summary, log])
    return picked


def copy_files(files: List[Path], root: Path, target: Path) -> List[Path]:
    copied: List[Path] = []
    for src in files:
        rel = src.relative_to(root)
        dst = target / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
        copied.append(dst)
    return copied


def write_manifest(version_dir: Path, copied: List[Path], version: str) -> Path:
    manifest = version_dir / "README.md"
    now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M:%S UTC")
    lines = [
        f"# Benchmark Results - {version}",
        "",
        f"- Generated at: {now}",
        f"- File count: {len(copied)}",
        "",
        "## Files",
        "",
    ]

    for p in sorted(copied):
        rel = p.relative_to(version_dir).as_posix()
        lines.append(f"- `{rel}`")

    manifest.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return manifest


def run_cmd(command: List[str], cwd: Path) -> None:
    print("RUN:", " ".join(command))
    subprocess.run(command, cwd=str(cwd), check=True)


def run_benchmark_generators(
    root: Path,
    samples: str,
    resampler_cmd: str,
    length_ms: int,
    repeats: int,
    run_validation: bool,
    run_perf: bool,
) -> None:
    py = sys.executable

    if run_validation:
        run_cmd(
            [
                py,
                "scripts/run-simd-validation.py",
                "--samples",
                samples,
                "--out-dir",
                "simd-validation",
                "--resampler-cmd",
                resampler_cmd,
                "--length-ms",
                str(length_ms),
            ],
            root,
        )

    if run_perf:
        run_cmd(
            [
                py,
                "scripts/run-simd-perf-bench.py",
                "--samples",
                samples,
                "--out-dir",
                "simd-bench",
                "--resampler-cmd",
                resampler_cmd,
                "--length-ms",
                str(length_ms),
                "--repeats",
                str(repeats),
            ],
            root,
        )


def write_sine_wav(path: Path, sample_rate: int, duration_sec: float, freq: float, noise: bool) -> None:
    n = int(sample_rate * duration_sec)
    buf = array("h")
    for i in range(n):
        t = i / sample_rate
        v = 0.5 * math.sin(2.0 * math.pi * freq * t)
        if noise:
            v += 0.1 * (random.random() * 2.0 - 1.0)
        s = int(max(-1.0, min(1.0, v)) * 32767)
        buf.append(s)

    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), "wb") as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(sample_rate)
        wf.writeframes(buf.tobytes())


def ensure_samples_file(root: Path, samples_rel: str) -> Path:
    samples = Path(samples_rel)
    samples_path = samples if samples.is_absolute() else (root / samples).resolve()
    if samples_path.exists():
        return samples_path

    sample_rate = 44100
    specs = [
        ("short_01.wav", 0.5, 440.0, False),
        ("short_02.wav", 0.5, 523.25, False),
        ("short_03.wav", 0.5, 659.25, False),
        ("medium_01.wav", 2.0, 440.0, False),
        ("medium_02.wav", 2.0, 523.25, False),
        ("medium_03.wav", 2.0, 659.25, False),
        ("long_01.wav", 5.0, 440.0, False),
        ("long_02.wav", 5.0, 523.25, False),
        ("long_03.wav", 5.0, 659.25, False),
        ("extreme_high_noise.wav", 2.0, 2000.0, True),
    ]

    base = root / "audios" / "simd_test"
    paths: List[Path] = []
    for name, duration, freq, noise in specs:
        p = base / name
        write_sine_wav(p, sample_rate, duration, freq, noise)
        paths.append(p)

    samples_path.parent.mkdir(parents=True, exist_ok=True)
    lines = [p.relative_to(root).as_posix() for p in paths]
    samples_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"Generated samples list: {samples_path}")
    return samples_path


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run benchmark scripts and collect outputs into benchmarks/<version>"
    )
    parser.add_argument("--version", required=True, help="Release version (example: v0.0.4)")
    parser.add_argument(
        "--root",
        default=".",
        help="Repository root (default: current directory)",
    )
    parser.add_argument(
        "--clean",
        action="store_true",
        help="Delete benchmarks/<version> before copying",
    )
    parser.add_argument(
        "--samples",
        default="samples.txt",
        help="Samples list for benchmark scripts",
    )
    parser.add_argument(
        "--resampler-cmd",
        default="target/release/organum-resampler",
        help="Resampler command passed to benchmark scripts",
    )
    parser.add_argument(
        "--length-ms",
        type=int,
        default=500,
        help="length_req ms for validation/perf scripts",
    )
    parser.add_argument(
        "--repeats",
        type=int,
        default=3,
        help="repeat count for perf benchmark script",
    )
    parser.add_argument(
        "--skip-validation",
        action="store_true",
        help="Skip running SIMD validation script",
    )
    parser.add_argument(
        "--skip-perf",
        action="store_true",
        help="Skip running SIMD performance script",
    )
    args = parser.parse_args()

    root = Path(args.root).resolve()
    version_dir = root / "benchmarks" / args.version
    samples_path = ensure_samples_file(root, args.samples)

    run_validation = not args.skip_validation
    run_perf = not args.skip_perf
    if not run_validation and not run_perf:
        raise SystemExit("Both validation and perf runs are disabled.")

    run_benchmark_generators(
        root=root,
        samples=str(samples_path),
        resampler_cmd=args.resampler_cmd,
        length_ms=args.length_ms,
        repeats=args.repeats,
        run_validation=run_validation,
        run_perf=run_perf,
    )

    if args.clean and version_dir.exists():
        shutil.rmtree(version_dir)

    files: List[Path] = []

    simd_bench = root / "simd-bench"
    if simd_bench.exists():
        files.extend(sorted(simd_bench.glob("*.csv")))
        files.extend(sorted(simd_bench.glob("*.md")))

    simd_validation = root / "simd-validation"
    if simd_validation.exists():
        files.extend(sorted(simd_validation.glob("*.csv")))
        files.extend(sorted(simd_validation.glob("*.md")))

    files.extend(find_latest_bench_pairs(root / "bench-results"))

    # De-duplicate while preserving order.
    deduped: List[Path] = []
    seen = set()
    for f in files:
        if f in seen or not f.exists():
            continue
        seen.add(f)
        deduped.append(f)

    if not deduped:
        raise SystemExit("No benchmark/validation result files found.")

    copied = copy_files(deduped, root, version_dir)
    manifest = write_manifest(version_dir, copied, args.version)

    print(f"DONE: {version_dir}")
    print(f"MANIFEST: {manifest}")
    for p in copied:
        print(f"- {p.relative_to(root).as_posix()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
