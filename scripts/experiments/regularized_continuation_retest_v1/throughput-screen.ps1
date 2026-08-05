param(
    [string]$EvidenceRoot = 'D:\mtg-kernel-regularized-continuation-retest-v1\preflight\seed-969999'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
$root = New-UniqueAttemptRoot -EvidenceRoot $EvidenceRoot -GateName 'throughput-screen'

Assert-ExclusiveWindow
$git = Get-GitRecord -RepoRoot $script:RepoRoot
$identityPrerequisite = Get-PassedIdentityPrerequisite -EvidenceRoot $EvidenceRoot -CandidateCommit $git.commit
$toolchain = Get-ToolchainRecord
$cuda = Get-CudaRecord
$inputs = Get-InputRecord
$executable = Get-ReleaseTestExecutable -RepoRoot $script:RepoRoot -EvidenceRoot $root -Label 'candidate'
$executableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $executable).Hash.ToLowerInvariant()
$gpu1 = Assert-Gpu1Idle
$gpu0 = Assert-GpuIdentity -Ordinal 0
$prelaunchResources = Assert-PrelaunchResourceWindow
Assert-NoForeignGpu1ComputeProcesses

$singleLane = $null
$dualLanes = @()
try {
    $singleParent = Join-Path $root 'single-gpu1'
    $singleLane = Start-NativeLane -Executable $executable -Seed 969999 -Updates 8 -StoreParent $singleParent -GpuOrdinal 1 -LogPath (Join-Path $root 'single-gpu1.log') -EvidenceRoot $root
    $single = Wait-NativeLane -Lane $singleLane
    Assert-Gpu1Idle | Out-Null

    $dualClock = [System.Diagnostics.Stopwatch]::StartNew()
    $gpu0Lane = Start-NativeLane -Executable $executable -Seed 969999 -Updates 8 -StoreParent (Join-Path $root 'dual-gpu0') -GpuOrdinal 0 -LogPath (Join-Path $root 'dual-gpu0.log') -EvidenceRoot $root
    $gpu1Lane = Start-NativeLane -Executable $executable -Seed 969999 -Updates 8 -StoreParent (Join-Path $root 'dual-gpu1') -GpuOrdinal 1 -LogPath (Join-Path $root 'dual-gpu1.log') -EvidenceRoot $root
    $dualLanes = @($gpu0Lane, $gpu1Lane)
    $dualResults = @($dualLanes | ForEach-Object { Wait-NativeLane -Lane $_ })
    $dualClock.Stop()
    $gpu1After = Assert-Gpu1Idle
    Assert-NoForeignGpu1ComputeProcesses
}
catch {
    Stop-NativeLane -Lane $singleLane
    foreach ($lane in $dualLanes) { Stop-NativeLane -Lane $lane }
    throw
}

$singleStore = Join-Path $single.store_parent 'run-0\store'
$singleLatest = Get-Content -LiteralPath (Join-Path $singleStore 'latest.json') -Raw | ConvertFrom-Json
if ([uint64]$singleLatest.generation_index -ne 8) {
    throw "single-lane throughput Store did not reach generation 8: $($singleLatest.generation_index)"
}
$dualStores = foreach ($result in $dualResults) {
    $store = Join-Path $result.store_parent 'run-0\store'
    $latest = Get-Content -LiteralPath (Join-Path $store 'latest.json') -Raw | ConvertFrom-Json
    if ([uint64]$latest.generation_index -ne 8) {
        throw "throughput Store did not reach generation 8 on GPU $($result.gpu_ordinal)"
    }
    [ordered]@{
        gpu_ordinal = $result.gpu_ordinal
        store = $store
        store_tree_sha256 = Get-StoreTreeHash -Path $store
        policy_anchor_authority = [ordered]@{
            path = Join-Path $result.store_parent $script:PolicyAnchorAuthorityFileName
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $result.store_parent $script:PolicyAnchorAuthorityFileName)).Hash.ToLowerInvariant()
        }
        lane = $result
    }
}

$singleHash = Get-StoreTreeHash -Path $singleStore
$gpu0Store = $dualStores | Where-Object { $_.gpu_ordinal -eq 0 } | Select-Object -First 1
$gpu1Store = $dualStores | Where-Object { $_.gpu_ordinal -eq 1 } | Select-Object -First 1
$crossDeviceIdentical = $gpu0Store.store_tree_sha256 -eq $gpu1Store.store_tree_sha256
$sameDeviceIdentical = $singleHash -eq $gpu1Store.store_tree_sha256
$singleRate = 512.0 / [double]$single.wall_seconds
$dualRate = 1024.0 / [double]$dualClock.Elapsed.TotalSeconds
$speedup = $dualRate / $singleRate
$dualSamples = @($dualStores | ForEach-Object { $_.lane.resource_samples })
$resourceLimits = [ordered]@{ max_gpu_memory_fraction = 0.95; max_host_memory_fraction = 0.90 }
$resourceSafe = $dualSamples.Count -gt 0 -and @($dualSamples | ForEach-Object {
    $sample = $_
    $hasBothGpus = @($sample.gpus | Where-Object { $_.ordinal -eq 0 }).Count -eq 1 -and @($sample.gpus | Where-Object { $_.ordinal -eq 1 }).Count -eq 1
    $gpuMemorySafe = @($sample.gpus | Where-Object { $_.memory_total_mib -le 0 -or ($_.memory_used_mib / [double]$_.memory_total_mib) -gt $resourceLimits.max_gpu_memory_fraction }).Count -eq 0
    $hostMemorySafe = $sample.host_memory_total_mib -gt 0 -and ($sample.host_memory_used_mib / [double]$sample.host_memory_total_mib) -le $resourceLimits.max_host_memory_fraction
    if ($hasBothGpus -and $gpuMemorySafe -and $hostMemorySafe) { 1 } else { 0 }
} | Where-Object { $_ -eq 0 }).Count -eq 0
$selected = if ($resourceSafe -and $sameDeviceIdentical -and $crossDeviceIdentical -and $speedup -ge 1.5) { 'gpu0+gpu1' } else { 'gpu1-only' }

$manifest = [ordered]@{
    schema = 'regularized-continuation-throughput-screen/v2'
    passed = $sameDeviceIdentical
    disposition_on_same_device_nonidentity = 'FAIL-INVESTIGATE; no later gate may run'
    beta = '0'
    seed = 969999
    updates = 8
    episodes_per_run = 512
    topology_rule = 'select gpu0+gpu1 only when resource-safe, cross-device bit-identical, and aggregate speedup >= 1.5; otherwise gpu1-only'
    selected_topology = $selected
    git = $git
    prerequisite_identity = $identityPrerequisite
    toolchain = $toolchain
    cuda = $cuda
    executable = [ordered]@{ path = $executable; sha256 = $executableHash }
    inputs = $inputs
    prelaunch_gpus = @($gpu0, $gpu1)
    prelaunch_resources = $prelaunchResources
    postrun_gpu1 = $gpu1After
    resource_limits = $resourceLimits
    points = @(
        [ordered]@{
            topology = 'gpu1-only'
            gpu_ordinals = @(1)
            wall_seconds = $single.wall_seconds
            episodes_per_second = $singleRate
            store_tree_sha256 = $singleHash
            stores = @([ordered]@{
                gpu_ordinal = 1
                store = $singleStore
                store_tree_sha256 = $singleHash
                policy_anchor_authority = [ordered]@{
                    path = Join-Path $single.store_parent $script:PolicyAnchorAuthorityFileName
                    sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $single.store_parent $script:PolicyAnchorAuthorityFileName)).Hash.ToLowerInvariant()
                }
            })
            resource_samples = $single.resource_samples
        },
        [ordered]@{
            topology = 'gpu0+gpu1'
            gpu_ordinals = @(0, 1)
            wall_seconds = $dualClock.Elapsed.TotalSeconds
            episodes_per_second_aggregate = $dualRate
            stores = $dualStores
            cross_device_same_seed_bit_identical = $crossDeviceIdentical
            resource_safe = $resourceSafe
            resource_samples = $dualStores | ForEach-Object { $_.lane.resource_samples }
        }
    )
    aggregate_speedup = $speedup
    same_device_single_hash = $singleHash
    same_device_repeat_bit_identical = $sameDeviceIdentical
    cross_device_hashes = [ordered]@{ gpu0 = $gpu0Store.store_tree_sha256; gpu1 = $gpu1Store.store_tree_sha256 }
    cross_device_same_seed_bit_identical = $crossDeviceIdentical
    gpu0_desktop_load_sampled = $true
}
Write-JsonFile -Value $manifest -Path (Join-Path $root 'throughput-manifest.json')
if (-not $sameDeviceIdentical) {
    throw 'throughput screen failed: repeat execution on GPU 1 was not bit-identical'
}
Write-Host "THROUGHPUT SCREEN PASS selected=$selected speedup=$([math]::Round($speedup, 3)) evidence=$root"
