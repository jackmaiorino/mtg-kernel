$ErrorActionPreference = 'Stop'

$root = Split-Path $PSScriptRoot -Parent
$hostPath = (Resolve-Path (Join-Path $root '..\mtgo_visible_duel_producer_v1\tests\fixtures\ChromeHost\bin\Release\net472\visible_chrome_fixture_host_v1.exe')).Path
$brokerPath = (Resolve-Path (Join-Path $root 'build\mtgo_visible_duel_broker_v1.exe')).Path
$bootstrapPath = (Resolve-Path (Join-Path $root 'build\mtgo_visible_duel_bootstrap_v1.dll')).Path
$producerPath = (Resolve-Path (Join-Path $root '..\mtgo_visible_duel_producer_v1\bin\Release\net472\mtgo_visible_duel_producer_v1.dll')).Path
$validatorPath = (Get-Command check_mtgo_visible_duel_producer_result_v1.exe -ErrorAction Stop).Source

$hostProcess = Start-Process -FilePath $hostPath -ArgumentList '--wait-for-broker' -WindowStyle Hidden -PassThru
try {
    Start-Sleep -Milliseconds 500
    $output = & $brokerPath --pid $hostProcess.Id --bootstrap $bootstrapPath --producer $producerPath --validator $validatorPath
    if ($LASTEXITCODE -ne 0) {
        throw 'synthetic visible-decision broker invocation failed'
    }
    $result = ($output -join "`n") | ConvertFrom-Json
    if ($result.result_kind -ne 'visible_decision' -or
        $result.decision.current_state.turn -ne 1 -or
        $result.decision.current_state.phase -ne 'main1' -or
        @($result.decision.ordered_legal_actions | Where-Object action_kind -eq 'pass').Count -ne 1) {
        throw 'native broker did not release the strict validated visible decision'
    }

    $invalidValidatorPath = (Resolve-Path (Join-Path $root 'README.md')).Path
    $priorErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $rejectedOutput = & $brokerPath --pid $hostProcess.Id --bootstrap $bootstrapPath --producer $producerPath --validator $invalidValidatorPath 2>$null
        $rejectedExitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $priorErrorActionPreference
    }
    if ($rejectedExitCode -eq 0 -or -not [string]::IsNullOrWhiteSpace(($rejectedOutput -join "`n"))) {
        throw 'broker released data without strict validator acceptance'
    }
} finally {
    if (-not $hostProcess.HasExited) {
        Stop-Process -Id $hostProcess.Id -Force
        $hostProcess.WaitForExit()
    }
}

Write-Output 'MTGO_VISIBLE_DUEL_BROKER_OFFLINE_VISIBLE_DECISION_V1:PASS'
