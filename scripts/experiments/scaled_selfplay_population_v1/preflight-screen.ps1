param(
    [string]$EvidenceRoot = 'D:\mtg-kernel-scaled-selfplay-population-v1\preflight',
    [string]$ExecutablePath = '',
    [string]$ExecutableSourceCommit = '',
    [string]$RetestStoreRoot = ''
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
$root = New-UniqueAttemptRoot -EvidenceRoot $EvidenceRoot -GateName 'identity-throughput-screen'

function Get-FileRecordV1 {
    param([Parameter(Mandatory = $true)][string]$Path)
    $item = Get-Item -LiteralPath $Path
    return [ordered]@{
        path = $item.FullName
        bytes = [uint64]$item.Length
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $item.FullName).Hash.ToLowerInvariant()
    }
}

function Get-ResourceSummaryV1 {
    param([Parameter(Mandatory = $true)][object[]]$Samples)
    $rows = @($Samples)
    $gpuRows = @(
        foreach ($ordinal in @(0, 1)) {
            $matches = @($rows | ForEach-Object { $_.gpus } | Where-Object { $_.ordinal -eq $ordinal })
            if ($matches.Count -ne 0) {
                [ordered]@{
                    ordinal = $ordinal
                    samples = $matches.Count
                    utilization_mean_percent = ($matches | ForEach-Object { [double]$_['utilization_percent'] } | Measure-Object -Average).Average
                    utilization_peak_percent = ($matches | ForEach-Object { [double]$_['utilization_percent'] } | Measure-Object -Maximum).Maximum
                    memory_peak_mib = ($matches | ForEach-Object { [double]$_['memory_used_mib'] } | Measure-Object -Maximum).Maximum
                }
            }
        }
    )
    return [ordered]@{
        samples = $rows.Count
        cpu_mean_percent = ($rows | ForEach-Object { [double]$_['cpu_total_percent'] } | Measure-Object -Average).Average
        cpu_peak_percent = ($rows | ForEach-Object { [double]$_['cpu_total_percent'] } | Measure-Object -Maximum).Maximum
        host_memory_peak_fraction = ($rows | ForEach-Object {
            $_.host_memory_used_mib / [double]$_.host_memory_total_mib
        } | Measure-Object -Maximum).Maximum
        gpus = $gpuRows
    }
}

function Invoke-TwoPhaseLaneV1 {
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][ValidateSet('retest', 'successor')][string]$Mode,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $parent = Join-Path $root $Label
    $phase1 = Start-ScaledNativeLane -Executable $Executable -Seed 970001 -Updates $(if ($Mode -eq 'retest') { 512 } else { 1536 }) -StoreParent $parent -GpuOrdinal 1 -Mode $Mode -StopAfterGeneration 4 -LogPath (Join-Path $root "$Label-phase1.log") -EvidenceRoot $root
    $phase1Result = Wait-NativeLane -Lane $phase1
    $generation4 = Get-ScaledEndpointRecord -StoreRoot (Join-Path $parent 'run-0\store') -Generation 4
    $phase2 = Start-ScaledNativeLane -Executable $Executable -Seed 970001 -Updates $(if ($Mode -eq 'retest') { 512 } else { 1536 }) -StoreParent $parent -GpuOrdinal 1 -Mode $Mode -ExpectedResumeGeneration 4 -StopAfterGeneration 8 -ResumeExistingStore -LogPath (Join-Path $root "$Label-phase2.log") -EvidenceRoot $root
    $phase2Result = Wait-NativeLane -Lane $phase2
    $phase2Text = Get-Content -Raw -LiteralPath $phase2.log
    if ($phase2Text -notmatch 'STORE CLOSE_REOPEN resume_generation=4') {
        throw "$Mode close/reopen marker is absent"
    }
    return [ordered]@{
        mode = $Mode
        generation4 = $generation4
        generation8 = Get-ScaledEndpointRecord -StoreRoot (Join-Path $parent 'run-0\store') -Generation 8
        wall_seconds = [double]$phase1Result.wall_seconds + [double]$phase2Result.wall_seconds
        phases = @($phase1Result, $phase2Result)
        resource_summary = Get-ResourceSummaryV1 -Samples @($phase1Result.resource_samples + $phase2Result.resource_samples)
    }
}

$phase = 'preflight'
$activeLanes = New-Object System.Collections.Generic.List[object]
try {
    Assert-ExclusiveWindow
    $git = Get-GitRecord -RepoRoot $script:RepoRoot
    $toolchain = Get-ToolchainRecord
    $cuda = Get-CudaRecord
    $inputs = Get-InputRecord
    $gpu0 = Assert-GpuIdentity -Ordinal 0
    $gpu1 = Assert-Gpu1Idle
    $prelaunch = Assert-PrelaunchResourceWindow
    Assert-NoForeignGpu1ComputeProcesses
    if ([string]::IsNullOrWhiteSpace($ExecutablePath)) {
        $executable = Get-ReleaseTestExecutable -RepoRoot $script:RepoRoot -EvidenceRoot $root -Label 'scaled-population'
        $ExecutableSourceCommit = [string]$git.commit
    }
    else {
        if ([string]::IsNullOrWhiteSpace($ExecutableSourceCommit)) {
            throw 'ExecutableSourceCommit is required with ExecutablePath'
        }
        $executable = (Resolve-Path -LiteralPath $ExecutablePath).Path
        & git -C $script:RepoRoot merge-base --is-ancestor $ExecutableSourceCommit ([string]$git.commit)
        Assert-LastExitCode $LASTEXITCODE 'executable source commit ancestry'
    }
    $executableRecord = Get-FileRecordV1 -Path $executable
    $executableRecord['source_commit'] = $ExecutableSourceCommit

    $phase = 'matched-identity'
    if ([string]::IsNullOrWhiteSpace($RetestStoreRoot)) {
        $retest = Invoke-TwoPhaseLaneV1 -Executable $executable -Mode retest -Label 'identity-retest'
    }
    else {
        $retestRoot = (Resolve-Path -LiteralPath $RetestStoreRoot).Path
        $retest = [ordered]@{
            mode = 'retest'
            reused = $true
            generation4 = Get-ScaledEndpointRecord -StoreRoot $retestRoot -Generation 4
            generation8 = Get-ScaledEndpointRecord -StoreRoot $retestRoot -Generation 8
            source_store_root = $retestRoot
        }
    }
    $successor = Invoke-TwoPhaseLaneV1 -Executable $executable -Mode successor -Label 'identity-successor'
    $generation4Identical = $retest.generation4.state_sha256 -eq $successor.generation4.state_sha256 -and
        $retest.generation4.model_parameter_sha256 -eq $successor.generation4.model_parameter_sha256
    $generation8Identical = $retest.generation8.state_sha256 -eq $successor.generation8.state_sha256 -and
        $retest.generation8.model_parameter_sha256 -eq $successor.generation8.model_parameter_sha256
    if (-not ($generation4Identical -and $generation8Identical)) {
        throw 'successor record changed native learner state before population activation'
    }

    $phase = 'dual-topology'
    $dualClock = [System.Diagnostics.Stopwatch]::StartNew()
    foreach ($ordinal in @(0, 1)) {
        $label = "dual-gpu$ordinal"
        $activeLanes.Add((Start-ScaledNativeLane -Executable $executable -Seed 970001 -Updates 1536 -StoreParent (Join-Path $root $label) -GpuOrdinal $ordinal -Mode successor -StopAfterGeneration 8 -LogPath (Join-Path $root "$label.log") -EvidenceRoot $root))
    }
    $dualResults = @($activeLanes | ForEach-Object { Wait-NativeLane -Lane $_ })
    $dualClock.Stop()
    $activeLanes.Clear()
    $dualEndpoints = @(
        foreach ($result in $dualResults) {
            [ordered]@{
                gpu_ordinal = [int]$result.gpu_ordinal
                endpoint = Get-ScaledEndpointRecord -StoreRoot (Join-Path $result.store_parent 'run-0\store') -Generation 8
                lane = $result
            }
        }
    )
    $dualGpu0 = $dualEndpoints | Where-Object { $_.gpu_ordinal -eq 0 } | Select-Object -First 1
    $dualGpu1 = $dualEndpoints | Where-Object { $_.gpu_ordinal -eq 1 } | Select-Object -First 1
    $sameDeviceIdentical = $successor.generation8.tree_sha256 -eq $dualGpu1.endpoint.tree_sha256
    $crossDeviceIdentical = $dualGpu0.endpoint.tree_sha256 -eq $dualGpu1.endpoint.tree_sha256
    $singleRate = 512.0 / [double]$successor.wall_seconds
    $dualRate = 1024.0 / [double]$dualClock.Elapsed.TotalSeconds
    $speedup = $dualRate / $singleRate
    $dualSamples = @($dualResults | ForEach-Object { $_.resource_samples })
    $resourceSafe = @($dualSamples | Where-Object {
        $_.host_memory_total_mib -le 0 -or
        ($_.host_memory_used_mib / [double]$_.host_memory_total_mib) -gt 0.90 -or
        @($_.gpus | Where-Object {
            $_.memory_total_mib -le 0 -or ($_.memory_used_mib / [double]$_.memory_total_mib) -gt 0.95
        }).Count -ne 0
    }).Count -eq 0
    $selected = if ($resourceSafe -and $sameDeviceIdentical -and $crossDeviceIdentical -and $speedup -ge 1.5) { 'gpu0+gpu1' } else { 'gpu1-only' }
    if (-not $sameDeviceIdentical) { throw 'successor Store was not bit-identical across close/reopen and direct repeat on GPU 1' }

    $phase = 'complete'
    $manifest = [ordered]@{
        schema = 'scaled-selfplay-population-identity-throughput-screen/v1'
        passed = $true
        completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
        git = $git
        toolchain = $toolchain
        cuda = $cuda
        executable = $executableRecord
        inputs = $inputs
        prelaunch_gpus = @($gpu0, $gpu1)
        prelaunch_resources = $prelaunch
        identity = [ordered]@{
            seed = 970001
            generations = @(4, 8)
            checkpoint_close_reopen = $true
            generation4_native_state_identical = $generation4Identical
            generation8_native_state_identical = $generation8Identical
            retest = $retest
            successor = $successor
        }
        topology = [ordered]@{
            rule = 'select gpu0+gpu1 only when resource-safe, same-seed Stores bit-identical, and aggregate speedup >= 1.5; otherwise gpu1-only'
            selected = $selected
            single_gpu1_wall_seconds = $successor.wall_seconds
            single_gpu1_episodes_per_second = $singleRate
            dual_wall_seconds = $dualClock.Elapsed.TotalSeconds
            dual_episodes_per_second = $dualRate
            aggregate_speedup = $speedup
            resource_safe = $resourceSafe
            same_device_repeat_bit_identical = $sameDeviceIdentical
            cross_device_same_seed_bit_identical = $crossDeviceIdentical
            dual_resource_summary = Get-ResourceSummaryV1 -Samples $dualSamples
            dual = $dualEndpoints
        }
        expected_replay_episodes = 98304
        projected_replay_wall_seconds = if ($selected -eq 'gpu0+gpu1') {
            65536.0 / $dualRate + 32768.0 / $singleRate
        } else {
            98304.0 / $singleRate
        }
        terminal_outcomes_read = $false
        nonclaim = 'This screen proves mechanical identity and throughput only, not playing strength.'
    }
    $manifestPath = Join-Path $root 'screen-manifest.json'
    Write-JsonFile -Value $manifest -Path $manifestPath
    Write-Host "SCALED PREFLIGHT PASS selected=$selected speedup=$([math]::Round($speedup, 3)) evidence=$manifestPath"
}
catch {
    foreach ($lane in $activeLanes) { Stop-NativeLane -Lane $lane }
    "$([DateTimeOffset]::UtcNow.ToString('O')) phase=$phase VOID=$($_.Exception.Message)" |
        Set-Content -LiteralPath (Join-Path $root "void-$phase.log") -Encoding utf8
    throw
}
