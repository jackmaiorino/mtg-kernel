$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$env:PATH = "C:\Program Files (x86)\Microsoft Visual Studio\Installer;" + $env:PATH

function Invoke-Step([string]$label, [string]$directory, [scriptblock]$body) {
    Write-Output "=== $label"
    Push-Location $directory
    try {
        $global:LASTEXITCODE = 0
        & $body
        if ($LASTEXITCODE -ne 0) { throw "$label failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
}

Invoke-Step 'blackbox crate tests' (Join-Path $root 'mtgo_blackbox_v1') { cargo test --release }
Invoke-Step 'dxgi capture crate tests' (Join-Path $root 'mtgo_dxgi_capture_v1') { cargo test --release }
Invoke-Step 'broker source policy' (Join-Path $root 'mtgo_visible_duel_broker_v1') { & .\tests\source_policy_v1.ps1 }
Invoke-Step 'producer source policy' (Join-Path $root 'mtgo_visible_duel_producer_v1') { & .\tests\source_policy_v1.ps1 }
Invoke-Step 'producer build' (Join-Path $root 'mtgo_visible_duel_producer_v1') { dotnet build -c Release --nologo -v q }
Write-Output 'MTGO_INTEGRATION_CI_V1:PASS'
