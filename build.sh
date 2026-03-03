#!/usr/bin/env bash
set -euo pipefail

if ! command -v cargo &>/dev/null; then
    echo "cargo not found. Install Rust: https://rustup.rs/"
    exit 1
fi

echo "Building release..."
cargo build --workspace --release

mkdir -p dist
cp target/release/organum-resampler dist/
cp target/release/organum-wavtool dist/
cp target/release/caching-tool dist/

if [ ! -f dist/organum.yaml ]; then
    cat > dist/organum.yaml <<EOF
feature_extension: "ogc"
sample_rate: 44100
frame_period: 5.0
zstd_compression_level: 3
compressor_threshold: 0.85
compressor_limit: 0.99
EOF
fi

echo "Done."
ls dist/
