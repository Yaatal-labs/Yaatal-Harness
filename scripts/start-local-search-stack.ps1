param(
    [string]$QdrantImage = "qdrant/qdrant:latest",
    [string]$QdrantContainerName = "yaatal-qdrant",
    [string]$QdrantUrl = "http://127.0.0.1:6333",
    [switch]$UseMockQdrant,
    [string]$EmbedderBind = "127.0.0.1:8090",
    [string]$SearchBind = "127.0.0.1:8081",
    [string]$QdrantCollection = "yaatal-search"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$embedderUrl = "http://$EmbedderBind"

function Wait-HttpOk {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Url,
        [int]$Attempts = 30
    )

    for ($attempt = 0; $attempt -lt $Attempts; $attempt++) {
        try {
            Invoke-WebRequest -UseBasicParsing -Uri $Url -TimeoutSec 2 | Out-Null
            return
        } catch {
            Start-Sleep -Seconds 1
        }
    }

    throw "Timed out waiting for $Url"
}

if ($UseMockQdrant) {
    $mockQdrantCommand = @(
        "Set-Location '$repoRoot'"
        "`$env:QDRANT_BIND = '127.0.0.1:6333'"
        "cargo run -p yaatal-search --bin mock_qdrant"
    ) -join "; "

    Write-Host "Starting mock Qdrant on 127.0.0.1:6333"
    $mockQdrantProcess = Start-Process `
        -FilePath "pwsh" `
        -ArgumentList @("-NoExit", "-Command", $mockQdrantCommand) `
        -PassThru

    Wait-HttpOk -Url $QdrantUrl
    Write-Host "Mock Qdrant PID: $($mockQdrantProcess.Id)"
} else {
    if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
        throw "docker is required to launch local Qdrant unless -UseMockQdrant is set"
    }

    $runningContainer = docker ps --filter "name=^/$QdrantContainerName$" --format "{{.Names}}"
    if (-not $runningContainer) {
        $existingContainer = docker ps -a --filter "name=^/$QdrantContainerName$" --format "{{.Names}}"
        if ($existingContainer) {
            Write-Host "Starting existing Qdrant container $QdrantContainerName"
            docker start $QdrantContainerName | Out-Null
        } else {
            Write-Host "Creating Qdrant container $QdrantContainerName"
            docker run -d --name $QdrantContainerName -p 6333:6333 $QdrantImage | Out-Null
        }
    }

    Wait-HttpOk -Url $QdrantUrl
}

$embedderCommand = @(
    "Set-Location '$repoRoot'"
    "`$env:BGE_M3_BIND = '$EmbedderBind'"
    "cargo run -p yaatal-search --bin mock_embedder"
) -join "; "

Write-Host "Starting mock embedder at $EmbedderBind"
$embedderProcess = Start-Process `
    -FilePath "pwsh" `
    -ArgumentList @("-NoExit", "-Command", $embedderCommand) `
    -PassThru

Wait-HttpOk -Url "$embedderUrl/health"
Write-Host "Mock embedder PID: $($embedderProcess.Id)"

$env:SEARCH_BIND = $SearchBind
$env:SEARCH_BACKEND = "external"
$env:BGE_M3_URL = $embedderUrl
$env:QDRANT_URL = $QdrantUrl
$env:QDRANT_COLLECTION = $QdrantCollection

Write-Host "Launching search service on $SearchBind against $QdrantUrl"
cargo run -p yaatal-search --bin search_service
