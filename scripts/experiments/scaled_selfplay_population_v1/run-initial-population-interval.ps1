param(
    [Parameter(Mandatory = $true)][string]$ReplayExecutionManifestPath,
    [Parameter(Mandatory = $true)][string]$RefreshManifestPath,
    [Parameter(Mandatory = $true)][string]$FallbackRecordPath,
    [Parameter(Mandatory = $true)][string]$ThroughputManifestPath,
    [Parameter(Mandatory = $true)][string]$ExecutablePath,
    [Parameter(Mandatory = $true)][string]$ExecutableSourceCommit,
    [string]$EvidenceRoot = 'D:\mtg-kernel-scaled-selfplay-population-v1\active'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
$root = New-UniqueAttemptRoot -EvidenceRoot $EvidenceRoot -GateName 'interval-0512-0640'
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
        $active.Add((Start-ScaledNativeLane -Executable $Executable -Seed ([uint64]$member.seed) -Updates 1536 -StoreParent ([string]$member.store_parent) -GpuOrdinal ([int]$member.gpu) -Mode population -ExpectedResumeGeneration 512 -StopAfterGeneration 640 -RefreshChain $RefreshChain -SlotRoots $SlotRoots -ResumeExistingStore -LogPath (Join-Path $root "$label.log") -EvidenceRoot $root))
    }
    $results = @($active | ForEach-Object { Wait-NativeLane -Lane $_ })
    $active.Clear()
    $clock.Stop()
    return [ordered]@{
        wave_index = $WaveIndex
        wall_seconds = $clock.Elapsed.TotalSeconds
        episode_count = [uint64](8192 * $Members.Count)
        episodes_per_second = (8192.0 * $Members.Count) / $clock.Elapsed.TotalSeconds
        lanes = @($results | ForEach-Object {
            $store = Join-Path $_.store_parent 'run-0\store'
            Assert-GenerationCheckpoint -Store $store -Generation 640
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
    Assert-ExclusiveWindow
    $git = Get-GitRecord -RepoRoot $script:RepoRoot
    $replayPath = (Resolve-Path -LiteralPath $ReplayExecutionManifestPath).Path
    $replay = Get-Content -Raw -LiteralPath $replayPath | ConvertFrom-Json
    if ($replay.passed -ne $true -or [string]$replay.disposition -ne 'REPLAY-BIT-MATCH-ADVANCE') {
        throw 'replay prerequisite did not ADVANCE'
    }
    $handoffPath = [string]$replay.handoff_manifest.path
    $handoff = Get-Content -Raw -LiteralPath $handoffPath | ConvertFrom-Json
    $validation = Get-Content -Raw -LiteralPath ([string]$replay.handoff_validation.path) | ConvertFrom-Json
    if ($validation.continuation_authorized -ne $true -or [string]$validation.disposition -ne 'ADVANCE') {
        throw 'all-three replay handoff is not authorized'
    }
    $refreshPath = (Resolve-Path -LiteralPath $RefreshManifestPath).Path
    $fallbackPath = (Resolve-Path -LiteralPath $FallbackRecordPath).Path
    $refreshHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $refreshPath).Hash.ToLowerInvariant()
    $fallback = Get-Content -Raw -LiteralPath $fallbackPath | ConvertFrom-Json
    if ([string]$fallback.refresh_manifest_sha256 -ne $refreshHash -or
        $fallback.selection_uses_terminal_outcomes -ne $false) {
        throw 'historical-fallback record does not bind the initial refresh'
    }
    $throughputPath = (Resolve-Path -LiteralPath $ThroughputManifestPath).Path
    $throughput = Get-Content -Raw -LiteralPath $throughputPath | ConvertFrom-Json
    if ($throughput.passed -ne $true -or [string]$throughput.topology.selected -ne 'gpu0+gpu1') {
        throw 'dual-GPU topology prerequisite is absent'
    }
    $executable = (Resolve-Path -LiteralPath $ExecutablePath).Path
    $executableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $executable).Hash.ToLowerInvariant()
    if ($executableHash -ne [string]$throughput.executable.sha256 -or
        $ExecutableSourceCommit -ne [string]$throughput.executable.source_commit) {
        throw 'population executable does not match throughput authority'
    }
    & git -C $script:RepoRoot merge-base --is-ancestor $ExecutableSourceCommit ([string]$git.commit)
    Assert-LastExitCode $LASTEXITCODE 'executable source commit ancestry'
    Assert-Gpu1Idle | Out-Null
    Assert-GpuIdentity -Ordinal 0 | Out-Null
    Assert-NoForeignGpu1ComputeProcesses

    $lineages = [ordered]@{}
    foreach ($lineage in $handoff.lineages) {
        $store = [string]$lineage.successor.store_root
        $lineages[[string]$lineage.seed] = [ordered]@{
            store_root = $store
            store_parent = Split-Path -Parent (Split-Path -Parent $store)
        }
    }
    $anchor0 = 'D:\mtg-kernel-ladder-pilot-20260725\pool3\primary'
    $anchor1 = 'D:\mtg-kernel-ladder-pilot-20260725\pool3\pred-a'
    $slotRoots = @(
        $anchor0, $anchor1,
        $lineages['970003'].store_root, $lineages['970003'].store_root,
        $lineages['970001'].store_root, $lineages['970002'].store_root,
        $lineages['970003'].store_root, $lineages['970003'].store_root
    ) -join ';'
    $plan = [ordered]@{
        schema = 'scaled-selfplay-population-initial-interval-plan/v1'
        created_utc = [DateTimeOffset]::UtcNow.ToString('O')
        git = $git
        executable = [ordered]@{ path = $executable; sha256 = $executableHash; source_commit = $ExecutableSourceCommit }
        replay = Get-FileRecordV1 -Path $replayPath
        refresh = Get-FileRecordV1 -Path $refreshPath
        fallback = Get-FileRecordV1 -Path $fallbackPath
        throughput = Get-FileRecordV1 -Path $throughputPath
        start_generation = 512
        stop_generation = 640
        episode_count = 24576
        topology = 'gpu0+gpu1, then gpu1'
        terminal_reward_only = $true
        exploiter_robustness_claim = $false
    }
    $planPath = Join-Path $root 'interval-plan.json'
    Write-JsonFile -Value $plan -Path $planPath

    $campaignClock = [System.Diagnostics.Stopwatch]::StartNew()
    $phase = 'population-wave-0'
    $wave0 = Invoke-PopulationWaveV1 -WaveIndex 0 -Executable $executable -RefreshChain $refreshPath -SlotRoots $slotRoots -Members @(
        [ordered]@{ seed = [uint64]970001; gpu = 0; store_parent = $lineages['970001'].store_parent },
        [ordered]@{ seed = [uint64]970002; gpu = 1; store_parent = $lineages['970002'].store_parent }
    )
    $phase = 'population-wave-1'
    $wave1 = Invoke-PopulationWaveV1 -WaveIndex 1 -Executable $executable -RefreshChain $refreshPath -SlotRoots $slotRoots -Members @(
        [ordered]@{ seed = [uint64]970003; gpu = 1; store_parent = $lineages['970003'].store_parent }
    )
    $campaignClock.Stop()
    Assert-Gpu1Idle | Out-Null
    Assert-NoForeignGpu1ComputeProcesses

    $phase = 'complete'
    $manifest = [ordered]@{
        schema = 'scaled-selfplay-population-initial-interval-execution/v1'
        passed = $true
        disposition = 'INTERVAL-0640-COMPLETE'
        completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
        plan = Get-FileRecordV1 -Path $planPath
        refresh = $plan.refresh
        fallback = $plan.fallback
        episode_count = 24576
        wall_seconds = $campaignClock.Elapsed.TotalSeconds
        aggregate_episodes_per_second = 24576.0 / $campaignClock.Elapsed.TotalSeconds
        waves = @($wave0, $wave1)
        terminal_outcomes_read = $false
        nonclaim = 'Historical fallback preserves the population-pressure test but makes no exploiter-robustness or playing-strength claim.'
    }
    $manifestPath = Join-Path $root 'interval-execution-manifest.json'
    Write-JsonFile -Value $manifest -Path $manifestPath
    Write-Host "INITIAL POPULATION INTERVAL PASS evidence=$manifestPath"
}
catch {
    foreach ($lane in $active) { Stop-NativeLane -Lane $lane }
    "$([DateTimeOffset]::UtcNow.ToString('O')) phase=$phase VOID=$($_.Exception.Message)" |
        Set-Content -LiteralPath (Join-Path $root "void-$phase.log") -Encoding utf8
    throw
}
