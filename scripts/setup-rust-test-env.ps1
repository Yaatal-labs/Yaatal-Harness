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
    $cpExe = Get-Command cp.exe -ErrorAction SilentlyContinue
    if (-not $cpExe) {
        $gitCp = "C:\Program Files\Git\usr\bin\cp.exe"
        if (Test-Path $gitCp) {
            $env:PATH = "$(Split-Path -Parent $gitCp);$env:PATH"
            Write-Host "Added Git usr/bin to PATH for cp.exe"
        }
    }

    $cl = Get-Command cl.exe -ErrorAction SilentlyContinue
    if (-not $cl) {
        $vswhere = "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe"
        if (Test-Path $vswhere) {
            $vsInstall = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
            if ($LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace($vsInstall)) {
                $vcvars = Join-Path $vsInstall "VC\Auxiliary\Build\vcvars64.bat"
                if (Test-Path $vcvars) {
                    $envDump = & cmd.exe /s /c "call `"$vcvars`" >nul && set"
                    if ($LASTEXITCODE -eq 0) {
                        foreach ($line in $envDump) {
                            if ($line -match "^(.*?)=(.*)$") {
                                Set-Item -Path "Env:$($matches[1])" -Value $matches[2]
                            }
                        }
                        $cl = Get-Command cl.exe -ErrorAction SilentlyContinue
                        if ($cl) {
                            Write-Host "Imported MSVC build environment via vcvars64.bat"
                        }
                    }
                }
            }
        }
    }

    if (-not $cl) {
        Write-Warning "cl.exe not found in PATH. Native crates may fail to build."
    }

    $cpExe = Get-Command cp.exe -ErrorAction SilentlyContinue
    if (-not $cpExe) {
        Write-Warning "cp.exe not found in PATH. Some libsql build scripts may fail."
    }
}
