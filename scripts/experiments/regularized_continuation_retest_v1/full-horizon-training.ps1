param(
    [string]$EvidenceRoot = 'D:\mtg-kernel-regularized-continuation-retest-v1\development',
    [string]$Gate3ManifestPath = 'D:\mtg-kernel-regularized-continuation-retest-v1\development\seed-1940001\coefficient-screen\attempt-002\coefficient-manifest.json',
    [string]$Gate4ManifestPath = 'D:\mtg-kernel-regularized-continuation-retest-v1\development\seed-1942001\gross-safety\attempt-001\gross-safety-manifest.json',
    [string]$ThroughputManifestPath = 'D:\mtg-kernel-regularized-continuation-retest-v1\preflight\seed-969999\throughput-screen\attempt-007\throughput-manifest.json',
    [string]$DesignDocumentPath = 'C:\Users\Jack\IdeaProjects\mtg-kernel-composed-factorial-v1-codex\docs\native_regularized_continuation_retest_v1.md',
    [switch]$PreflightOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
$script:SelectedBeta = '0.1'
$script:Updates = [uint64]512
$script:EpisodesPerSeed = [uint64]32768
$script:RequiredGenerations = @([uint64]64, [uint64]128, [uint64]256, [uint64]384, [uint64]512)
$script:ExpectedGate3Sha256 = 'd580706976c0e650c2cab9f760c7064ad1ad7a805d4992151887e577535f82ac'
$script:ExpectedGate4Sha256 = '1381e95e8d8e2db49264aae9baa79cfd01475e08f6ef2ac7f29bc424ccd7f226'
$script:ExpectedThroughputSha256 = '988b5c1383afd24f54493215850592b3ef2b79b20ec68b937d5cfca947ec0ab2'
$script:ExpectedTrainingExecutableSha256 = '475e82cad20da268574d8d4df475b42e7251e468831efc37d67b013927a30d1b'
$root = New-UniqueAttemptRoot -EvidenceRoot $EvidenceRoot -GateName 'full-horizon-training'

function Get-FileRecord {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "required file is missing: $Path"
    }
    $item = Get-Item -LiteralPath $Path
    return [ordered]@{
        path = $item.FullName
        bytes = $item.Length
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $item.FullName).Hash.ToLowerInvariant()
    }
}

function Assert-FileSha256 {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$ExpectedSha256,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $record = Get-FileRecord -Path $Path
    if ([string]$record.sha256 -ne $ExpectedSha256) {
        throw "$Label SHA-256 mismatch: $($record.sha256); expected $ExpectedSha256"
    }
    return $record
}

function Get-HarnessRecord {
    return [ordered]@{
        full_horizon_training = Get-FileRecord -Path (Join-Path $PSScriptRoot 'full-horizon-training.ps1')
        common = Get-FileRecord -Path (Join-Path $PSScriptRoot 'common.ps1')
        run_native = Get-FileRecord -Path (Join-Path $PSScriptRoot 'run-native.ps1')
    }
}

function Assert-LaunchBindingsUnchanged {
    param(
        [Parameter(Mandatory = $true)][string]$ExecutablePath,
        [Parameter(Mandatory = $true)]$ExpectedExecutable,
        [Parameter(Mandatory = $true)]$ExpectedInputs,
        [Parameter(Mandatory = $true)]$ExpectedHarness,
        [Parameter(Mandatory = $true)]$ExpectedDesign
    )
    $currentExecutable = Get-FileRecord -Path $ExecutablePath
    if ([string]$currentExecutable.sha256 -ne [string]$ExpectedExecutable.sha256 -or
        [uint64]$currentExecutable.bytes -ne [uint64]$ExpectedExecutable.bytes) {
        throw 'training executable changed after preflight'
    }
    $currentInputs = Get-InputRecord
    foreach ($field in @('pool_json_sha256', 'init_checkpoint_sha256', 'init_sidecar_sha256', 'init_state_sha256')) {
        if ([string]$currentInputs[$field] -ne [string]$ExpectedInputs[$field]) {
            throw "training input changed after preflight: $field"
        }
    }
    $currentHarness = Get-HarnessRecord
    foreach ($field in @('full_horizon_training', 'common', 'run_native')) {
        if ([string]$currentHarness[$field].sha256 -ne [string]$ExpectedHarness[$field].sha256 -or
            [uint64]$currentHarness[$field].bytes -ne [uint64]$ExpectedHarness[$field].bytes) {
            throw "training harness changed after preflight: $field"
        }
    }
    $currentDesign = Get-FileRecord -Path ([string]$ExpectedDesign.path)
    if ([string]$currentDesign.sha256 -ne [string]$ExpectedDesign.sha256 -or
        [uint64]$currentDesign.bytes -ne [uint64]$ExpectedDesign.bytes) {
        throw 'scientific design document changed after preflight'
    }
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
        throw "generation-$Generation checkpoint does not bind generation, successful updates, and Adam step"
    }
    return [ordered]@{
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
        [uint64]$run.schedule.batch_episodes -ne 64) {
        throw "$Role seed-$Seed Store schedule mismatch"
    }
    if ([string]$run.environment.environment_randomization_v2.identity -ne 'mtg-kernel-environment-randomization-sha256-v2') {
        throw "$Role seed-$Seed Store does not bind environment randomization v2"
    }
    $endpoint = Get-CheckpointRecord -StoreRoot $StoreRoot -Generation $script:Updates
    if ([uint64]$endpoint.completed_episode_count -ne $script:EpisodesPerSeed) {
        throw "$Role seed-$Seed endpoint episode count mismatch"
    }
    $checkpoints = @($script:RequiredGenerations | ForEach-Object {
        Get-CheckpointRecord -StoreRoot $StoreRoot -Generation $_
    })
    return [ordered]@{
        role = $Role
        seed = $Seed
        store_root = (Resolve-Path -LiteralPath $StoreRoot).Path
        store_tree_sha256 = Get-StoreTreeHash -Path $StoreRoot
        store_file_count = @(Get-StoreFileInventory -Path $StoreRoot).Count
        run = Get-FileRecord -Path $runPath
        latest = Get-FileRecord -Path $latestPath
        generation = $script:Updates
        adam_step = [uint64]$endpoint.adam_step
        completed_episode_count = [uint64]$endpoint.completed_episode_count
        checkpoints = $checkpoints
    }
}

function Get-LaneRecord {
    param([Parameter(Mandatory = $true)]$Lane)
    $samples = @($Lane.resource_samples)
    $memoryFractions = @($samples | Where-Object { $_.host_memory_total_mib -gt 0 } | ForEach-Object {
        $_.host_memory_used_mib / [double]$_.host_memory_total_mib
    })
    $gpus = @(
        foreach ($ordinal in @(0, 1)) {
            $rows = @($samples | ForEach-Object { $_.gpus } | Where-Object { $_.ordinal -eq $ordinal })
            if ($rows.Count -gt 0) {
                [ordered]@{
                    ordinal = $ordinal
                    sample_count = $rows.Count
                    utilization_mean_percent = ($rows | ForEach-Object { [double]$_.utilization_percent } | Measure-Object -Average).Average
                    utilization_peak_percent = ($rows | ForEach-Object { [double]$_.utilization_percent } | Measure-Object -Maximum).Maximum
                    memory_peak_mib = ($rows | ForEach-Object { [double]$_.memory_used_mib } | Measure-Object -Maximum).Maximum
                }
            }
        }
    )
    return [ordered]@{
        seed = [uint64]$Lane.seed
        gpu_ordinal = [int]$Lane.gpu_ordinal
        store_parent = $Lane.store_parent
        wall_seconds = [double]$Lane.wall_seconds
        exit_code = [int]$Lane.exit_code
        started_utc = $Lane.started_utc
        completed_utc = $Lane.completed_utc
        log = Get-FileRecord -Path $Lane.log
        stdout = Get-FileRecord -Path $Lane.stdout
        stderr = Get-FileRecord -Path $Lane.stderr
        completion = $Lane.completion
        resource_summary = [ordered]@{
            sample_count = $samples.Count
            cpu_mean_percent = if ($samples.Count -eq 0) { $null } else { ($samples | Measure-Object -Property cpu_total_percent -Average).Average }
            cpu_peak_percent = if ($samples.Count -eq 0) { $null } else { ($samples | Measure-Object -Property cpu_total_percent -Maximum).Maximum }
            host_memory_peak_fraction = if ($memoryFractions.Count -eq 0) { $null } else { ($memoryFractions | Measure-Object -Maximum).Maximum }
            gpus = $gpus
        }
    }
}

function Get-ResourceSummary {
    param([Parameter(Mandatory = $true)]$Samples)
    $rows = @($Samples)
    if ($rows.Count -eq 0) {
        throw 'resource summary requires at least one sample'
    }
    $cpu = @($rows | ForEach-Object { [double]$_.cpu_total_percent })
    $used = @($rows | ForEach-Object { [double]$_.host_memory_used_mib })
    $hostTotal = [double]$rows[0].host_memory_total_mib
    $gpus = @(
        foreach ($ordinal in @(0, 1)) {
            $gpuRows = @($rows | ForEach-Object { $_.gpus } | Where-Object { $_.ordinal -eq $ordinal })
            [ordered]@{
                ordinal = $ordinal
                sample_count = $gpuRows.Count
                utilization_mean_percent = ($gpuRows | ForEach-Object { [double]$_.utilization_percent } | Measure-Object -Average).Average
                utilization_peak_percent = ($gpuRows | ForEach-Object { [double]$_.utilization_percent } | Measure-Object -Maximum).Maximum
                memory_peak_mib = ($gpuRows | ForEach-Object { [double]$_.memory_used_mib } | Measure-Object -Maximum).Maximum
            }
        }
    )
    return [ordered]@{
        sample_count = $rows.Count
        cpu_mean_percent = ($cpu | Measure-Object -Average).Average
        cpu_peak_percent = ($cpu | Measure-Object -Maximum).Maximum
        host_memory_peak_mib = ($used | Measure-Object -Maximum).Maximum
        host_memory_minimum_free_mib = $hostTotal - ($used | Measure-Object -Maximum).Maximum
        gpus = $gpus
    }
}

function Invoke-TrainingWave {
    param(
        [Parameter(Mandatory = $true)][int]$WaveIndex,
        [Parameter(Mandatory = $true)][object[]]$Members,
        [Parameter(Mandatory = $true)][string]$Executable
    )
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    $lanes = New-Object System.Collections.Generic.List[object]
    try {
        foreach ($member in $Members) {
            $label = "wave-$('{0:d2}' -f $WaveIndex)-seed-$($member.seed)-gpu$($member.gpu)"
            $storeParent = Join-Path $root $label
            $lane = Start-NativeLane -Executable $Executable -Seed $member.seed -Updates $script:Updates -StoreParent $storeParent -GpuOrdinal $member.gpu -PolicyAnchorBeta $script:SelectedBeta -LogPath (Join-Path $root "$label.log") -EvidenceRoot $root
            $lanes.Add($lane)
        }
        $results = @($lanes | ForEach-Object { Wait-NativeLane -Lane $_ })
        $clock.Stop()
        $allSamples = @($results | ForEach-Object { $_.resource_samples })
        return [ordered]@{
            wave_index = $WaveIndex
            wall_seconds = $clock.Elapsed.TotalSeconds
            episode_count = [uint64]($Members.Count * $script:EpisodesPerSeed)
            episodes_per_second = ($Members.Count * [double]$script:EpisodesPerSeed) / $clock.Elapsed.TotalSeconds
            resource_summary = Get-ResourceSummary -Samples $allSamples
            lanes = @($results)
        }
    }
    catch {
        foreach ($lane in $lanes) {
            Stop-NativeLane -Lane $lane
        }
        throw
    }
}

$phase = 'preflight'
try {
    Assert-ExclusiveWindow
    $git = Get-GitRecord -RepoRoot $script:RepoRoot
    $toolchain = Get-ToolchainRecord
    $cuda = Get-CudaRecord
    $inputs = Get-InputRecord
    $harness = Get-HarnessRecord
    $designFile = Assert-FileSha256 -Path $DesignDocumentPath -ExpectedSha256 '1f6ea9128e7d5b44f80c34c96c42ccd47325224fb11d22bc4aae0d43cef40b00' -Label 'scientific design document'
    $gpu1 = Assert-Gpu1Idle
    $gpu0 = Assert-GpuIdentity -Ordinal 0
    $prelaunchResources = Assert-PrelaunchResourceWindow
    Assert-NoForeignGpu1ComputeProcesses

    $gate3File = Assert-FileSha256 -Path $Gate3ManifestPath -ExpectedSha256 $script:ExpectedGate3Sha256 -Label 'Gate 3 manifest'
    $gate3 = Get-Content -LiteralPath $Gate3ManifestPath -Raw | ConvertFrom-Json
    if ($gate3.passed -ne $true -or [string]$gate3.disposition -ne 'PASS' -or
        [string]$gate3.selected_beta -ne $script:SelectedBeta -or $gate3.terminal_outcomes_read -ne $false) {
        throw 'Gate 3 does not authorize the selected terminal-blind beta'
    }
    $executable = [string]$gate3.executable.path
    $executableFile = Assert-FileSha256 -Path $executable -ExpectedSha256 $script:ExpectedTrainingExecutableSha256 -Label 'selected training executable'
    if ([string]$gate3.executable.sha256 -ne $script:ExpectedTrainingExecutableSha256) {
        throw 'Gate 3 executable binding mismatch'
    }

    $gate4File = Assert-FileSha256 -Path $Gate4ManifestPath -ExpectedSha256 $script:ExpectedGate4Sha256 -Label 'Gate 4 manifest'
    $gate4 = Get-Content -LiteralPath $Gate4ManifestPath -Raw | ConvertFrom-Json
    if ($gate4.passed -ne $true -or [string]$gate4.disposition -ne 'PASS' -or
        [string]$gate4.selected_beta -ne $script:SelectedBeta -or
        [string]$gate4.prerequisite_coefficient.sha256 -ne $script:ExpectedGate3Sha256) {
        throw 'Gate 4 gross-safety prerequisite is not a chained PASS'
    }

    $throughputFile = Assert-FileSha256 -Path $ThroughputManifestPath -ExpectedSha256 $script:ExpectedThroughputSha256 -Label 'training throughput manifest'
    $throughput = Get-Content -LiteralPath $ThroughputManifestPath -Raw | ConvertFrom-Json
    if ($throughput.passed -ne $true -or
        [string]$throughput.selected_topology -ne 'gpu0+gpu1' -or
        $throughput.same_device_repeat_bit_identical -ne $true -or
        $throughput.cross_device_same_seed_bit_identical -ne $true -or
        [string]$gate3.prerequisite_throughput.manifest_sha256 -ne $script:ExpectedThroughputSha256) {
        throw 'the selected dual-GPU training topology is not bound to a passed deterministic throughput screen'
    }
    $singlePoint = @($throughput.points | Where-Object { $_.topology -eq 'gpu1-only' })
    $dualPoint = @($throughput.points | Where-Object { $_.topology -eq 'gpu0+gpu1' })
    if ($singlePoint.Count -ne 1 -or $dualPoint.Count -ne 1) {
        throw 'throughput manifest does not contain exactly one single-GPU and one dual-GPU point'
    }
    $projectedWave0Seconds = (2.0 * $script:EpisodesPerSeed) / [double]$dualPoint[0].episodes_per_second_aggregate
    $projectedWave1Seconds = $script:EpisodesPerSeed / [double]$singlePoint[0].episodes_per_second

    $controlRecords = @(
        foreach ($seed in @([uint64]970001, [uint64]970002, [uint64]970003)) {
            $controlRoot = "D:\mtg-kernel-macro-selfplay-envrand-v2-rung-v1\runs\seed-$seed\run-0\store"
            Get-StoreRecord -Seed $seed -StoreRoot $controlRoot -Role 'control'
        }
    )
    $waves = @(
        [ordered]@{ wave_index = 0; members = @(
            [ordered]@{ seed = [uint64]970001; gpu = 0 },
            [ordered]@{ seed = [uint64]970002; gpu = 1 }
        ) },
        [ordered]@{ wave_index = 1; members = @(
            [ordered]@{ seed = [uint64]970003; gpu = 1 }
        ) }
    )
    $plan = [ordered]@{
        schema = 'regularized-continuation-full-horizon-training-plan/v1'
        status = 'preflight complete; development training not started'
        created_utc = [DateTimeOffset]::UtcNow.ToString('O')
        design = [ordered]@{
            commit = 'e9bd7e5b4ef7b8320bb22edfc573ba50a8496ba7'
            document = $designFile
        }
        git = $git
        toolchain = $toolchain
        cuda = $cuda
        executable = $executableFile
        harness = $harness
        inputs = $inputs
        prerequisites = [ordered]@{
            coefficient_screen = $gate3File
            gross_safety = $gate4File
            throughput = $throughputFile
        }
        prelaunch_gpus = @($gpu0, $gpu1)
        prelaunch_resources = $prelaunchResources
        selected_beta = $script:SelectedBeta
        updates_per_seed = $script:Updates
        episodes_per_seed = $script:EpisodesPerSeed
        training_seeds = @([uint64]970001, [uint64]970002, [uint64]970003)
        required_generations = $script:RequiredGenerations
        controls = $controlRecords
        topology = [ordered]@{
            selected = 'two concurrent seeds on GPU 0 and GPU 1, then the remaining seed on GPU 1'
            aggregate_speedup = [double]$throughput.aggregate_speedup
            measured_gpu1_episodes_per_second = [double]$singlePoint[0].episodes_per_second
            measured_dual_gpu_episodes_per_second = [double]$dualPoint[0].episodes_per_second_aggregate
            projected_wave_0_seconds = $projectedWave0Seconds
            projected_wave_1_seconds = $projectedWave1Seconds
            projected_total_seconds = $projectedWave0Seconds + $projectedWave1Seconds
            waves = $waves
        }
        fixed = [ordered]@{
            architecture = 'kernel-policy-value-net-8'
            loss = 'terminal_reinforce_value/v3'
            reward_and_strength_signal = 'terminal W/L/D only'
            pool = 'Pool3'
            environment_randomization = 'v2'
            schedule = '2 workers, 32 sessions, broker target 16, 64 episodes per update'
            parent = 'promoted(2) generation 384'
        }
        terminal_outcomes_read = $false
    }
    $planPath = Join-Path $root 'training-plan.json'
    Write-JsonFile -Value $plan -Path $planPath
    if ($PreflightOnly) {
        Write-Host "Full-horizon training preflight complete: $planPath"
        return
    }

    $phase = 'development-training'
    Write-JsonFile -Value ([ordered]@{
        schema = 'regularized-continuation-full-horizon-training-start/v1'
        utc = [DateTimeOffset]::UtcNow.ToString('O')
        plan_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $planPath).Hash.ToLowerInvariant()
    }) -Path (Join-Path $root 'training-start.json')

    $campaignClock = [System.Diagnostics.Stopwatch]::StartNew()
    $trainingWaves = New-Object System.Collections.Generic.List[object]
    foreach ($wave in $waves) {
        Assert-LaunchBindingsUnchanged -ExecutablePath $executable -ExpectedExecutable $executableFile -ExpectedInputs $inputs -ExpectedHarness $harness -ExpectedDesign $designFile
        $trainingWaves.Add((Invoke-TrainingWave -WaveIndex $wave.wave_index -Members @($wave.members) -Executable $executable))
    }
    $campaignClock.Stop()
    Assert-LaunchBindingsUnchanged -ExecutablePath $executable -ExpectedExecutable $executableFile -ExpectedInputs $inputs -ExpectedHarness $harness -ExpectedDesign $designFile
    Assert-GpuIdentity -Ordinal 0 | Out-Null
    Assert-Gpu1Idle | Out-Null
    Assert-NoForeignGpu1ComputeProcesses

    $candidateRecords = @(
        foreach ($lane in @($trainingWaves | ForEach-Object { $_.lanes })) {
            Get-StoreRecord -Seed ([uint64]$lane.seed) -StoreRoot (Join-Path $lane.store_parent 'run-0\store') -Role 'candidate'
        }
    ) | Sort-Object seed
    if ($candidateRecords.Count -ne 3) {
        throw 'full-horizon training did not produce exactly three candidate Stores'
    }
    $finalControlRecords = @(
        foreach ($control in $controlRecords) {
            $record = Get-StoreRecord -Seed ([uint64]$control.seed) -StoreRoot ([string]$control.store_root) -Role 'control'
            if ([string]$record.store_tree_sha256 -ne [string]$control.store_tree_sha256 -or
                [uint64]$record.store_file_count -ne [uint64]$control.store_file_count) {
                throw "beta-zero control Store changed during training for seed $($control.seed)"
            }
            $record
        }
    )
    $laneRecords = @(
        foreach ($wave in $trainingWaves) {
            [ordered]@{
                wave_index = $wave.wave_index
                wall_seconds = $wave.wall_seconds
                episode_count = $wave.episode_count
                episodes_per_second = $wave.episodes_per_second
                resource_summary = $wave.resource_summary
                lanes = @($wave.lanes | ForEach-Object { Get-LaneRecord -Lane $_ })
            }
        }
    )
    $phase = 'complete'
    $manifest = [ordered]@{
        schema = 'regularized-continuation-full-horizon-training/v1'
        passed = $true
        disposition = 'TRAINING-COMPLETE; DEVELOPMENT-EVALUATION-RELEASED'
        completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
        plan = Get-FileRecord -Path $planPath
        git = $git
        toolchain = $toolchain
        cuda = $cuda
        executable = $executableFile
        harness = $harness
        prerequisites = [ordered]@{
            coefficient_screen = $gate3File
            gross_safety = $gate4File
            throughput = $throughputFile
        }
        selected_beta = $script:SelectedBeta
        training_seeds = @([uint64]970001, [uint64]970002, [uint64]970003)
        updates_per_seed = $script:Updates
        episodes_per_seed = $script:EpisodesPerSeed
        total_episode_count = [uint64](3 * $script:EpisodesPerSeed)
        wall_seconds = $campaignClock.Elapsed.TotalSeconds
        aggregate_episodes_per_second = (3.0 * $script:EpisodesPerSeed) / $campaignClock.Elapsed.TotalSeconds
        projected_wall_seconds = $projectedWave0Seconds + $projectedWave1Seconds
        topology = 'gpu0+gpu1, then gpu1'
        waves = $laneRecords
        controls = $finalControlRecords
        candidates = $candidateRecords
        terminal_outcomes_read = $false
        nonclaim = 'Training completion is not playing-strength evidence and does not nominate or promote a policy.'
    }
    $manifestPath = Join-Path $root 'training-manifest.json'
    Write-JsonFile -Value $manifest -Path $manifestPath
    Write-Host "Full-horizon training PASS: $manifestPath"
}
catch {
    $line = "$([DateTimeOffset]::UtcNow.ToString('O')) phase=$phase VOID environmental_or_harness_interruption=$($_.Exception.Message)"
    $line | Set-Content -LiteralPath (Join-Path $root "void-$phase.log") -Encoding utf8
    throw
}
