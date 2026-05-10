#!/usr/bin/env bash
set -euo pipefail

if ! command -v cargo &>/dev/null; then
    echo "cargo not found. Install Rust: https://rustup.rs/"
    exit 1
fi

echo "Building GPU WORLD release..."
cargo build --workspace --release --features gpu-warp

mkdir -p dist-gpu
cp target/release/organum-resampler dist-gpu/
cp target/release/organum-wavtool dist-gpu/
cp target/release/caching-tool dist-gpu/

if [ ! -f dist-gpu/organum.yaml ]; then
    cat > dist-gpu/organum.yaml <<EOF
feature_extension: "ogc"
sample_rate: 44100
frame_period: 5.0
zstd_compression_level: 3
compressor_threshold: 0.85
compressor_limit: 0.99
gpu_warp_enabled: true
gpu_warp_min_frames: 2048
EOF
fi

echo "Done. GPU WORLD artifacts are in dist-gpu/."
ls dist-gpu/
