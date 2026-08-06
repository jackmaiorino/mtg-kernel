param(
    [Parameter(Mandatory = $true)][string]$PrerequisiteIntervalManifestPath,
    [Parameter(Mandatory = $true)][string]$PayoffMatrixManifestPath,
    [Parameter(Mandatory = $true)][string]$WeightUpdatePath,
    [Parameter(Mandatory = $true)][string[]]$RefreshManifestPaths,
    [Parameter(Mandatory = $true)][string]$ThroughputManifestPath,
    [Parameter(Mandatory = $true)][string]$ExecutablePath,
    [Parameter(Mandatory = $true)][string]$ExecutableSourceCommit,
    [Parameter(Mandatory = $true)][uint64]$StartGeneration,
    [Parameter(Mandatory = $true)][uint64]$StopGeneration,
    [string]$EvidenceRoot = 'D:\mtg-kernel-scaled-selfplay-population-v1\active'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
$gateName = "interval-$('{0:d4}' -f $StartGeneration)-$('{0:d4}' -f $StopGeneration)"
$root = New-UniqueAttemptRoot -EvidenceRoot $EvidenceRoot -GateName $gateName
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

function Invoke-PopulationWaveV1 {
    param(
        [Parameter(Mandatory = $true)][int]$WaveIndex,
        [Parameter(Mandatory = $true)][object[]]$Members,
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string]$RefreshChain,
        [Parameter(Mandatory = $true)][string]$SlotRoots
    )
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    foreach ($member in $Members) {
        $label = "wave-$('{0:d2}' -f $WaveIndex)-seed-$($member.seed)-gpu$($member.gpu)"
        $active.Add((Start-ScaledNativeLane -Executable $Executable -Seed ([uint64]$member.seed) -Updates 1536 -StoreParent ([string]$member.store_parent) -GpuOrdinal ([int]$member.gpu) -Mode population -ExpectedResumeGeneration $StartGeneration -StopAfterGeneration $StopGeneration -RefreshChain $RefreshChain -SlotRoots $SlotRoots -ResumeExistingStore -LogPath (Join-Path $root "$label.log") -EvidenceRoot $root))
    }
    $results = @($active | ForEach-Object { Wait-NativeLane -Lane $_ })
    $active.Clear()
    $clock.Stop()
    $episodesPerLineage = [uint64](($StopGeneration - $StartGeneration) * 64)
    return [ordered]@{
        wave_index = $WaveIndex
        wall_seconds = $clock.Elapsed.TotalSeconds
        episode_count = [uint64]($episodesPerLineage * $Members.Count)
        episodes_per_second = ($episodesPerLineage * $Members.Count) / $clock.Elapsed.TotalSeconds
        lanes = @($results | ForEach-Object {
            $store = Join-Path $_.store_parent 'run-0\store'
            Assert-GenerationCheckpoint -Store $store -Generation $StopGeneration
            [ordered]@{
                seed = $_.seed
                gpu_ordinal = $_.gpu_ordinal
                store_root = (Resolve-Path -LiteralPath $store).Path
                store_tree_sha256 = Get-StoreTreeHash -Path $store
                wall_seconds = $_.wall_seconds
                log = Get-FileRecordV1 -Path $_.log
                completion = Get-FileRecordV1 -Path $_.completion.path
            }
        })
    }
}

try {
    if ($StopGeneration -ne $StartGeneration + 128) {
        throw 'population intervals must contain exactly 128 updates'
    }
    Assert-ExclusiveWindow
    $git = Get-GitRecord -RepoRoot $script:RepoRoot
    $prerequisitePath = (Resolve-Path -LiteralPath $PrerequisiteIntervalManifestPath).Path
    $prerequisite = Get-Content -Raw -LiteralPath $prerequisitePath | ConvertFrom-Json
    if ($prerequisite.passed -ne $true -or [string]$prerequisite.disposition -notmatch "INTERVAL-0*$StartGeneration-COMPLETE") {
        throw 'prerequisite interval is not complete at the requested start generation'
    }
    $matrixPath = (Resolve-Path -LiteralPath $PayoffMatrixManifestPath).Path
    $matrix = Get-Content -Raw -LiteralPath $matrixPath | ConvertFrom-Json
    if ($matrix.passed -ne $true -or [string]$matrix.disposition -ne "PAYOFF-MATRIX-$StartGeneration-COMPLETE") {
        throw 'required complete payoff matrix is absent'
    }
    $weightUpdatePath = (Resolve-Path -LiteralPath $WeightUpdatePath).Path
    $weightUpdate = Get-Content -Raw -LiteralPath $weightUpdatePath | ConvertFrom-Json
    $throughputPath = (Resolve-Path -LiteralPath $ThroughputManifestPath).Path
    $throughput = Get-Content -Raw -LiteralPath $throughputPath | ConvertFrom-Json
    if ($throughput.passed -ne $true -or [string]$throughput.topology.selected -ne 'gpu0+gpu1') {
        throw 'dual-GPU topology prerequisite is absent'
    }

    $refreshPaths = @($RefreshManifestPaths | ForEach-Object { (Resolve-Path -LiteralPath $_).Path })
    if ($refreshPaths.Count -lt 2) {
        throw 'population continuation requires the full refresh chain'
    }
    $refreshes = @($refreshPaths | ForEach-Object { Get-Content -Raw -LiteralPath $_ | ConvertFrom-Json })
    for ($index = 1; $index -lt $refreshes.Count; $index++) {
        $previousHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $refreshPaths[$index - 1]).Hash.ToLowerInvariant()
        if ([string]$refreshes[$index].previous_manifest_sha256 -ne $previousHash -or
            [uint64]$refreshes[$index].refresh_index -ne [uint64]$refreshes[$index - 1].refresh_index + 1) {
            throw "refresh chain mismatch at index $index"
        }
    }
    $refresh = $refreshes[-1]
    $refreshPath = $refreshPaths[-1]
    $refreshHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $refreshPath).Hash.ToLowerInvariant()
    if ([uint64]$refresh.global_generation -ne $StartGeneration -or
        [string]$refresh.payoff_panel_sha256 -ne [string]$matrix.payoff_panel.sha256 -or
        [string]$weightUpdate.refresh_manifest_sha256 -ne $refreshHash -or
        [uint64]$weightUpdate.weight_total_units -ne 1000000) {
        throw 'refresh, payoff panel, or weight update binding mismatch'
    }

    $executable = (Resolve-Path -LiteralPath $ExecutablePath).Path
    $executableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $executable).Hash.ToLowerInvariant()
    & git -C $script:RepoRoot merge-base --is-ancestor $ExecutableSourceCommit ([string]$git.commit)
    Assert-LastExitCode $LASTEXITCODE 'executable source commit ancestry'
    $changedRust = @(& git -C $script:RepoRoot diff --name-only ([string]$throughput.executable.source_commit) $ExecutableSourceCommit -- mtg-kernel)
    Assert-LastExitCode $LASTEXITCODE 'training executable source-diff audit'
    if ($changedRust.Count -ne 1 -or $changedRust[0] -ne 'mtg-kernel/src/native_science_loop_v1.rs') {
        throw 'training executable differs from throughput authority outside the reviewed evaluator test file'
    }

    Assert-Gpu1Idle | Out-Null
    Assert-GpuIdentity -Ordinal 0 | Out-Null
    Assert-NoForeignGpu1ComputeProcesses
    $lineages = [ordered]@{}
    foreach ($wave in $prerequisite.waves) {
        foreach ($lane in $wave.lanes) {
            $lineages[[string]$lane.seed] = [ordered]@{
                store_root = [string]$lane.store_root
                store_parent = Split-Path -Parent (Split-Path -Parent ([string]$lane.store_root))
            }
        }
    }
    if ($lineages.Count -ne 3) {
        throw 'prerequisite interval does not bind all three lineages'
    }
    $rootBySeed = [ordered]@{
        '920012' = 'D:\mtg-kernel-ladder-pilot-20260725\pool3\primary'
        '920005' = 'D:\mtg-kernel-ladder-pilot-20260725\pool3\pred-a'
        '970001' = $lineages['970001'].store_root
        '970002' = $lineages['970002'].store_root
        '970003' = $lineages['970003'].store_root
    }
    $slotRoots = @($refresh.slots | ForEach-Object {
        $value = $rootBySeed[[string]$_.source_base_seed]
        if ([string]::IsNullOrWhiteSpace($value)) {
            throw "no Store root for refresh seed $($_.source_base_seed)"
        }
        $value
    }) -join ';'
    $refreshChain = $refreshPaths -join ';'
    $episodeCount = [uint64](($StopGeneration - $StartGeneration) * 64 * 3)
    $plan = [ordered]@{
        schema = 'scaled-selfplay-population-interval-plan/v1'
        created_utc = [DateTimeOffset]::UtcNow.ToString('O')
        git = $git
        executable = [ordered]@{ path = $executable; sha256 = $executableHash; source_commit = $ExecutableSourceCommit }
        training_executable_equivalence = [ordered]@{
            throughput_source_commit = [string]$throughput.executable.source_commit
            current_source_commit = $ExecutableSourceCommit
            changed_rust_files = $changedRust
            review = 'Only the ignored H2H evaluator test changed; population training runtime is unchanged.'
        }
        prerequisite_interval = Get-FileRecordV1 -Path $prerequisitePath
        payoff_matrix = Get-FileRecordV1 -Path $matrixPath
        payoff_panel = $matrix.payoff_panel
        weight_update = Get-FileRecordV1 -Path $weightUpdatePath
        refresh_chain = @($refreshPaths | ForEach-Object { Get-FileRecordV1 -Path $_ })
        throughput = Get-FileRecordV1 -Path $throughputPath
        start_generation = $StartGeneration
        stop_generation = $StopGeneration
        episode_count = $episodeCount
        topology = 'gpu0+gpu1, then gpu1'
        terminal_reward_only = $true
        exploiter_robustness_claim = $false
    }
    $planPath = Join-Path $root 'interval-plan.json'
    Write-JsonFile -Value $plan -Path $planPath

    $campaignClock = [System.Diagnostics.Stopwatch]::StartNew()
    $phase = 'population-wave-0'
    $wave0 = Invoke-PopulationWaveV1 -WaveIndex 0 -Executable $executable -RefreshChain $refreshChain -SlotRoots $slotRoots -Members @(
        [ordered]@{ seed = [uint64]970001; gpu = 0; store_parent = $lineages['970001'].store_parent },
        [ordered]@{ seed = [uint64]970002; gpu = 1; store_parent = $lineages['970002'].store_parent }
    )
    $phase = 'population-wave-1'
    $wave1 = Invoke-PopulationWaveV1 -WaveIndex 1 -Executable $executable -RefreshChain $refreshChain -SlotRoots $slotRoots -Members @(
        [ordered]@{ seed = [uint64]970003; gpu = 1; store_parent = $lineages['970003'].store_parent }
    )
    $campaignClock.Stop()
    Assert-Gpu1Idle | Out-Null
    Assert-NoForeignGpu1ComputeProcesses

    $phase = 'complete'
    $manifest = [ordered]@{
        schema = 'scaled-selfplay-population-interval-execution/v1'
        passed = $true
        disposition = "INTERVAL-$('{0:d4}' -f $StopGeneration)-COMPLETE"
        completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
        plan = Get-FileRecordV1 -Path $planPath
        refresh = Get-FileRecordV1 -Path $refreshPath
        payoff_matrix = $plan.payoff_matrix
        episode_count = $episodeCount
        wall_seconds = $campaignClock.Elapsed.TotalSeconds
        aggregate_episodes_per_second = $episodeCount / $campaignClock.Elapsed.TotalSeconds
        waves = @($wave0, $wave1)
        terminal_outcomes_read = $false
        nonclaim = 'Historical fallback preserves the population-pressure test but makes no exploiter-robustness or playing-strength claim.'
    }
    $manifestPath = Join-Path $root 'interval-execution-manifest.json'
    Write-JsonFile -Value $manifest -Path $manifestPath
    Write-Host "POPULATION INTERVAL PASS evidence=$manifestPath"
}
catch {
    foreach ($lane in $active) { Stop-NativeLane -Lane $lane }
    "$([DateTimeOffset]::UtcNow.ToString('O')) phase=$phase VOID=$($_.Exception.Message)" |
        Set-Content -LiteralPath (Join-Path $root "void-$phase.log") -Encoding utf8
    throw
}
