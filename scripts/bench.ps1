# Benchmark gocx index performance against a real Go repo
# Usage: .\scripts\bench.ps1
# Requires: git, gocx binary in target/release/

param(
    [string]$RepoUrl = "https://github.com/kubernetes/client-go",
    [string]$RepoRef = "v0.29.3",
    [string]$WorkDir = "$env:TEMP\gocx-bench",
    [int]$Runs = 3
)

$BinaryPath = Join-Path $PSScriptRoot "..\target\release\gocx.exe"
if (-not (Test-Path $BinaryPath)) {
    Write-Error "gocx binary not found. Run: cargo build --release"
    exit 1
}

# Clone if needed
if (-not (Test-Path "$WorkDir\client-go")) {
    Write-Host "Cloning $RepoUrl @ $RepoRef ..."
    git clone --depth 1 --branch $RepoRef $RepoUrl "$WorkDir\client-go"
}

$Repo = "$WorkDir\client-go"
$GocxDir = "$Repo\.gocx"

# Init
& $BinaryPath init $Repo | Out-Null

Write-Host "`nBenchmark: gocx index --full on kubernetes/client-go ($RepoRef)"
Write-Host "Runs: $Runs"
Write-Host ("-" * 60)

$times = @()
for ($i = 1; $i -le $Runs; $i++) {
    # Remove old index to force cold start
    if (Test-Path "$GocxDir\index.db") {
        Remove-Item "$GocxDir\index.db" -Force
    }
    & $BinaryPath init $Repo | Out-Null

    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $output = & $BinaryPath index $Repo 2>&1
    $sw.Stop()
    $elapsed = $sw.Elapsed.TotalSeconds
    $times += $elapsed

    $symbols = ($output | Select-String "(\d+) symbols").Matches[0].Groups[1].Value
    $files   = ($output | Select-String "(\d+) files").Matches[0].Groups[1].Value
    Write-Host "Run $i: ${elapsed:F2}s  ($files files, $symbols symbols)"
}

$avg = ($times | Measure-Object -Average).Average
$min = ($times | Measure-Object -Minimum).Minimum
$max = ($times | Measure-Object -Maximum).Maximum

Write-Host ("-" * 60)
Write-Host "Min: ${min:F2}s  Max: ${max:F2}s  Avg: ${avg:F2}s"

# Status check
Write-Host "`nIndex status:"
& $BinaryPath status $Repo

# Quick query benchmark
Write-Host "`nQuery benchmark (find 'Client', 3 runs):"
for ($i = 1; $i -le 3; $i++) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    & $BinaryPath find "Client" --path $Repo --limit 20 | Out-Null
    $sw.Stop()
    Write-Host "  find: $($sw.Elapsed.TotalMilliseconds.ToString('F1'))ms"
}
