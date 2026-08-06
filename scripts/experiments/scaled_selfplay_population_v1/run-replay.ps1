param(
    [Parameter(Mandatory = $true)][string]$ThroughputManifestPath,
    [Parameter(Mandatory = $true)][string]$ExecutablePath,
    [Parameter(Mandatory = $true)][string]$ExecutableSourceCommit,
    [string]$EvidenceRoot = 'D:\mtg-kernel-scaled-selfplay-population-v1\replay'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
$root = New-UniqueAttemptRoot -EvidenceRoot $EvidenceRoot -GateName 'three-lineage-replay'
$active = New-Object System.Collections.Generic.List[object]
$phase = 'preflight'

function Get-FileRecordV1 {
    param([Parameter(Mandatory = $true)][string]$Path)
    $item = Get-Item -LiteralPath $Path
    return [ordered]@{
        path = $item.FullName
        bytes = [uint64]$item.Length
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $item.FullName).Hash.ToLowerInvariant()
    }
}

function Get-ReplayResourceSummaryV1 {
    param([Parameter(Mandatory = $true)][object[]]$Samples)
    $rows = @($Samples)
    return [ordered]@{
        sample_count = $rows.Count
        cpu_mean_percent = ($rows | ForEach-Object { [double]$_['cpu_total_percent'] } | Measure-Object -Average).Average
        cpu_peak_percent = ($rows | ForEach-Object { [double]$_['cpu_total_percent'] } | Measure-Object -Maximum).Maximum
        host_memory_peak_fraction = ($rows | ForEach-Object {
            [double]$_['host_memory_used_mib'] / [double]$_['host_memory_total_mib']
        } | Measure-Object -Maximum).Maximum
        gpus = @(
            foreach ($ordinal in @(0, 1)) {
                $matches = @($rows | ForEach-Object { $_['gpus'] } | Where-Object { [int]$_['ordinal'] -eq $ordinal })
                [ordered]@{
                    ordinal = $ordinal
                    utilization_mean_percent = ($matches | ForEach-Object { [double]$_['utilization_percent'] } | Measure-Object -Average).Average
                    utilization_peak_percent = ($matches | ForEach-Object { [double]$_['utilization_percent'] } | Measure-Object -Maximum).Maximum
                    memory_peak_mib = ($matches | ForEach-Object { [double]$_['memory_used_mib'] } | Measure-Object -Maximum).Maximum
                }
            }
        )
    }
}

function Invoke-ReplayWaveV1 {
    param(
        [Parameter(Mandatory = $true)][int]$WaveIndex,
        [Parameter(Mandatory = $true)][object[]]$Members,
        [Parameter(Mandatory = $true)][string]$Executable
    )
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    foreach ($member in $Members) {
        $label = "wave-$('{0:d2}' -f $WaveIndex)-seed-$($member.seed)-gpu$($member.gpu)"
        $active.Add((Start-ScaledNativeLane -Executable $Executable -Seed ([uint64]$member.seed) -Updates 1536 -StoreParent (Join-Path $root $label) -GpuOrdinal ([int]$member.gpu) -Mode successor -StopAfterGeneration 512 -LogPath (Join-Path $root "$label.log") -EvidenceRoot $root))
    }
    $results = @($active | ForEach-Object { Wait-NativeLane -Lane $_ })
    $active.Clear()
    $clock.Stop()
    $samples = @($results | ForEach-Object { $_.resource_samples })
    return [ordered]@{
        wave_index = $WaveIndex
        wall_seconds = $clock.Elapsed.TotalSeconds
        episode_count = [uint64](32768 * $Members.Count)
        episodes_per_second = (32768.0 * $Members.Count) / $clock.Elapsed.TotalSeconds
        resources = Get-ReplayResourceSummaryV1 -Samples $samples
        lanes = @($results | ForEach-Object {
            [ordered]@{
                seed = $_.seed
                gpu_ordinal = $_.gpu_ordinal
                store_parent = $_.store_parent
                wall_seconds = $_.wall_seconds
                log = Get-FileRecordV1 -Path $_.log
                completion = Get-FileRecordV1 -Path $_.completion.path
            }
        })
    }
}

try {
    Assert-ExclusiveWindow
    $git = Get-GitRecord -RepoRoot $script:RepoRoot
    $toolchain = Get-ToolchainRecord
    $cuda = Get-CudaRecord
    $inputs = Get-InputRecord
    $throughputPath = (Resolve-Path -LiteralPath $ThroughputManifestPath).Path
    $throughput = Get-Content -Raw -LiteralPath $throughputPath | ConvertFrom-Json
    if ($throughput.passed -ne $true -or
        [string]$throughput.topology.selected -ne 'gpu0+gpu1' -or
        [double]$throughput.topology.aggregate_speedup -lt 1.5 -or
        $throughput.topology.resource_safe -ne $true -or
        $throughput.topology.same_device_repeat_bit_identical -ne $true -or
        $throughput.topology.cross_device_same_seed_bit_identical -ne $true -or
        $throughput.identity.generation4_native_state_identical -ne $true -or
        $throughput.identity.generation8_native_state_identical -ne $true) {
        throw 'corrected identity and throughput prerequisite is not a dual-GPU PASS'
    }
    $executable = (Resolve-Path -LiteralPath $ExecutablePath).Path
    $executableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $executable).Hash.ToLowerInvariant()
    if ($executableHash -ne [string]$throughput.executable.sha256 -or
        $ExecutableSourceCommit -ne [string]$throughput.executable.source_commit) {
        throw 'replay executable does not match corrected throughput authority'
    }
    & git -C $script:RepoRoot merge-base --is-ancestor $ExecutableSourceCommit ([string]$git.commit)
    Assert-LastExitCode $LASTEXITCODE 'executable source commit ancestry'
    Assert-Gpu1Idle | Out-Null
    Assert-GpuIdentity -Ordinal 0 | Out-Null
    Assert-NoForeignGpu1ComputeProcesses

    $plan = [ordered]@{
        schema = 'scaled-selfplay-population-replay-plan/v1'
        created_utc = [DateTimeOffset]::UtcNow.ToString('O')
        git = $git
        toolchain = $toolchain
        cuda = $cuda
        executable = [ordered]@{ path = $executable; sha256 = $executableHash; source_commit = $ExecutableSourceCommit }
        throughput = Get-FileRecordV1 -Path $throughputPath
        inputs = $inputs
        seeds = @([uint64]970001, [uint64]970002, [uint64]970003)
        target_generation = 1536
        stop_after_generation = 512
        episode_count = 98304
        expected_wall_seconds = 10836.1347
        topology = 'gpu0+gpu1, then gpu1'
        terminal_outcomes_read = $false
    }
    $planPath = Join-Path $root 'replay-plan.json'
    Write-JsonFile -Value $plan -Path $planPath

    $phase = 'replay-wave-0'
    $campaignClock = [System.Diagnostics.Stopwatch]::StartNew()
    $wave0 = Invoke-ReplayWaveV1 -WaveIndex 0 -Executable $executable -Members @(
        [ordered]@{ seed = [uint64]970001; gpu = 0 },
        [ordered]@{ seed = [uint64]970002; gpu = 1 }
    )
    $phase = 'replay-wave-1'
    $wave1 = Invoke-ReplayWaveV1 -WaveIndex 1 -Executable $executable -Members @(
        [ordered]@{ seed = [uint64]970003; gpu = 1 }
    )
    $campaignClock.Stop()
    Assert-Gpu1Idle | Out-Null
    Assert-NoForeignGpu1ComputeProcesses

    $phase = 'handoff-validation'
    $stores = [ordered]@{}
    foreach ($wave in @($wave0, $wave1)) {
        foreach ($laneRecord in $wave.lanes) {
            $store = Join-Path $laneRecord.store_parent 'run-0\store'
            Assert-GenerationCheckpoint -Store $store -Generation 512
            $stores[[string]$laneRecord.seed] = (Resolve-Path -LiteralPath $store).Path
        }
    }
    $handoffPath = Join-Path $root 'replay-handoff-manifest.json'
    $assembler = Join-Path $PSScriptRoot 'assemble_replay_handoff.py'
    $assemblerStderr = Join-Path $root 'assemble-replay-handoff.stderr.log'
    & python $assembler --output $handoffPath `
        --lineage "970001=$($stores['970001'])" `
        --lineage "970002=$($stores['970002'])" `
        --lineage "970003=$($stores['970003'])" 2> $assemblerStderr
    Assert-LastExitCode $LASTEXITCODE "assemble replay handoff; see $assemblerStderr"
    $validator = Join-Path $PSScriptRoot 'validate_replay_handoff.py'
    $validatorStderr = Join-Path $root 'validate-replay-handoff.stderr.log'
    $validationText = @(& python $validator $handoffPath 2> $validatorStderr) -join "`n"
    Assert-LastExitCode $LASTEXITCODE "validate replay handoff; see $validatorStderr"
    $validation = $validationText | ConvertFrom-Json
    if ($validation.continuation_authorized -ne $true -or [string]$validation.disposition -ne 'ADVANCE') {
        throw 'all-three replay handoff did not ADVANCE'
    }
    $validationPath = Join-Path $root 'replay-handoff-validation.json'
    [IO.File]::WriteAllText($validationPath, $validationText, [Text.UTF8Encoding]::new($false))

    $phase = 'complete'
    $manifest = [ordered]@{
        schema = 'scaled-selfplay-population-replay-execution/v1'
        passed = $true
        disposition = 'REPLAY-BIT-MATCH-ADVANCE'
        completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
        plan = Get-FileRecordV1 -Path $planPath
        git = $git
        executable = $plan.executable
        throughput = $plan.throughput
        episode_count = 98304
        wall_seconds = $campaignClock.Elapsed.TotalSeconds
        aggregate_episodes_per_second = 98304.0 / $campaignClock.Elapsed.TotalSeconds
        topology = $plan.topology
        waves = @($wave0, $wave1)
        handoff_manifest = Get-FileRecordV1 -Path $handoffPath
        handoff_validation = Get-FileRecordV1 -Path $validationPath
        terminal_outcomes_read = $false
        nonclaim = 'Replay completion and bit identity are mechanical gates, not playing-strength evidence.'
    }
    $manifestPath = Join-Path $root 'replay-execution-manifest.json'
    Write-JsonFile -Value $manifest -Path $manifestPath
    Write-Host "REPLAY PASS evidence=$manifestPath"
}
catch {
    foreach ($lane in $active) { Stop-NativeLane -Lane $lane }
    "$([DateTimeOffset]::UtcNow.ToString('O')) phase=$phase VOID=$($_.Exception.Message)" |
        Set-Content -LiteralPath (Join-Path $root "void-$phase.log") -Encoding utf8
    throw
}
