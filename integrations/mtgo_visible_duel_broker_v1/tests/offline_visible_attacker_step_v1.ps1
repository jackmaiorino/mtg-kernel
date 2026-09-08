$ErrorActionPreference = 'Stop'

$root = Split-Path $PSScriptRoot -Parent
$fixtureDirectory = if ([string]::IsNullOrWhiteSpace($env:MTGO_VISIBLE_ATTACKER_FIXTURE_DIR)) {
    Join-Path $root '..\mtgo_visible_duel_producer_v1\tests\fixtures\ChromeHost\bin\Release\net472'
} else {
    $env:MTGO_VISIBLE_ATTACKER_FIXTURE_DIR
}
$hostPath = (Resolve-Path (Join-Path $fixtureDirectory 'visible_chrome_fixture_host_v1.exe')).Path
$brokerDirectory = if ([string]::IsNullOrWhiteSpace($env:MTGO_VISIBLE_ATTACKER_BROKER_DIR)) {
    Join-Path $root 'build'
} else {
    $env:MTGO_VISIBLE_ATTACKER_BROKER_DIR
}
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

function Get-AttackerPlanCommitment(
    [string]$SourceSha256,
    [int]$CandidateCount,
    [string]$DesiredMask
) {
    $bytes = [Collections.Generic.List[byte]]::new()
    $bytes.AddRange([Text.Encoding]::ASCII.GetBytes('mtgo-visible-attacker-execution-plan-v1'))
    foreach ($part in @($SourceSha256, $CandidateCount.ToString(), $DesiredMask)) {
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

$hostProcess = Start-Process -FilePath $hostPath -ArgumentList '--wait-for-attacker-broker' -PassThru
try {
    Start-Sleep -Seconds 1
    $output = & $brokerPath --pid $hostProcess.Id --bootstrap $bootstrapPath --producer $producerPath --validator $validatorPath
    if ($LASTEXITCODE -ne 0) {
        throw 'synthetic visible-attacker observation failed'
    }
    $json = $output -join "`n"
    $result = $json | ConvertFrom-Json
    if ($result.result_kind -ne 'visible_attacker_selection' -or
        @($result.selection.ordered_candidates).Count -ne 1 -or
        $result.selection.ordered_candidates[0].currently_attacking) {
        throw 'native broker did not release the strict visible attacker selection'
    }
    $selectionSha256 = Get-LowerSha256 ([Text.Encoding]::UTF8.GetBytes($json))
    $desiredMask = '0000000000000001'
    $planSha256 = Get-AttackerPlanCommitment $selectionSha256 1 $desiredMask
    $receiptOutput = & $brokerPath --pid $hostProcess.Id --bootstrap $bootstrapPath --producer $producerPath --validator $validatorPath --attacker-selection-sha256 $selectionSha256 --candidate-count 1 --desired-mask $desiredMask --plan-sha256 $planSha256
    if ($LASTEXITCODE -ne 0) {
        throw 'sealed visible-attacker step failed'
    }
    $receipt = ($receiptOutput -join "`n") | ConvertFrom-Json
    if ($receipt.result_kind -ne 'action_dispatch_receipt' -or
        $receipt.status -ne 'submitted') {
        throw 'sealed visible-attacker step did not return the fixed submitted receipt'
    }
    $staleOutput = & $brokerPath --pid $hostProcess.Id --bootstrap $bootstrapPath --producer $producerPath --validator $validatorPath --attacker-selection-sha256 $selectionSha256 --candidate-count 1 --desired-mask $desiredMask --plan-sha256 $planSha256
    if ($LASTEXITCODE -ne 0) {
        throw 'stale visible-attacker step did not return a fixed receipt'
    }
    $stale = ($staleOutput -join "`n") | ConvertFrom-Json
    if ($stale.status -ne 'rejected') {
        throw 'missing visible attacker postcondition did not fail closed'
    }
} finally {
    if (-not $hostProcess.HasExited) {
        Stop-Process -Id $hostProcess.Id -Force
        $hostProcess.WaitForExit()
    }
}

Write-Output 'MTGO_VISIBLE_DUEL_BROKER_OFFLINE_VISIBLE_ATTACKER_STEP_V1:PASS'
