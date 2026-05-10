@echo off
setlocal enabledelayedexpansion

:: Check Rust
where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo cargo not found. Install Rust: https://rustup.rs/
    exit /b 1
)

:: Build
echo Building release...
cargo build --workspace --release
if %errorlevel% neq 0 (
    echo Build failed.
    exit /b %errorlevel%
)

:: Copy to dist
if not exist "dist" mkdir "dist"
copy /Y "target\release\organum-resampler.exe" "dist\" >nul
copy /Y "target\release\organum-wavtool.exe" "dist\" >nul
copy /Y "target\release\caching-tool.exe" "dist\" >nul
copy /Y "LICENSE" "dist\" >nul
copy /Y "THIRD_PARTY_NOTICES.md" "dist\" >nul
if not exist "dist\licenses" mkdir "dist\licenses"
copy /Y "licenses\*" "dist\licenses\" >nul

if not exist "dist\organum.yaml" (
    echo feature_extension: "ogc" > "dist\organum.yaml"
    echo sample_rate: 44100 >> "dist\organum.yaml"
    echo frame_period: 5.0 >> "dist\organum.yaml"
    echo zstd_compression_level: 3 >> "dist\organum.yaml"
    echo compressor_threshold: 0.85 >> "dist\organum.yaml"
    echo compressor_limit: 0.99 >> "dist\organum.yaml"
    echo gpu_warp_enabled: false >> "dist\organum.yaml"
    echo gpu_warp_min_frames: 2048 >> "dist\organum.yaml"
)

echo Done.
dir /B "dist"
