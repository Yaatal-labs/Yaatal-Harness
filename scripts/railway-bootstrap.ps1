[CmdletBinding()]
param(
    [string]$ServiceName = "Yaatal-Engine",
    [string]$Environment = "production",
    [string]$DatabaseService = "Postgres",
    [switch]$Apply,
    [switch]$SkipRedeploy
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Invoke-Railway {
    param([string[]]$Arguments)

    $output = & railway @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Railway command failed: railway $($Arguments -join ' ')"
    }

    return $output
}

function Invoke-RailwayJson {
    param([string[]]$Arguments)

    $raw = Invoke-Railway -Arguments $Arguments
    $text = ($raw -join "`n").Trim()
    if ([string]::IsNullOrWhiteSpace($text)) {
        return $null
    }

    return $text | ConvertFrom-Json -Depth 64
}

function Set-RailwayVariable {
    param(
        [string]$Name,
        [string]$Value,
        [switch]$Secret
    )

    if ($Secret) {
        $Value | & railway variable set $Name --stdin -s $ServiceName -e $Environment --skip-deploys | Out-Null
    } else {
        & railway variable set "$Name=$Value" -s $ServiceName -e $Environment --skip-deploys | Out-Null
    }

    if ($LASTEXITCODE -ne 0) {
        throw "Failed to set Railway variable '$Name'."
    }
}

function New-Secret {
    $bytes = New-Object byte[] 48
    $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    try {
        $rng.GetBytes($bytes)
    } finally {
        $rng.Dispose()
    }

    return [Convert]::ToBase64String($bytes)
}

function Get-VariableValue {
    param(
        [object]$Variables,
        [string]$Name
    )

    $property = $Variables.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }

    return $property.Value
}

$null = Invoke-Railway -Arguments @("whoami")

$status = Invoke-RailwayJson -Arguments @("status", "--json")
$environmentNode = $status.environments.edges |
    ForEach-Object { $_.node } |
    Where-Object { $_.name -eq $Environment } |
    Select-Object -First 1

if (-not $environmentNode) {
    throw "Railway environment '$Environment' was not found in the linked project."
}

$serviceInstances = $environmentNode.serviceInstances.edges | ForEach-Object { $_.node }
$databaseInstance = $serviceInstances |
    Where-Object { $_.serviceName -eq $DatabaseService } |
    Select-Object -First 1

if (-not $databaseInstance) {
    throw "Railway service '$DatabaseService' was not found in environment '$Environment'."
}

$databaseHealthy = $databaseInstance.latestDeployment.status -eq "SUCCESS" -and -not $databaseInstance.latestDeployment.deploymentStopped
if (-not $databaseHealthy) {
    throw "Railway service '$DatabaseService' is not active. Fix the database service before wiring the API."
}

$variables = Invoke-RailwayJson -Arguments @("variable", "list", "--json", "-s", $ServiceName, "-e", $Environment)
if (-not $variables) {
    $variables = [pscustomobject]@{}
}

$changes = New-Object System.Collections.Generic.List[object]

$databaseReference = '${{' + $DatabaseService + '.DATABASE_URL}}'
$databaseUrl = Get-VariableValue -Variables $variables -Name "DATABASE_URL"
if ([string]::IsNullOrWhiteSpace($databaseUrl)) {
    $changes.Add([pscustomobject]@{
        Name = "DATABASE_URL"
        CurrentValue = $databaseUrl
        DesiredValue = $databaseReference
        Secret = $false
    })
} elseif ($databaseUrl -notmatch '^postgres(ql)?://') {
    Write-Warning "DATABASE_URL is set but does not look like a Postgres connection string."
}

$jwtSecret = Get-VariableValue -Variables $variables -Name "JWT_SECRET"
if ([string]::IsNullOrWhiteSpace($jwtSecret)) {
    $changes.Add([pscustomobject]@{
        Name = "JWT_SECRET"
        CurrentValue = $jwtSecret
        DesiredValue = "<generated>"
        Secret = $true
    })
}

Write-Host "Railway project: $($status.name)"
Write-Host "Environment: $Environment"
Write-Host "Service: $ServiceName"
Write-Host "Database service: $DatabaseService ($($databaseInstance.latestDeployment.status))"

if ($changes.Count -eq 0) {
    Write-Host "Bootstrap status: ready"
} else {
    Write-Host "Bootstrap status: missing or mismatched variables detected"
    foreach ($change in $changes) {
        $targetValue = if ($change.Secret) { "<generated>" } else { $change.DesiredValue }
        Write-Host " - $($change.Name) -> $targetValue"
    }
}

if (-not $Apply) {
    if ($changes.Count -gt 0) {
        Write-Host ""
        Write-Host "Re-run with -Apply to wire the missing values."
        exit 1
    }

    exit 0
}

foreach ($change in $changes) {
    if ($change.Secret) {
        Set-RailwayVariable -Name $change.Name -Value (New-Secret) -Secret
    } else {
        Set-RailwayVariable -Name $change.Name -Value $change.DesiredValue
    }
}

if ($changes.Count -eq 0) {
    Write-Host "No variable changes were needed."
} else {
    Write-Host "Applied $($changes.Count) Railway variable update(s)."
}

if (-not $SkipRedeploy) {
    $deployment = Invoke-RailwayJson -Arguments @("redeploy", "-s", $ServiceName, "-y", "--json")
    Write-Host "Redeploy triggered: $($deployment.id)"
    Write-Host "Tail deploy logs with: railway logs --latest --deployment -s $ServiceName --lines 200"
} else {
    Write-Host "Skipped redeploy. Run 'railway redeploy -s $ServiceName -y' when ready."
}
