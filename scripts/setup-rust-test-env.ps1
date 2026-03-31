param(
    [int]$BuildJobs = 1
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Cross-platform temp directory: $env:TEMP (Windows) -> $env:TMPDIR (macOS) -> /tmp (Linux)
$tmpBase = if ($env:TEMP) { $env:TEMP } elseif ($env:TMPDIR) { $env:TMPDIR } else { "/tmp" }

$cargoHome = Join-Path $tmpBase "yaatal-cargo-home"
$targetDir = Join-Path $tmpBase "yaatal-target"

New-Item -ItemType Directory -Force -Path $cargoHome | Out-Null
New-Item -ItemType Directory -Force -Path $targetDir | Out-Null

$env:CARGO_HOME = $cargoHome
$env:CARGO_TARGET_DIR = $targetDir
$env:CARGO_BUILD_JOBS = "$BuildJobs"

Write-Host "CARGO_HOME=$env:CARGO_HOME"
Write-Host "CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR"
Write-Host "CARGO_BUILD_JOBS=$env:CARGO_BUILD_JOBS"

if ($IsWindows) {
    $cl = Get-Command cl.exe -ErrorAction SilentlyContinue
    if (-not $cl) {
        Write-Warning "cl.exe not found in PATH. Native crates may fail to build."
    }

    $cpExe = Get-Command cp.exe -ErrorAction SilentlyContinue
    if (-not $cpExe) {
        Write-Warning "cp.exe not found in PATH. Some libsql build scripts may fail."
    }
}
