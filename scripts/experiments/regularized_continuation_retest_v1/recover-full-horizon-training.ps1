param(
    [string]$AttemptRoot = 'D:\mtg-kernel-regularized-continuation-retest-v1\development\full-horizon-training\attempt-003'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
$script:Updates = [uint64]512
$script:EpisodesPerSeed = [uint64]32768
$script:RequiredGenerations = @([uint64]64, [uint64]128, [uint64]256, [uint64]384, [uint64]512)
$script:SelectedBeta = '0.1'
$script:ExpectedExecutableSha256 = '475e82cad20da268574d8d4df475b42e7251e468831efc37d67b013927a30d1b'
$script:ExpectedDesignSha256 = '1f6ea9128e7d5b44f80c34c96c42ccd47325224fb11d22bc4aae0d43cef40b00'
$script:ExpectedVoidText = "phase=development-training VOID environmental_or_harness_interruption=The property 'seed' cannot be found on this object. Verify that the property exists."

function Get-FileRecord {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "required file is missing: $Path"
    }
    $item = Get-Item -LiteralPath $Path
    [ordered]@{
        path = $item.FullName
        bytes = [uint64]$item.Length
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $item.FullName).Hash.ToLowerInvariant()
    }
}

function Assert-FileRecordUnchanged {
    param(
        [Parameter(Mandatory = $true)]$Record,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $actual = Get-FileRecord -Path ([string]$Record.path)
    if ([string]$actual.sha256 -ne [string]$Record.sha256 -or
        [uint64]$actual.bytes -ne [uint64]$Record.bytes) {
        throw "$Label changed after the bound training plan"
    }
    $actual
}

function Get-CheckpointRecord {
    param(
        [Parameter(Mandatory = $true)][string]$StoreRoot,
        [Parameter(Mandatory = $true)][uint64]$Generation
    )
    $prefix = Join-Path $StoreRoot ('checkpoints\update-{0:d8}' -f $Generation)
    $checkpointPath = "$prefix.checkpoint.json"
    $sidecarPath = "$prefix.sidecar.json"
    $statePath = "$prefix.state.f32le"
    foreach ($path in @($checkpointPath, $sidecarPath, $statePath)) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "generation-$Generation artifact is missing: $path"
        }
    }
    $checkpoint = Get-Content -LiteralPath $checkpointPath -Raw | ConvertFrom-Json
    if ([uint64]$checkpoint.generation_index -ne $Generation -or
        [uint64]$checkpoint.progress.successful_update_count -ne $Generation -or
        [uint64]$checkpoint.train_state.adam_step -ne $Generation) {
        throw "generation-$Generation checkpoint progress binding mismatch"
    }
    [ordered]@{
        generation = $Generation
        checkpoint = Get-FileRecord -Path $checkpointPath
        sidecar = Get-FileRecord -Path $sidecarPath
        state = Get-FileRecord -Path $statePath
        completed_episode_count = [uint64]$checkpoint.progress.completed_episode_count
        adam_step = [uint64]$checkpoint.train_state.adam_step
    }
}

function Get-StoreRecord {
    param(
        [Parameter(Mandatory = $true)][uint64]$Seed,
        [Parameter(Mandatory = $true)][string]$StoreRoot,
        [Parameter(Mandatory = $true)][ValidateSet('candidate', 'control')][string]$Role
    )
    Assert-GenerationCheckpoint -Store $StoreRoot -Generation $script:Updates
    $runPath = Join-Path $StoreRoot 'run.json'
    $latestPath = Join-Path $StoreRoot 'latest.json'
    $run = Get-Content -LiteralPath $runPath -Raw | ConvertFrom-Json
    if ([uint64]$run.schedule.base_seed -ne $Seed -or
        [uint64]$run.schedule.requested_successful_updates -ne $script:Updates -or
        [uint64]$run.schedule.batch_episodes -ne 64 -or
        [string]$run.environment.environment_randomization_v2.identity -ne 'mtg-kernel-environment-randomization-sha256-v2') {
        throw "$Role seed-$Seed Store schedule or environment binding mismatch"
    }
    $endpoint = Get-CheckpointRecord -StoreRoot $StoreRoot -Generation $script:Updates
    if ([uint64]$endpoint.completed_episode_count -ne $script:EpisodesPerSeed) {
        throw "$Role seed-$Seed endpoint episode count mismatch"
    }
    [ordered]@{
        role = $Role
        seed = $Seed
        store_root = (Resolve-Path -LiteralPath $StoreRoot).Path
        store_tree_sha256 = Get-StoreTreeHash -Path $StoreRoot
        store_file_count = [uint64]@((Get-StoreFileInventory -Path $StoreRoot)).Count
        run = Get-FileRecord -Path $runPath
        latest = Get-FileRecord -Path $latestPath
        generation = $script:Updates
        adam_step = [uint64]$endpoint.adam_step
        completed_episode_count = [uint64]$endpoint.completed_episode_count
        checkpoints = @($script:RequiredGenerations | ForEach-Object {
            Get-CheckpointRecord -StoreRoot $StoreRoot -Generation $_
        })
    }
}

function Get-RecoveredLaneRecord {
    param(
        [Parameter(Mandatory = $true)][int]$WaveIndex,
        [Parameter(Mandatory = $true)][uint64]$Seed,
        [Parameter(Mandatory = $true)][int]$GpuOrdinal,
        [Parameter(Mandatory = $true)]$Plan
    )
    $label = "wave-$('{0:d2}' -f $WaveIndex)-seed-$Seed-gpu$GpuOrdinal"
    $storeParent = Join-Path $AttemptRoot $label
    $completionPath = Join-Path $AttemptRoot "$label.completion.json"
    $logPath = Join-Path $AttemptRoot "$label.log"
    $stdoutPath = Join-Path $AttemptRoot "$label.stdout.log"
    $stderrPath = Join-Path $AttemptRoot "$label.stderr.log"
    $completion = Get-Content -LiteralPath $completionPath -Raw | ConvertFrom-Json
    if ([string]$completion.schema -ne 'regularized-continuation-native-lane-completion/v1' -or
        $completion.success -ne $true -or
        [uint64]$completion.seed -ne $Seed -or [uint64]$completion.updates -ne $script:Updates -or
        [int]$completion.gpu_ordinal -ne $GpuOrdinal -or
        [string]$completion.policy_anchor_beta -ne $script:SelectedBeta -or
        [string]$completion.store_parent -ne $storeParent -or
        [string]$completion.log_path -ne $logPath -or
        [string]$completion.executable_sha256 -ne [string]$Plan.executable.sha256 -or
        [string]$completion.log_sha256 -ne (Get-FileHash -Algorithm SHA256 -LiteralPath $logPath).Hash.ToLowerInvariant()) {
        throw "$label completion binding mismatch"
    }
    if ([int]$completion.process_id -le 0 -or
        (Get-Process -Id ([int]$completion.process_id) -ErrorAction SilentlyContinue)) {
        throw "$label wrapper process has not exited"
    }
    if ((Get-Item -LiteralPath $stdoutPath).Length -ne 0 -or
        (Get-Item -LiteralPath $stderrPath).Length -ne 0) {
        throw "$label wrote wrapper stdout or stderr"
    }
    if ((Get-Content -LiteralPath $logPath -Raw) -notmatch 'test result: ok\.') {
        throw "$label native success marker is missing"
    }
    $store = Get-StoreRecord -Seed $Seed -StoreRoot (Join-Path $storeParent 'run-0\store') -Role 'candidate'
    [ordered]@{
        wave_index = $WaveIndex
        seed = $Seed
        gpu_ordinal = $GpuOrdinal
        store_parent = $storeParent
        completed_utc = [string]$completion.completed_utc
        log = Get-FileRecord -Path $logPath
        stdout = Get-FileRecord -Path $stdoutPath
        stderr = Get-FileRecord -Path $stderrPath
        completion = Get-FileRecord -Path $completionPath
        resource_summary = [ordered]@{
            available = $false
            reason = 'live samples were held only in controller memory and were lost in the postprocessing exception'
        }
        store = $store
    }
}

Assert-ExclusiveWindow
$git = Get-GitRecord -RepoRoot $script:RepoRoot
$planPath = Join-Path $AttemptRoot 'training-plan.json'
$startPath = Join-Path $AttemptRoot 'training-start.json'
$voidPath = Join-Path $AttemptRoot 'void-development-training.log'
$manifestPath = Join-Path $AttemptRoot 'training-manifest.json'
if (Test-Path -LiteralPath $manifestPath) {
    throw "training manifest already exists: $manifestPath"
}
$planFile = Get-FileRecord -Path $planPath
$plan = Get-Content -LiteralPath $planPath -Raw | ConvertFrom-Json
$startFile = Get-FileRecord -Path $startPath
$start = Get-Content -LiteralPath $startPath -Raw | ConvertFrom-Json
$voidFile = Get-FileRecord -Path $voidPath
$voidText = Get-Content -LiteralPath $voidPath -Raw
if ([string]$plan.schema -ne 'regularized-continuation-full-horizon-training-plan/v1' -or
    $plan.terminal_outcomes_read -ne $false -or [string]$plan.selected_beta -ne $script:SelectedBeta -or
    [uint64]$plan.updates_per_seed -ne $script:Updates -or
    [uint64]$plan.episodes_per_seed -ne $script:EpisodesPerSeed -or
    @($plan.training_seeds).Count -ne 3 -or
    @($plan.training_seeds | ForEach-Object { [uint64]$_ }) -join ',' -ne '970001,970002,970003') {
    throw 'bound training plan identity mismatch'
}
if ($voidText -notlike "*$($script:ExpectedVoidText)*") {
    throw 'recovery is authorized only for the exact dropped-seed postprocessing exception'
}
if ([string]$plan.executable.sha256 -ne $script:ExpectedExecutableSha256 -or
    [string]$plan.design.document.sha256 -ne $script:ExpectedDesignSha256) {
    throw 'training executable or design identity mismatch'
}
Assert-FileRecordUnchanged -Record $plan.executable -Label 'training executable' | Out-Null
Assert-FileRecordUnchanged -Record $plan.design.document -Label 'design document' | Out-Null
foreach ($name in @('full_horizon_training', 'common', 'run_native')) {
    Assert-FileRecordUnchanged -Record $plan.harness.$name -Label "training harness $name" | Out-Null
}
foreach ($name in @('coefficient_screen', 'gross_safety', 'throughput')) {
    Assert-FileRecordUnchanged -Record $plan.prerequisites.$name -Label "training prerequisite $name" | Out-Null
}
foreach ($prefix in @('pool_json', 'init_checkpoint', 'init_sidecar', 'init_state')) {
    $path = [string]$plan.inputs."${prefix}_path"
    $sha = [string]$plan.inputs."${prefix}_sha256"
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant() -ne $sha) {
        throw "training input changed: $prefix"
    }
}

$lanes = @(
    foreach ($wave in @($plan.topology.waves | Sort-Object { [int]$_.wave_index })) {
        foreach ($member in @($wave.members)) {
            Get-RecoveredLaneRecord -WaveIndex ([int]$wave.wave_index) -Seed ([uint64]$member.seed) -GpuOrdinal ([int]$member.gpu) -Plan $plan
        }
    }
)
if ($lanes.Count -ne 3) { throw 'recovery requires exactly three completed candidate lanes' }
$candidateRecords = @($lanes | Sort-Object seed | ForEach-Object { $_.store })
$controlRecords = @(
    foreach ($control in @($plan.controls | Sort-Object { [uint64]$_.seed })) {
        $record = Get-StoreRecord -Seed ([uint64]$control.seed) -StoreRoot ([string]$control.store_root) -Role 'control'
        if ([string]$record.store_tree_sha256 -ne [string]$control.store_tree_sha256 -or
            [uint64]$record.store_file_count -ne [uint64]$control.store_file_count) {
            throw "beta-zero control Store changed for seed $($control.seed)"
        }
        $record
    }
)
if ($controlRecords.Count -ne 3) { throw 'recovery requires exactly three immutable controls' }

$startUtc = [DateTimeOffset]::Parse([string]$start.utc)
$completedUtc = @($lanes | ForEach-Object { [DateTimeOffset]::Parse([string]$_.completed_utc) } | Sort-Object | Select-Object -Last 1)[0]
$wallSeconds = ($completedUtc - $startUtc).TotalSeconds
$manifest = [ordered]@{
    schema = 'regularized-continuation-full-horizon-training/v1'
    passed = $true
    disposition = 'TRAINING-COMPLETE; DEVELOPMENT-EVALUATION-RELEASED'
    completed_utc = $completedUtc.ToString('O')
    plan = $planFile
    git = $plan.git
    recovery_git = $git
    toolchain = $plan.toolchain
    cuda = $plan.cuda
    executable = $plan.executable
    harness = $plan.harness
    prerequisites = $plan.prerequisites
    selected_beta = $script:SelectedBeta
    training_seeds = @([uint64]970001, [uint64]970002, [uint64]970003)
    updates_per_seed = $script:Updates
    episodes_per_seed = $script:EpisodesPerSeed
    total_episode_count = [uint64](3 * $script:EpisodesPerSeed)
    wall_seconds = $wallSeconds
    aggregate_episodes_per_second = (3.0 * $script:EpisodesPerSeed) / $wallSeconds
    projected_wall_seconds = [double]$plan.topology.projected_total_seconds
    topology = 'gpu0+gpu1, then gpu1'
    waves = @(
        foreach ($waveIndex in @(0, 1)) {
            [ordered]@{
                wave_index = $waveIndex
                lanes = @($lanes | Where-Object { [int]$_.wave_index -eq $waveIndex } | ForEach-Object {
                    [ordered]@{
                        seed = $_.seed
                        gpu_ordinal = $_.gpu_ordinal
                        store_parent = $_.store_parent
                        completed_utc = $_.completed_utc
                        log = $_.log
                        stdout = $_.stdout
                        stderr = $_.stderr
                        completion = $_.completion
                        resource_summary = $_.resource_summary
                    }
                })
            }
        }
    )
    controls = $controlRecords
    candidates = $candidateRecords
    recovery = [ordered]@{
        kind = 'postprocessing-only'
        source_void = $voidFile
        source_start = $startFile
        exact_exception = "The property 'seed' cannot be found on this object. Verify that the property exists."
        measurement_replayed = $false
        training_or_gameplay_executed_by_recovery = $false
        terminal_outcomes_read = $false
        lost_non_scientific_field = 'per-second live resource samples'
    }
    terminal_outcomes_read = $false
    nonclaim = 'Recovery validates already-completed training evidence only. Training completion is not playing-strength evidence and does not nominate or promote a policy.'
}
Write-JsonFile -Value $manifest -Path $manifestPath
Write-Host "Full-horizon training recovery PASS: $manifestPath"
