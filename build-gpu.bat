@echo off
setlocal enabledelayedexpansion

:: Check Rust
where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo cargo not found. Install Rust: https://rustup.rs/
    exit /b 1
)

:: Build with GPU features
echo Building GPU-enabled release...
cargo build --workspace --release --features gpu-warp
if %errorlevel% neq 0 (
    echo Build failed.
    exit /b %errorlevel%
)

:: Copy to dist-gpu
if not exist "dist-gpu" mkdir "dist-gpu"
copy /Y "target\release\organum-resampler.exe" "dist-gpu\" >nul
copy /Y "target\release\organum-wavtool.exe" "dist-gpu\" >nul
copy /Y "target\release\caching-tool.exe" "dist-gpu\" >nul
copy /Y "LICENSE" "dist-gpu\" >nul
copy /Y "THIRD_PARTY_NOTICES.md" "dist-gpu\" >nul
if not exist "dist-gpu\licenses" mkdir "dist-gpu\licenses"
copy /Y "licenses\*" "dist-gpu\licenses\" >nul

if not exist "dist-gpu\organum.yaml" (
    echo feature_extension: "ogc" > "dist-gpu\organum.yaml"
    echo sample_rate: 44100 >> "dist-gpu\organum.yaml"
    echo frame_period: 5.0 >> "dist-gpu\organum.yaml"
    echo zstd_compression_level: 3 >> "dist-gpu\organum.yaml"
    echo compressor_threshold: 0.85 >> "dist-gpu\organum.yaml"
    echo compressor_limit: 0.99 >> "dist-gpu\organum.yaml"
    echo gpu_warp_enabled: true >> "dist-gpu\organum.yaml"
    echo gpu_warp_min_frames: 2048 >> "dist-gpu\organum.yaml"
)

echo Done. GPU-enabled artifacts are in dist-gpu/.
dir /B "dist-gpu"
