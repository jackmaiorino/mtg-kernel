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

    $decisionBytes = [Text.Encoding]::UTF8.GetBytes(($output -join "`n"))
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        $decisionSha256 = -join ($sha256.ComputeHash($decisionBytes) | ForEach-Object { $_.ToString('x2') })
    } finally {
        $sha256.Dispose()
    }
    $passIndex = -1
    for ($index = 0; $index -lt $result.decision.ordered_legal_actions.Count; $index++) {
        if ($result.decision.ordered_legal_actions[$index].action_kind -eq 'pass') {
            $passIndex = $index
            break
        }
    }
    if ($passIndex -lt 0) {
        throw 'strict visible decision has no pass index'
    }
    $dispatchOutput = & $brokerPath --pid $hostProcess.Id --bootstrap $bootstrapPath --producer $producerPath --validator $validatorPath --decision-sha256 $decisionSha256 --selected-index $passIndex
    if ($LASTEXITCODE -ne 0) {
        throw 'sealed visible-action dispatch failed'
    }
    $dispatchReceipt = ($dispatchOutput -join "`n") | ConvertFrom-Json
    if ($dispatchReceipt.result_kind -ne 'action_dispatch_receipt' -or
        $dispatchReceipt.status -ne 'submitted') {
        throw 'sealed visible-action dispatch did not return the fixed submitted receipt'
    }

    $priorErrorActionPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $staleOutput = & $brokerPath --pid $hostProcess.Id --bootstrap $bootstrapPath --producer $producerPath --validator $validatorPath --decision-sha256 $decisionSha256 --selected-index $passIndex 2>$null
        $staleExitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $priorErrorActionPreference
    }
    if ($staleExitCode -ne 0) {
        throw 'stale sealed dispatch must return a fixed rejected receipt'
    }
    $staleReceipt = ($staleOutput -join "`n") | ConvertFrom-Json
    if ($staleReceipt.status -ne 'rejected') {
        throw 'stale sealed dispatch did not fail closed'
    }

    $differentIndex = if ($passIndex -eq 0) { 1 } else { 0 }
    $differentOutput = & $brokerPath --pid $hostProcess.Id --bootstrap $bootstrapPath --producer $producerPath --validator $validatorPath --decision-sha256 $decisionSha256 --selected-index $differentIndex
    if ($LASTEXITCODE -ne 0) {
        throw 'second-index sealed dispatch must return a fixed rejected receipt'
    }
    $differentReceipt = ($differentOutput -join "`n") | ConvertFrom-Json
    if ($differentReceipt.status -ne 'rejected') {
        throw 'one visible decision authorized more than one selected index'
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
