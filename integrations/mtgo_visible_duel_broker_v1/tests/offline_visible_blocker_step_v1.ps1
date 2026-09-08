$ErrorActionPreference = 'Stop'

$root = Split-Path $PSScriptRoot -Parent
$fixtureDirectory = if ([string]::IsNullOrWhiteSpace($env:MTGO_VISIBLE_BLOCKER_FIXTURE_DIR)) {
    Join-Path $root '..\mtgo_visible_duel_producer_v1\tests\fixtures\ChromeHost\bin\Release\net472'
} else {
    $env:MTGO_VISIBLE_BLOCKER_FIXTURE_DIR
}
$brokerDirectory = if ([string]::IsNullOrWhiteSpace($env:MTGO_VISIBLE_BLOCKER_BROKER_DIR)) {
    Join-Path $root 'build'
} else {
    $env:MTGO_VISIBLE_BLOCKER_BROKER_DIR
}
$hostPath = (Resolve-Path (Join-Path $fixtureDirectory 'visible_chrome_fixture_host_v1.exe')).Path
$brokerPath = (Resolve-Path (Join-Path $brokerDirectory 'mtgo_visible_duel_broker_v1.exe')).Path
$bootstrapPath = (Resolve-Path (Join-Path $brokerDirectory 'mtgo_visible_duel_bootstrap_v1.dll')).Path
$producerPath = (Resolve-Path (Join-Path $fixtureDirectory 'mtgo_visible_duel_producer_v1.dll')).Path
$validatorPath = (Get-Command check_mtgo_visible_duel_producer_result_v1.exe -ErrorAction Stop).Source

function Get-LowerSha256([byte[]]$Bytes) {
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try {
        return -join ($sha256.ComputeHash($Bytes) | ForEach-Object { $_.ToString('x2') })
    } finally {
        $sha256.Dispose()
    }
}

function Get-BlockerStepCommitment(
    [string]$SourceSha256,
    [int]$SelectedIndex,
    [string]$Operation,
    [string]$BlockerOrdinal,
    [string]$AttackerOrdinal,
    [string]$ModelSelectionSha256
) {
    $bytes = [Collections.Generic.List[byte]]::new()
    $bytes.AddRange([Text.Encoding]::ASCII.GetBytes(
        'mtgo-visible-multi-attacker-blocker-execution-step-v1'))
    foreach ($part in @(
        $SourceSha256,
        $SelectedIndex.ToString(),
        $Operation,
        $BlockerOrdinal,
        $AttackerOrdinal,
        $ModelSelectionSha256
    )) {
        $partBytes = [Text.Encoding]::ASCII.GetBytes($part)
        [uint64]$count = $partBytes.Length
        $length = [byte[]]::new(8)
        for ($index = 7; $index -ge 0; $index--) {
            $length[$index] = [byte]($count -band 0xff)
            $count = $count -shr 8
        }
        $bytes.AddRange($length)
        $bytes.AddRange($partBytes)
    }
    return Get-LowerSha256 $bytes.ToArray()
}

$hostProcess = Start-Process -FilePath $hostPath -ArgumentList '--wait-for-blocker-broker' -WindowStyle Hidden -PassThru
try {
    Start-Sleep -Seconds 1
    $output = & $brokerPath --pid $hostProcess.Id --bootstrap $bootstrapPath --producer $producerPath --validator $validatorPath
    if ($LASTEXITCODE -ne 0) {
        throw 'synthetic visible-blocker observation failed'
    }
    $json = $output -join "`n"
    $result = $json | ConvertFrom-Json
    if ($result.result_kind -ne 'visible_multi_attacker_blocker_selection' -or
        @($result.selection.ordered_available_blockers).Count -ne 1 -or
        @($result.selection.current_state.combat.ordered_attackers).Count -ne 2) {
        throw 'native broker did not release the strict visible blocker selection'
    }
    $selectionSha256 = Get-LowerSha256 ([Text.Encoding]::UTF8.GetBytes($json))
    $modelSelectionSha256 = '8' * 64
    $stepSha256 = Get-BlockerStepCommitment `
        $selectionSha256 1 'b' '0' '-' $modelSelectionSha256
    $receiptOutput = & $brokerPath --pid $hostProcess.Id --bootstrap $bootstrapPath --producer $producerPath --validator $validatorPath --blocker-selection-sha256 $selectionSha256 --selected-index 1 --blocker-operation b --blocker-ordinal 0 --attacker-ordinal - --model-selection-sha256 $modelSelectionSha256 --step-sha256 $stepSha256
    if ($LASTEXITCODE -ne 0) {
        throw 'sealed visible-blocker step failed'
    }
    $receipt = ($receiptOutput -join "`n") | ConvertFrom-Json
    if ($receipt.status -ne 'submitted') {
        throw 'sealed visible-blocker step did not submit'
    }
    $staleOutput = & $brokerPath --pid $hostProcess.Id --bootstrap $bootstrapPath --producer $producerPath --validator $validatorPath --blocker-selection-sha256 $selectionSha256 --selected-index 1 --blocker-operation b --blocker-ordinal 0 --attacker-ordinal - --model-selection-sha256 $modelSelectionSha256 --step-sha256 $stepSha256
    if ($LASTEXITCODE -ne 0 -or (($staleOutput -join "`n") | ConvertFrom-Json).status -ne 'rejected') {
        throw 'unchanged blocker source did not fail closed'
    }
} finally {
    if (-not $hostProcess.HasExited) {
        Stop-Process -Id $hostProcess.Id -Force
        $hostProcess.WaitForExit()
    }
}

Write-Output 'MTGO_VISIBLE_DUEL_BROKER_OFFLINE_VISIBLE_BLOCKER_STEP_V1:PASS'
