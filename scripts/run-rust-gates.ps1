param(
    [ValidateSet("fmt", "check", "clippy", "test", "all")]
    [string]$Mode = "all"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $scriptDir "setup-rust-test-env.ps1")

function Invoke-Cargo {
    param([string[]]$CargoArgs)

    Write-Host ""
    Write-Host ">> cargo $($CargoArgs -join ' ')"
    & cargo @CargoArgs
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

switch ($Mode) {
    "fmt" { Invoke-Cargo -CargoArgs @("fmt", "--all", "--check") }
    "check" { Invoke-Cargo -CargoArgs @("check", "--workspace") }
    "clippy" { Invoke-Cargo -CargoArgs @("clippy", "--workspace", "--all-targets", "--", "-D", "warnings") }
    "test" { Invoke-Cargo -CargoArgs @("test", "--workspace", "--", "--test-threads=1") }
    "all" {
        Invoke-Cargo -CargoArgs @("fmt", "--all", "--check")
        Invoke-Cargo -CargoArgs @("check", "--workspace")
        Invoke-Cargo -CargoArgs @("clippy", "--workspace", "--all-targets", "--", "-D", "warnings")
        Invoke-Cargo -CargoArgs @("test", "--workspace", "--", "--test-threads=1")
    }
}
