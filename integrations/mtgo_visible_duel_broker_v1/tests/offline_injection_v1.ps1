$ErrorActionPreference = 'Stop'

$root = Split-Path $PSScriptRoot -Parent
$hostPath = (Resolve-Path (Join-Path $root 'synthetic_host\bin\Release\net472\synthetic_managed_host_v1.exe')).Path
$brokerPath = (Resolve-Path (Join-Path $root 'build\mtgo_visible_duel_broker_v1.exe')).Path
$bootstrapPath = (Resolve-Path (Join-Path $root 'build\mtgo_visible_duel_bootstrap_v1.dll')).Path
$producerPath = (Resolve-Path (Join-Path $root '..\mtgo_visible_duel_producer_v1\bin\Release\net472\mtgo_visible_duel_producer_v1.dll')).Path

$hostProcess = Start-Process -FilePath $hostPath -WindowStyle Hidden -PassThru
try {
    Start-Sleep -Milliseconds 300
    $output = & $brokerPath --pid $hostProcess.Id --bootstrap $bootstrapPath --producer $producerPath
    if ($LASTEXITCODE -ne 0) {
        throw 'synthetic broker invocation failed'
    }
    $expected = '{"result_kind":"abstained","reason":"duel_surface_unavailable"}'
    if (($output -join "`n") -ne $expected) {
        throw 'synthetic broker emitted unexpected output'
    }
} finally {
    if (-not $hostProcess.HasExited) {
        Stop-Process -Id $hostProcess.Id -Force
        $hostProcess.WaitForExit()
    }
}

Write-Output 'MTGO_VISIBLE_DUEL_BROKER_OFFLINE_INJECTION_V1:PASS'
