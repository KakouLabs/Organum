#!/usr/bin/env bash
set -euo pipefail

INFLIGHT_VALUES=(${INFLIGHT_VALUES:-1 2 3})
CHUNK_VALUES=(${CHUNK_VALUES:-auto 2048 4096})

read_version_tag() {
  local cargo_toml="Cargo.toml"
  if [[ ! -f "$cargo_toml" ]]; then
    printf "v0.0.0"
    return
  fi

  local ver
  ver="$(
    awk -F'"' '
      /^\[package\]/ { in_pkg=1; next }
      /^\[/ { if (in_pkg) exit }
      in_pkg && $0 ~ /^[[:space:]]*version[[:space:]]*=/ { print $2; exit }
    ' "$cargo_toml"
  )"

  if [[ -z "$ver" ]]; then
    printf "v0.0.0"
  elif [[ "$ver" == v* ]]; then
    printf "%s" "$ver"
  else
    printf "v%s" "$ver"
  fi
}

VERSION_TAG="${BENCH_VERSION:-$(read_version_tag)}"
OUTPUT_DIR="${OUTPUT_DIR:-benchmarks/$VERSION_TAG/bench-results}"
PROFILE="${PROFILE:-dev}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo not found. Install Rust toolchain first." >&2
  exit 1
fi

mkdir -p "$OUTPUT_DIR"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
RESULT_FILE="$OUTPUT_DIR/bench-$TIMESTAMP.txt"
SUMMARY_FILE="$OUTPUT_DIR/bench-$TIMESTAMP.summary.txt"

run_bench() {
  local bench_name="$1"
  local cmd=(cargo run --features gpu-warp --bin "$bench_name")
  if [[ "$PROFILE" == "release" ]]; then
    cmd=(cargo run --release --features gpu-warp --bin "$bench_name")
  fi
  {
    echo
    echo ">>> RUN: ${cmd[*]}"
    "${cmd[@]}"
  } | tee -a "$RESULT_FILE"
}

log_case_header() {
  local inflight="$1"
  local chunk="$2"
  {
    echo
    echo "============================================================"
    echo "CASE inflight=$inflight chunk=$chunk"
    echo "============================================================"
  } | tee -a "$RESULT_FILE"
}

set_chunk_env() {
  local chunk="$1"
  if [[ "$chunk" == "auto" ]]; then
    unset WARP_GPU_CHUNK_FRAMES || true
    echo "WARP_GPU_CHUNK_FRAMES=auto(full-frame)" | tee -a "$RESULT_FILE"
  else
    export WARP_GPU_CHUNK_FRAMES="$chunk"
    echo "WARP_GPU_CHUNK_FRAMES=$chunk" | tee -a "$RESULT_FILE"
  fi
}

run_case() {
  local inflight="$1"
  local chunk="$2"

  log_case_header "$inflight" "$chunk"

  export WARP_GPU_INFLIGHT="$inflight"
  export WARP_BENCH_GPU_MIN_FRAMES=1
  export AP_BENCH_GPU_MIN_FRAMES=1
  set_chunk_env "$chunk"

  run_bench "warp-bench"
  run_bench "ap-bench"
}

{
  echo "# Organum benchmark run"
  echo "# Timestamp: $TIMESTAMP"
  echo "# Profile: $PROFILE"
  echo "# Inflight values: ${INFLIGHT_VALUES[*]}"
  echo "# Chunk values: ${CHUNK_VALUES[*]}"
} | tee "$RESULT_FILE"

for inflight in "${INFLIGHT_VALUES[@]}"; do
  for chunk in "${CHUNK_VALUES[@]}"; do
    run_case "$inflight" "$chunk"
  done
done

echo
echo "Done. Log saved to: $RESULT_FILE" | tee -a "$RESULT_FILE"

python - "$RESULT_FILE" "$SUMMARY_FILE" <<'PY'
import re
import sys

src, dst = sys.argv[1], sys.argv[2]
case_re = re.compile(r"^CASE inflight=(\S+) chunk=(\S+)$")
run_re = re.compile(r"^>>> RUN: .*--bin\s+(warp-bench|ap-bench)")
ci_re = re.compile(
    r"^CI_SUMMARY,case=([^,]+),threshold=([^,]+),median_ratio=([^,]+),p95_ratio=([^,]+)$"
)

rows = []
cur_inflight = ""
cur_chunk = ""
cur_bench = ""

with open(src, "r", encoding="utf-8", errors="replace") as f:
    for line in f:
        line = line.rstrip("\n")
        m = case_re.match(line)
        if m:
            cur_inflight, cur_chunk = m.group(1), m.group(2)
            continue
        m = run_re.match(line)
        if m:
            cur_bench = "warp" if m.group(1) == "warp-bench" else "ap"
            continue
        m = ci_re.match(line)
        if m:
            rows.append(
                {
                    "bench": cur_bench,
                    "case": m.group(1),
                    "inflight": cur_inflight,
                    "chunk": cur_chunk,
                    "median": float(m.group(3)),
                    "p95": float(m.group(4)),
                }
            )

best = {}
for r in rows:
    key = (r["bench"], r["case"])
    if key not in best or (r["median"], r["p95"]) < (best[key]["median"], best[key]["p95"]):
        best[key] = r

ordered = sorted(best.values(), key=lambda x: (x["bench"], x["case"]))

with open(dst, "w", encoding="utf-8") as out:
    out.write("# Best config summary\n")
    out.write(f"# Source log: {src}\n")
    out.write(f"{'bench':<6} {'case':<10} {'inflight':<8} {'chunk':<8} {'median':<10} {'p95':<10} {'verdict':<12}\n")
    for r in ordered:
        verdict = "GPU faster" if r["median"] < 1.0 else "CPU faster"
        out.write(
            f"{r['bench']:<6} {r['case']:<10} {r['inflight']:<8} {r['chunk']:<8} {r['median']:<10.4f} {r['p95']:<10.4f} {verdict:<12}\n"
        )
PY

echo "Summary saved to: $SUMMARY_FILE" | tee -a "$RESULT_FILE"
