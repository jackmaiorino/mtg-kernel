param(
    [Parameter(Mandatory = $true)][string]$ControlPath,
    [Parameter(Mandatory = $true)][string]$SelectedPath,
    [Parameter(Mandatory = $true)][string]$OutputPath,
    [uint64]$ExpectedSeed = 1942001,
    [uint64]$ExpectedPairs = 512,
    [int]$OverallFloor = -26,
    [int]$SeatFloor = -18,
    [string]$ExpectedOpponentRunSha256 = '2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae',
    [string]$ExpectedOpponentCheckpointSha256 = '4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Get-FileSha256 {
    param([Parameter(Mandatory = $true)][string]$Path)
    (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Assert-Equal {
    param($Left, $Right, [Parameter(Mandatory = $true)][string]$Label)
    if ($Left -ne $Right) {
        throw "$Label mismatch: '$Left' != '$Right'"
    }
}

function Assert-SequenceEqual {
    param($Left, $Right, [Parameter(Mandatory = $true)][string]$Label)
    $leftJson = @($Left) | ConvertTo-Json -Compress
    $rightJson = @($Right) | ConvertTo-Json -Compress
    if ($leftJson -ne $rightJson) {
        throw "$Label mismatch: $leftJson != $rightJson"
    }
}

function Get-RecomputedOutcomeCounts {
    param([Parameter(Mandatory = $true)]$Rows)
    $result = [ordered]@{
        overall = [ordered]@{ wins = 0; losses = 0; draws = 0 }
        P0 = [ordered]@{ wins = 0; losses = 0; draws = 0 }
        P1 = [ordered]@{ wins = 0; losses = 0; draws = 0 }
    }
    foreach ($row in $Rows) {
        $seat = [string]$row.learner_seat
        $rank = [int]$row.terminal_order_rank
        $bucket = switch ($rank) {
            1 { 'wins' }
            0 { 'draws' }
            -1 { 'losses' }
            default { throw "invalid terminal-order rank $rank" }
        }
        $result.overall[$bucket]++
        $result[$seat][$bucket]++
    }
    return $result
}

function Assert-PublishedCounts {
    param([Parameter(Mandatory = $true)]$Artifact, [Parameter(Mandatory = $true)]$Recomputed, [Parameter(Mandatory = $true)][string]$Label)
    foreach ($scope in @('overall', 'P0', 'P1')) {
        foreach ($bucket in @('wins', 'losses', 'draws')) {
            Assert-Equal ([uint64]$Artifact.learner_outcomes.$scope.$bucket) ([uint64]$Recomputed[$scope][$bucket]) "$Label $scope $bucket"
        }
    }
}

if (-not (Test-Path -LiteralPath $ControlPath -PathType Leaf) -or
    -not (Test-Path -LiteralPath $SelectedPath -PathType Leaf)) {
    throw 'both complete terminal-stream artifacts are required before classification'
}
if (Test-Path -LiteralPath $OutputPath) {
    throw "classifier output already exists: $OutputPath"
}

$control = Get-Content -LiteralPath $ControlPath -Raw | ConvertFrom-Json
$selected = Get-Content -LiteralPath $SelectedPath -Raw | ConvertFrom-Json
$schema = 'mtg-kernel-head-to-head-terminal-stream/v1'
Assert-Equal ([string]$control.schema) $schema 'control schema'
Assert-Equal ([string]$selected.schema) $schema 'selected schema'
foreach ($artifact in @($control, $selected)) {
    Assert-Equal ([uint64]$artifact.evaluation_base_seed) $ExpectedSeed 'evaluation seed'
    Assert-Equal ([uint64]$artifact.pair_count) $ExpectedPairs 'pair count'
    Assert-Equal ([uint64]$artifact.episode_count) (2 * $ExpectedPairs) 'episode count'
    Assert-Equal ([uint64]$artifact.candidate.generation) 32 'candidate generation'
    Assert-Equal ([uint64]$artifact.opponent.generation) 384 'promoted(2) generation'
    Assert-Equal ([bool]$artifact.runtime.environment_randomization_v2) $true 'envrand-v2'
    Assert-Equal ([bool]$artifact.runtime.all_natural) $true 'natural terminal completion'
}

foreach ($field in @('run_sha256', 'identity_bundle_sha256')) {
    Assert-Equal ([string]$control.candidate.$field) ([string]$selected.candidate.$field) "candidate $field"
}
foreach ($field in @('run_sha256', 'checkpoint_manifest_sha256', 'checkpoint_payload_sha256', 'model_parameter_sha256')) {
    Assert-Equal ([string]$control.opponent.$field) ([string]$selected.opponent.$field) "opponent $field"
}
foreach ($field in @('worker_count', 'sessions_per_worker', 'broker_batch_target')) {
    Assert-Equal ([uint64]$control.runtime.$field) ([uint64]$selected.runtime.$field) "runtime $field"
}
foreach ($artifact in @($control, $selected)) {
    Assert-Equal ([uint64]$artifact.runtime.worker_count) 2 'frozen worker count'
    Assert-Equal ([uint64]$artifact.runtime.sessions_per_worker) 32 'frozen sessions per worker'
    Assert-Equal ([uint64]$artifact.runtime.broker_batch_target) 16 'frozen broker batch target'
    Assert-Equal ([string]$artifact.opponent.run_sha256) $ExpectedOpponentRunSha256 'promoted(2) run SHA-256'
    Assert-Equal ([string]$artifact.opponent.checkpoint_manifest_sha256) $ExpectedOpponentCheckpointSha256 'promoted(2) checkpoint SHA-256'
}
if ([string]$control.candidate.checkpoint_manifest_sha256 -eq [string]$selected.candidate.checkpoint_manifest_sha256 -or
    [string]$control.candidate.model_parameter_sha256 -eq [string]$selected.candidate.model_parameter_sha256) {
    throw 'selected and control checkpoints must be distinct'
}

$expectedEpisodes = [int](2 * $ExpectedPairs)
$controlRows = @($control.episodes)
$selectedRows = @($selected.episodes)
Assert-Equal $controlRows.Count $expectedEpisodes 'control row count'
Assert-Equal $selectedRows.Count $expectedEpisodes 'selected row count'

$comparison = [ordered]@{
    overall = [ordered]@{ selected_better = 0; control_better = 0; tied = 0 }
    P0 = [ordered]@{ selected_better = 0; control_better = 0; tied = 0 }
    P1 = [ordered]@{ selected_better = 0; control_better = 0; tied = 0 }
}
for ($index = 0; $index -lt $expectedEpisodes; $index++) {
    $controlRow = $controlRows[$index]
    $selectedRow = $selectedRows[$index]
    $expectedSeat = if (($index % 2) -eq 0) { 'P0' } else { 'P1' }
    $expectedPair = [uint64][math]::Floor($index / 2)
    Assert-Equal ([uint64]$controlRow.episode_index) ([uint64]$index) "control episode index $index"
    Assert-Equal ([uint64]$selectedRow.episode_index) ([uint64]$index) "selected episode index $index"
    Assert-Equal ([uint64]$controlRow.pair_index) $expectedPair "control pair index $index"
    Assert-Equal ([uint64]$selectedRow.pair_index) $expectedPair "selected pair index $index"
    Assert-Equal ([string]$controlRow.learner_seat) $expectedSeat "control learner seat $index"
    Assert-Equal ([string]$selectedRow.learner_seat) $expectedSeat "selected learner seat $index"
    Assert-Equal ([uint64]$controlRow.environment_seed) ([uint64]$selectedRow.environment_seed) "environment seed $index"
    Assert-SequenceEqual $controlRow.deck_hashes_u64 $selectedRow.deck_hashes_u64 "deck hashes $index"
    $controlRank = [int]$controlRow.terminal_order_rank
    $selectedRank = [int]$selectedRow.terminal_order_rank
    if ($controlRank -notin @(-1, 0, 1) -or $selectedRank -notin @(-1, 0, 1)) {
        throw "noncanonical terminal-order rank at episode $index"
    }
    # The frozen estimand is one selected-better or control-better event per
    # matched leg under W > D > L. In particular, W versus L contributes one
    # better event, not an arithmetic reward delta of two.
    $bucket = if ($selectedRank -gt $controlRank) {
        'selected_better'
    }
    elseif ($selectedRank -lt $controlRank) {
        'control_better'
    }
    else {
        'tied'
    }
    $comparison.overall[$bucket]++
    $comparison[$expectedSeat][$bucket]++
}

$controlCounts = Get-RecomputedOutcomeCounts $controlRows
$selectedCounts = Get-RecomputedOutcomeCounts $selectedRows
Assert-PublishedCounts $control $controlCounts 'control'
Assert-PublishedCounts $selected $selectedCounts 'selected'

foreach ($scope in @('overall', 'P0', 'P1')) {
    $comparison[$scope]['net'] = [int]$comparison[$scope].selected_better - [int]$comparison[$scope].control_better
}
$passed = ([int]$comparison.overall.net -ge $OverallFloor -and
    [int]$comparison.P0.net -ge $SeatFloor -and
    [int]$comparison.P1.net -ge $SeatFloor)
$result = [ordered]@{
    schema = 'regularized-continuation-gross-safety-classification/v1'
    passed = $passed
    disposition = if ($passed) { 'PASS' } else { 'GROSS-SAFETY-STOP' }
    estimand = 'paired-terminal-order-W>D>L/v1'
    evaluation_seed = $ExpectedSeed
    pair_count = $ExpectedPairs
    episode_count_per_arm = 2 * $ExpectedPairs
    thresholds = [ordered]@{ overall_net_minimum = $OverallFloor; each_selected_seat_net_minimum = $SeatFloor }
    comparison = $comparison
    control_outcomes = $controlCounts
    selected_outcomes = $selectedCounts
    identities = [ordered]@{
        control_checkpoint_manifest_sha256 = [string]$control.candidate.checkpoint_manifest_sha256
        control_model_parameter_sha256 = [string]$control.candidate.model_parameter_sha256
        selected_checkpoint_manifest_sha256 = [string]$selected.candidate.checkpoint_manifest_sha256
        selected_model_parameter_sha256 = [string]$selected.candidate.model_parameter_sha256
        opponent_checkpoint_manifest_sha256 = [string]$selected.opponent.checkpoint_manifest_sha256
        opponent_model_parameter_sha256 = [string]$selected.opponent.model_parameter_sha256
    }
    inputs = [ordered]@{
        control = [ordered]@{ path = (Resolve-Path -LiteralPath $ControlPath).Path; sha256 = Get-FileSha256 $ControlPath }
        selected = [ordered]@{ path = (Resolve-Path -LiteralPath $SelectedPath).Path; sha256 = Get-FileSha256 $SelectedPath }
    }
}

$json = $result | ConvertTo-Json -Depth 12
$bytes = [Text.UTF8Encoding]::new($false).GetBytes($json)
$stream = [IO.File]::Open($OutputPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
try {
    $stream.Write($bytes, 0, $bytes.Length)
    $stream.Flush($true)
}
finally {
    $stream.Dispose()
}
Write-Host "GROSS SAFETY classification=$($result.disposition) net=$($comparison.overall.net) P0=$($comparison.P0.net) P1=$($comparison.P1.net)"
