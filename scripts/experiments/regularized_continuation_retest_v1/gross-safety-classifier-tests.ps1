$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$classifier = Join-Path $PSScriptRoot 'gross-safety-classifier.ps1'
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ("mtg-kernel-gross-safety-tests-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testRoot | Out-Null

function Write-NoBomJson {
    param([Parameter(Mandatory = $true)]$Value, [Parameter(Mandatory = $true)][string]$Path)
    [IO.File]::WriteAllText($Path, ($Value | ConvertTo-Json -Depth 12), [Text.UTF8Encoding]::new($false))
}

function New-TerminalStream {
    param(
        [Parameter(Mandatory = $true)][int[]]$Ranks,
        [Parameter(Mandatory = $true)][string]$Checkpoint,
        [Parameter(Mandatory = $true)][string]$Model
    )
    $rows = @()
    $counts = [ordered]@{
        overall = [ordered]@{ wins = 0; losses = 0; draws = 0 }
        P0 = [ordered]@{ wins = 0; losses = 0; draws = 0 }
        P1 = [ordered]@{ wins = 0; losses = 0; draws = 0 }
    }
    for ($index = 0; $index -lt $Ranks.Count; $index++) {
        $seat = if (($index % 2) -eq 0) { 'P0' } else { 'P1' }
        $bucket = switch ($Ranks[$index]) { 1 { 'wins' }; 0 { 'draws' }; -1 { 'losses' } }
        $counts.overall[$bucket]++
        $counts[$seat][$bucket]++
        $rows += [ordered]@{
            episode_index = $index
            pair_index = [math]::Floor($index / 2)
            environment_seed = 9000 + [math]::Floor($index / 2)
            learner_seat = $seat
            deck_hashes_u64 = @(11, 22)
            terminal_order_rank = $Ranks[$index]
        }
    }
    [ordered]@{
        schema = 'mtg-kernel-head-to-head-terminal-stream/v1'
        evaluation_base_seed = 42
        pair_count = $Ranks.Count / 2
        episode_count = $Ranks.Count
        candidate = [ordered]@{
            run_sha256 = ('a' * 64)
            identity_bundle_sha256 = ('b' * 64)
            generation = 32
            checkpoint_manifest_sha256 = $Checkpoint
            checkpoint_payload_sha256 = ('c' * 64)
            model_parameter_sha256 = $Model
        }
        opponent = [ordered]@{
            run_sha256 = ('d' * 64)
            generation = 384
            checkpoint_manifest_sha256 = ('e' * 64)
            checkpoint_payload_sha256 = ('f' * 64)
            model_parameter_sha256 = ('1' * 64)
        }
        runtime = [ordered]@{
            worker_count = 2
            sessions_per_worker = 32
            broker_batch_target = 16
            environment_randomization_v2 = $true
            all_natural = $true
        }
        learner_outcomes = $counts
        episodes = $rows
    }
}

try {
    $controlPath = Join-Path $testRoot 'control.json'
    $selectedPath = Join-Path $testRoot 'selected.json'
    Write-NoBomJson (New-TerminalStream -Ranks @(-1, 0, 1, 1) -Checkpoint ('2' * 64) -Model ('3' * 64)) $controlPath
    Write-NoBomJson (New-TerminalStream -Ranks @(1, 0, -1, 1) -Checkpoint ('4' * 64) -Model ('5' * 64)) $selectedPath

    $passPath = Join-Path $testRoot 'pass.json'
    & $classifier -ControlPath $controlPath -SelectedPath $selectedPath -OutputPath $passPath -ExpectedSeed 42 -ExpectedPairs 2 -OverallFloor 0 -SeatFloor 0 -ExpectedOpponentRunSha256 ('d' * 64) -ExpectedOpponentCheckpointSha256 ('e' * 64)
    $pass = Get-Content -LiteralPath $passPath -Raw | ConvertFrom-Json
    if ($pass.passed -ne $true -or [int]$pass.comparison.overall.net -ne 0 -or
        [int]$pass.comparison.P0.net -ne 0 -or [int]$pass.comparison.P1.net -ne 0 -or
        [int]$pass.comparison.overall.selected_better -ne 1 -or
        [int]$pass.comparison.overall.control_better -ne 1) {
        throw 'balanced synthetic classification did not pass at the zero floors'
    }

    $stopPath = Join-Path $testRoot 'stop.json'
    & $classifier -ControlPath $controlPath -SelectedPath $selectedPath -OutputPath $stopPath -ExpectedSeed 42 -ExpectedPairs 2 -OverallFloor 1 -SeatFloor 0 -ExpectedOpponentRunSha256 ('d' * 64) -ExpectedOpponentCheckpointSha256 ('e' * 64)
    $stop = Get-Content -LiteralPath $stopPath -Raw | ConvertFrom-Json
    if ($stop.passed -ne $false -or [string]$stop.disposition -ne 'GROSS-SAFETY-STOP') {
        throw 'synthetic floor failure did not stop'
    }

    $tampered = Get-Content -LiteralPath $selectedPath -Raw | ConvertFrom-Json
    $tampered.episodes[2].environment_seed++
    $tamperedPath = Join-Path $testRoot 'tampered.json'
    Write-NoBomJson $tampered $tamperedPath
    $tamperedOutput = Join-Path $testRoot 'tampered-output.json'
    $rejected = $false
    try {
        & $classifier -ControlPath $controlPath -SelectedPath $tamperedPath -OutputPath $tamperedOutput -ExpectedSeed 42 -ExpectedPairs 2 -OverallFloor 0 -SeatFloor 0 -ExpectedOpponentRunSha256 ('d' * 64) -ExpectedOpponentCheckpointSha256 ('e' * 64)
    }
    catch {
        $rejected = $true
    }
    if (-not $rejected -or (Test-Path -LiteralPath $tamperedOutput)) {
        throw 'pairing tamper was not rejected before publication'
    }

    $wrongTopologyControl = Get-Content -LiteralPath $controlPath -Raw | ConvertFrom-Json
    $wrongTopologySelected = Get-Content -LiteralPath $selectedPath -Raw | ConvertFrom-Json
    $wrongTopologyControl.runtime.worker_count = 4
    $wrongTopologySelected.runtime.worker_count = 4
    $wrongTopologyControlPath = Join-Path $testRoot 'wrong-topology-control.json'
    $wrongTopologySelectedPath = Join-Path $testRoot 'wrong-topology-selected.json'
    Write-NoBomJson $wrongTopologyControl $wrongTopologyControlPath
    Write-NoBomJson $wrongTopologySelected $wrongTopologySelectedPath
    $wrongTopologyOutput = Join-Path $testRoot 'wrong-topology-output.json'
    $rejected = $false
    try {
        & $classifier -ControlPath $wrongTopologyControlPath -SelectedPath $wrongTopologySelectedPath -OutputPath $wrongTopologyOutput -ExpectedSeed 42 -ExpectedPairs 2 -OverallFloor 0 -SeatFloor 0 -ExpectedOpponentRunSha256 ('d' * 64) -ExpectedOpponentCheckpointSha256 ('e' * 64)
    }
    catch {
        $rejected = $true
    }
    if (-not $rejected -or (Test-Path -LiteralPath $wrongTopologyOutput)) {
        throw 'equal but wrong runtime topology was not rejected before publication'
    }
    Write-Host 'GROSS SAFETY CLASSIFIER TESTS PASS'
}
finally {
    $resolvedTestRoot = (Resolve-Path -LiteralPath $testRoot -ErrorAction SilentlyContinue).Path
    $resolvedTemp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if ($null -ne $resolvedTestRoot -and $resolvedTestRoot.StartsWith($resolvedTemp, [StringComparison]::OrdinalIgnoreCase)) {
        Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force
    }
}
