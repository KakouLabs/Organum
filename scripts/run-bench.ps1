param(
    [string[]]$InflightValues = @("1", "2", "3"),
    [string[]]$ChunkValues = @("auto", "2048", "4096"),
    [string]$OutputDir = "",
    [string]$BenchVersion = "",
    [string]$CargoProfile = "dev"
)

$ErrorActionPreference = "Stop"

$cargoCmd = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargoCmd) {
    throw "cargo not found. Install Rust toolchain first."
}
$cargoExe = $cargoCmd.Source

function Get-VersionTag {
    param([string]$RootPath)

    if ($BenchVersion) {
        if ($BenchVersion.StartsWith("v")) {
            return $BenchVersion
        }
        return "v$BenchVersion"
    }

    $cargoToml = Join-Path $RootPath "Cargo.toml"
    if (-not (Test-Path $cargoToml)) {
        return "v0.0.0"
    }

    $inPackage = $false
    foreach ($line in Get-Content $cargoToml) {
        if ($line -match '^\[package\]') {
            $inPackage = $true
            continue
        }
        if ($line -match '^\[' -and $inPackage) {
            break
        }
        if ($inPackage -and $line -match '^\s*version\s*=\s*"([^"]+)"') {
            $v = $Matches[1]
            if ($v.StartsWith("v")) {
                return $v
            }
            return "v$v"
        }
    }

    return "v0.0.0"
}

$versionTag = Get-VersionTag -RootPath (Get-Location)
if (-not $OutputDir) {
    $OutputDir = "benchmarks/$versionTag/bench-results"
}

function Run-Bench {
    param(
        [string]$BenchName,
        [string]$OutFile
    )

    $cargoArgs = @("run", "--features", "gpu-warp", "--bin", $BenchName)
    if ($CargoProfile -eq "release") {
        $cargoArgs = @("run", "--release", "--features", "gpu-warp", "--bin", $BenchName)
    }
    $cmd = "cargo " + ($cargoArgs -join " ")

    "`n>>> RUN: $cmd" | Tee-Object -FilePath $OutFile -Append

    $stdoutFile = [System.IO.Path]::GetTempFileName()
    $stderrFile = [System.IO.Path]::GetTempFileName()

    try {
        $startInfo = @{
            FilePath               = $cargoExe
            ArgumentList           = $cargoArgs
            NoNewWindow            = $true
            Wait                   = $true
            PassThru               = $true
            RedirectStandardOutput = $stdoutFile
            RedirectStandardError  = $stderrFile
        }

        $proc = Start-Process @startInfo

        if (Test-Path $stdoutFile) {
            Get-Content $stdoutFile | Tee-Object -FilePath $OutFile -Append
        }
        if (Test-Path $stderrFile) {
            Get-Content $stderrFile | Tee-Object -FilePath $OutFile -Append
        }

        if ($proc.ExitCode -ne 0) {
            throw "Command failed with exit code $($proc.ExitCode): $cmd"
        }
    }
    finally {
        Remove-Item $stdoutFile -ErrorAction SilentlyContinue
        Remove-Item $stderrFile -ErrorAction SilentlyContinue
    }
}

function Write-CaseHeader {
    param(
        [string]$Inflight,
        [string]$Chunk,
        [string]$OutFile
    )

    "`n============================================================" | Tee-Object -FilePath $OutFile -Append
    "CASE inflight=$Inflight chunk=$Chunk" | Tee-Object -FilePath $OutFile -Append
    "============================================================" | Tee-Object -FilePath $OutFile -Append
}

function Set-ChunkEnv {
    param(
        [string]$Chunk,
        [string]$OutFile
    )

    if ($Chunk -eq "auto") {
        Remove-Item Env:WARP_GPU_CHUNK_FRAMES -ErrorAction SilentlyContinue
        "WARP_GPU_CHUNK_FRAMES=auto(full-frame)" | Tee-Object -FilePath $OutFile -Append
    }
    else {
        $env:WARP_GPU_CHUNK_FRAMES = $Chunk
        "WARP_GPU_CHUNK_FRAMES=$Chunk" | Tee-Object -FilePath $OutFile -Append
    }
}

function Run-Case {
    param(
        [string]$Inflight,
        [string]$Chunk,
        [string]$OutFile
    )

    Write-CaseHeader -Inflight $Inflight -Chunk $Chunk -OutFile $OutFile

    $env:WARP_GPU_INFLIGHT = $Inflight
    $env:WARP_BENCH_GPU_MIN_FRAMES = "1"
    $env:AP_BENCH_GPU_MIN_FRAMES = "1"

    Set-ChunkEnv -Chunk $Chunk -OutFile $OutFile

    Run-Bench -BenchName "warp-bench" -OutFile $OutFile
    Run-Bench -BenchName "ap-bench" -OutFile $OutFile
}

if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir | Out-Null
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$resultFile = Join-Path $OutputDir "bench-$timestamp.txt"
$summaryFile = Join-Path $OutputDir "bench-$timestamp.summary.txt"

"# Organum benchmark run" | Tee-Object -FilePath $resultFile
"# Timestamp: $timestamp" | Tee-Object -FilePath $resultFile -Append
"# Profile: $CargoProfile" | Tee-Object -FilePath $resultFile -Append
"# Inflight values: $($InflightValues -join ', ')" | Tee-Object -FilePath $resultFile -Append
"# Chunk values: $($ChunkValues -join ', ')" | Tee-Object -FilePath $resultFile -Append

foreach ($inflight in $InflightValues) {
    foreach ($chunk in $ChunkValues) {
        Run-Case -Inflight $inflight -Chunk $chunk -OutFile $resultFile
    }
}

"`nDone. Log saved to: $resultFile" | Tee-Object -FilePath $resultFile -Append

$pattern = '^CI_SUMMARY,case=(?<case>[^,]+),threshold=(?<threshold>[^,]+),median_ratio=(?<median>[^,]+),p95_ratio=(?<p95>[^,]+)$'
$rows = New-Object System.Collections.Generic.List[Object]
$currentInflight = ""
$currentChunk = ""
$currentBench = ""

foreach ($line in Get-Content $resultFile) {
    if ($line -match '^CASE inflight=(\S+) chunk=(\S+)$') {
        $currentInflight = $Matches[1]
        $currentChunk = $Matches[2]
        continue
    }

    if ($line -match '^>>> RUN: .*--bin\s+warp-bench') {
        $currentBench = "warp"
        continue
    }

    if ($line -match '^>>> RUN: .*--bin\s+ap-bench') {
        $currentBench = "ap"
        continue
    }

    if ($line -match $pattern) {
        $rows.Add([pscustomobject]@{
            bench    = $currentBench
            inflight = [int]$currentInflight
            chunk    = $currentChunk
            case     = $Matches['case']
            median   = [double]$Matches['median']
            p95      = [double]$Matches['p95']
        })
    }
}

if ($rows.Count -gt 0) {
    $bestLines = New-Object System.Collections.Generic.List[Object]
    foreach ($bench in @("warp", "ap")) {
        foreach ($case in @("short", "medium", "long", "stress_mix")) {
            $subset = $rows | Where-Object { $_.bench -eq $bench -and $_.case -eq $case }
            if ($subset.Count -eq 0) { continue }
            $best = $subset | Sort-Object median, p95 | Select-Object -First 1
            $bestLines.Add([pscustomobject]@{
                bench    = $bench
                case     = $case
                inflight = $best.inflight
                chunk    = $best.chunk
                median   = [math]::Round($best.median, 4)
                p95      = [math]::Round($best.p95, 4)
                verdict  = if ($best.median -lt 1.0) { "GPU faster" } else { "CPU faster" }
            })
        }
    }

    "# Best config summary" | Tee-Object -FilePath $summaryFile
    "# Source log: $resultFile" | Tee-Object -FilePath $summaryFile -Append
    $bestLines | Format-Table -AutoSize | Out-String | Tee-Object -FilePath $summaryFile -Append | Out-Null

    "`nSummary saved to: $summaryFile" | Tee-Object -FilePath $resultFile -Append
}
