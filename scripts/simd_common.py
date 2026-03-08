from __future__ import annotations

import os
import re
import shlex
import subprocess
from pathlib import Path
from typing import Dict, List


ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")
TOTAL_RE = re.compile(r"total\s+([0-9]+(?:\.[0-9]+)?)(ms|s)\)")


def read_feature_extension(config_path: Path) -> str:
    if not config_path.exists():
        return "ogc"
    text = config_path.read_text(encoding="utf-8", errors="replace")
    m = re.search(r"^\s*feature_extension\s*:\s*\"?([A-Za-z0-9_\-]+)\"?\s*$", text, re.M)
    return m.group(1) if m else "ogc"


def cache_path_for_wav(wav_path: Path, ext: str) -> Path:
    return wav_path.with_suffix(f".wav.{ext}")


def parse_cmd(raw: str) -> List[str]:
    return shlex.split(raw, posix=(os.name != "nt"))


def resolve_resampler_cmd(root: Path, raw_cmd: str) -> List[str]:
    cmd = parse_cmd(raw_cmd)
    if cmd and cmd[0].endswith("organum-resampler") and os.name == "nt":
        exe = Path(cmd[0])
        if not exe.suffix and (root / (str(exe) + ".exe")).exists():
            cmd[0] = str(root / (str(exe) + ".exe"))
    return cmd


def load_samples(root: Path, samples_path: Path) -> List[Path]:
    path = samples_path if samples_path.is_absolute() else (root / samples_path).resolve()
    if not path.exists():
        raise RuntimeError(f"samples file not found: {path}")

    samples: List[Path] = []
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        p = Path(s)
        if not p.is_absolute():
            p = (root / p).resolve()
        if not p.exists():
            raise RuntimeError(f"sample not found: {p}")
        samples.append(p)

    if not samples:
        raise RuntimeError("no samples in samples file")
    return samples


def run_resampler_capture_total_ms(
    cmd: List[str],
    input_wav: Path,
    output_wav: Path,
    env_extra: Dict[str, str],
    length_req_ms: int,
) -> float:
    output_wav.parent.mkdir(parents=True, exist_ok=True)
    args = cmd + [
        str(input_wav),
        str(output_wav),
        "C4",
        "100",
        "-",
        "0",
        str(length_req_ms),
        "0",
        "0",
        "100",
        "0",
        "!120",
    ]

    env = os.environ.copy()
    env.update(env_extra)

    proc = subprocess.run(args, env=env, capture_output=True, text=True)
    out = (proc.stdout or "") + "\n" + (proc.stderr or "")
    if proc.returncode != 0:
        raise RuntimeError(f"resampler failed for {input_wav}:\n{out}")

    clean = ANSI_RE.sub("", out)
    matches = TOTAL_RE.findall(clean)
    if not matches:
        raise RuntimeError(f"failed to parse total ms from output for {input_wav}:\n{clean}")
    value, unit = matches[-1]
    v = float(value)
    return v * 1000.0 if unit == "s" else v


def run_resampler(
    cmd: List[str],
    input_wav: Path,
    output_wav: Path,
    env_extra: Dict[str, str],
    length_req_ms: int,
) -> None:
    output_wav.parent.mkdir(parents=True, exist_ok=True)
    args = cmd + [
        str(input_wav),
        str(output_wav),
        "C4",
        "100",
        "-",
        "0",
        str(length_req_ms),
        "0",
        "0",
        "100",
        "0",
        "!120",
    ]

    env = os.environ.copy()
    env.update(env_extra)
    subprocess.run(args, check=True, env=env)
