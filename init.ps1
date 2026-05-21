param(
    [ValidateSet("fmt", "check", "test", "all")]
    [string]$Mode = "check"
)

$ErrorActionPreference = "Stop"

function Invoke-Step {
    param(
        [string]$Name,
        [string[]]$Command
    )

    Write-Host "=== $Name ==="
    & $Command[0] @($Command[1..($Command.Length - 1)])
    if ($LASTEXITCODE -ne 0) {
        throw "$Name failed with exit code $LASTEXITCODE"
    }
}

if ($Mode -eq "fmt" -or $Mode -eq "all") {
    Invoke-Step "cargo fmt" @("cargo", "fmt", "--all", "--check")
}

if ($Mode -eq "check" -or $Mode -eq "all") {
    Invoke-Step "cargo check" @("cargo", "check", "--workspace", "--all-targets")
}

if ($Mode -eq "test" -or $Mode -eq "all") {
    Invoke-Step "cargo test" @("cargo", "test", "--workspace", "--", "--test-threads=1")
}

Write-Host "=== done: $Mode ==="
