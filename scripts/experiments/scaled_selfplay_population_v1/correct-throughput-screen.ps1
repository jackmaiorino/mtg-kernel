param(
    [Parameter(Mandatory = $true)][string]$SourceManifestPath,
    [Parameter(Mandatory = $true)][string]$ExecutablePath,
    [Parameter(Mandatory = $true)][string]$ExecutableSourceCommit,
    [string]$EvidenceRoot = 'D:\mtg-kernel-scaled-selfplay-population-v1\preflight'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
$root = New-UniqueAttemptRoot -EvidenceRoot $EvidenceRoot -GateName 'corrected-throughput-screen'
$phase = 'preflight'
$lane = $null
try {
    Assert-ExclusiveWindow
    $git = Get-GitRecord -RepoRoot $script:RepoRoot
    $sourcePath = (Resolve-Path -LiteralPath $SourceManifestPath).Path
    $source = Get-Content -Raw -LiteralPath $sourcePath | ConvertFrom-Json
    if ($source.passed -ne $true -or
        $source.identity.generation4_native_state_identical -ne $true -or
        $source.identity.generation8_native_state_identical -ne $true -or
        $source.topology.same_device_repeat_bit_identical -ne $true -or
        $source.topology.cross_device_same_seed_bit_identical -ne $true) {
        throw 'source identity screen is not a complete PASS'
    }
    $executable = (Resolve-Path -LiteralPath $ExecutablePath).Path
    & git -C $script:RepoRoot merge-base --is-ancestor $ExecutableSourceCommit ([string]$git.commit)
    Assert-LastExitCode $LASTEXITCODE 'executable source commit ancestry'
    Assert-Gpu1Idle | Out-Null
    Assert-NoForeignGpu1ComputeProcesses

    $phase = 'direct-single-gpu1'
    $parent = Join-Path $root 'direct-single-gpu1'
    $lane = Start-ScaledNativeLane -Executable $executable -Seed 970001 -Updates 1536 -StoreParent $parent -GpuOrdinal 1 -Mode successor -StopAfterGeneration 8 -LogPath (Join-Path $root 'direct-single-gpu1.log') -EvidenceRoot $root
    $single = Wait-NativeLane -Lane $lane
    $lane = $null
    $endpoint = Get-ScaledEndpointRecord -StoreRoot (Join-Path $parent 'run-0\store') -Generation 8
    $dual = @($source.topology.dual)
    $dualGpu0 = $dual | Where-Object { [int]$_.gpu_ordinal -eq 0 } | Select-Object -First 1
    $dualGpu1 = $dual | Where-Object { [int]$_.gpu_ordinal -eq 1 } | Select-Object -First 1
    $sameDevice = $endpoint.tree_sha256 -eq $dualGpu1.endpoint.tree_sha256
    $crossDevice = $dualGpu0.endpoint.tree_sha256 -eq $dualGpu1.endpoint.tree_sha256
    if (-not ($sameDevice -and $crossDevice)) {
        throw 'direct successor Store identity failed'
    }
    $singleRate = 512.0 / [double]$single.wall_seconds
    $dualRate = [double]$source.topology.dual_episodes_per_second
    $speedup = $dualRate / $singleRate
    $resourceSafe = $source.topology.resource_safe -eq $true
    $selected = if ($resourceSafe -and $speedup -ge 1.5) { 'gpu0+gpu1' } else { 'gpu1-only' }

    $phase = 'complete'
    $manifest = [ordered]@{
        schema = 'scaled-selfplay-population-corrected-throughput-screen/v1'
        passed = $true
        completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
        git = $git
        executable = [ordered]@{
            path = $executable
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $executable).Hash.ToLowerInvariant()
            source_commit = $ExecutableSourceCommit
        }
        source_identity_screen = [ordered]@{
            path = $sourcePath
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $sourcePath).Hash.ToLowerInvariant()
        }
        identity = $source.identity
        topology = [ordered]@{
            rule = 'select gpu0+gpu1 only when resource-safe, same-seed Stores bit-identical, and aggregate speedup >= 1.5; otherwise gpu1-only'
            selected = $selected
            single_gpu1 = [ordered]@{
                wall_seconds = $single.wall_seconds
                episodes_per_second = $singleRate
                endpoint = $endpoint
                lane = $single
            }
            dual_wall_seconds = [double]$source.topology.dual_wall_seconds
            dual_episodes_per_second = $dualRate
            aggregate_speedup = $speedup
            resource_safe = $resourceSafe
            same_device_repeat_bit_identical = $sameDevice
            cross_device_same_seed_bit_identical = $crossDevice
            dual_resource_summary = $source.topology.dual_resource_summary
            dual = $dual
        }
        replay_planning = [ordered]@{
            exact_prior_manifest = 'D:\mtg-kernel-regularized-continuation-retest-v1\development\full-horizon-training\attempt-003\training-manifest.json'
            exact_prior_manifest_sha256 = '0a430d62ec6a20d8f752bbcc4d71e15bf8e3a4a339917a07e7afd97d4ff7ef04'
            episode_count = 98304
            expected_wall_seconds = 10836.1347
            expected_wall_hours = 3.0100374167
        }
        terminal_outcomes_read = $false
        nonclaim = 'This screen proves mechanical identity and throughput only, not playing strength.'
    }
    $path = Join-Path $root 'corrected-screen-manifest.json'
    Write-JsonFile -Value $manifest -Path $path
    Write-Host "CORRECTED THROUGHPUT PASS selected=$selected speedup=$([math]::Round($speedup, 3)) evidence=$path"
}
catch {
    Stop-NativeLane -Lane $lane
    "$([DateTimeOffset]::UtcNow.ToString('O')) phase=$phase VOID=$($_.Exception.Message)" |
        Set-Content -LiteralPath (Join-Path $root "void-$phase.log") -Encoding utf8
    throw
}
