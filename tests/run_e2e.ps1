# E2E Test Suite PowerShell Runner
param(
    [int]$Tier = 0,
    [switch]$All,
    [switch]$Help
)

if ($Help) {
    Write-Host "Usage: .\run_e2e.ps1 [-Tier 1|2|3|4] [-All]"
    exit 0
}

$scriptPath = Join-Path $PSScriptRoot "run_e2e.py"
if ($Tier -gt 0) {
    python $scriptPath --tier $Tier
} else {
    python $scriptPath
}
exit $LASTEXITCODE
