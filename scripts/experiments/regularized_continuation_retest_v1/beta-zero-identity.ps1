param(
    [string]$EvidenceRoot = 'D:\mtg-kernel-regularized-continuation-retest-v1\preflight\seed-969999'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
$baselineRepoRoot = (Resolve-Path -LiteralPath $script:BaselineRepoRoot).Path
$root = New-UniqueAttemptRoot -EvidenceRoot $EvidenceRoot -GateName 'beta-zero-identity'

Assert-ExclusiveWindow
$candidateGit = Get-GitRecord -RepoRoot $script:RepoRoot
$baselineGit = Get-GitRecord -RepoRoot $baselineRepoRoot -RequireExactBase
$toolchain = Get-ToolchainRecord
$cuda = Get-CudaRecord
$inputs = Get-InputRecord
$candidateExecutable = Get-ReleaseTestExecutable -RepoRoot $script:RepoRoot -EvidenceRoot $root -Label 'candidate'
$baselineExecutable = Get-ReleaseTestExecutable -RepoRoot $baselineRepoRoot -EvidenceRoot $root -Label 'baseline'
$candidateExecutableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $candidateExecutable).Hash.ToLowerInvariant()
$baselineExecutableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $baselineExecutable).Hash.ToLowerInvariant()
$gpu0 = Assert-GpuIdentity -Ordinal 0
$gpuBefore = Assert-Gpu1Idle
$prelaunchResources = Assert-PrelaunchResourceWindow
Assert-NoForeignGpu1ComputeProcesses

$baselineParent = Join-Path $root 'baseline-original-uninterrupted'
$candidateParent = Join-Path $root 'candidate-close-reopen'
$baselineClock = [Diagnostics.Stopwatch]::StartNew()
Invoke-NativePilot -Executable $baselineExecutable -Seed 969999 -Updates 64 -StoreParent $baselineParent -GpuOrdinal 1 -LogPath (Join-Path $root 'baseline-original-uninterrupted.log')
$baselineClock.Stop()
Assert-Gpu1Idle | Out-Null

$phase1Clock = [Diagnostics.Stopwatch]::StartNew()
Invoke-NativePilot -Executable $candidateExecutable -Seed 969999 -Updates 64 -StoreParent $candidateParent -GpuOrdinal 1 -LogPath (Join-Path $root 'candidate-phase1-stop-32.log') -StopAfterGeneration 32 -RequirePolicyAnchorMarker
$phase1Clock.Stop()
$candidateStore = Join-Path $candidateParent 'run-0\store'
Assert-GenerationCheckpoint -Store $candidateStore -Generation 32
Assert-Gpu1Idle | Out-Null

$phase2Clock = [Diagnostics.Stopwatch]::StartNew()
Invoke-NativePilot -Executable $candidateExecutable -Seed 969999 -Updates 64 -StoreParent $candidateParent -GpuOrdinal 1 -LogPath (Join-Path $root 'candidate-phase2-resume-32-to-64.log') -ExpectedResumeGeneration 32 -ResumeExistingStore -RequirePolicyAnchorMarker
$phase2Clock.Stop()
$gpuAfter = Assert-Gpu1Idle

$baselineStore = Join-Path $baselineParent 'run-0\store'
Assert-GenerationCheckpoint -Store $baselineStore -Generation 64
Assert-GenerationCheckpoint -Store $candidateStore -Generation 64
$baselineHash = Get-StoreTreeHash -Path $baselineStore
$candidateHash = Get-StoreTreeHash -Path $candidateStore
$resumeLogText = Get-Content -LiteralPath (Join-Path $root 'candidate-phase2-resume-32-to-64.log') -Raw
$closeReopenObserved = $resumeLogText -match 'STORE CLOSE_REOPEN resume_generation=32'
$bitIdentical = $baselineHash -eq $candidateHash
$passed = $bitIdentical -and $closeReopenObserved

$manifest = [ordered]@{
    schema = 'regularized-continuation-beta-zero-identity-preflight/v2'
    passed = $passed
    disposition_on_failure = 'FAIL-INVESTIGATE; no later gate may run'
    close_reopen_required = $true
    close_reopen_marker = 'STORE CLOSE_REOPEN resume_generation=32'
    close_reopen_observed = $closeReopenObserved
    comparison = 'exact original macro executable uninterrupted 0-to-64 versus candidate beta-zero executable in two OS processes, 0-to-32 then close/reopen 32-to-64'
    beta = '0'
    seed = 969999
    target_updates = 64
    phase_boundary = 32
    episodes_total_per_comparison_arm = 4096
    checkpoint_boundary = 64
    topology = [ordered]@{ workers = 2; sessions = 32; broker_target = 16; gpu_ordinal = 1 }
    git = [ordered]@{ baseline = $baselineGit; candidate = $candidateGit }
    toolchain = $toolchain
    cuda = $cuda
    executables = [ordered]@{
        baseline = [ordered]@{ path = $baselineExecutable; sha256 = $baselineExecutableHash }
        candidate = [ordered]@{ path = $candidateExecutable; sha256 = $candidateExecutableHash }
    }
    inputs = $inputs
    gpu = [ordered]@{ gpu0_identity = $gpu0; prelaunch_gpu1 = $gpuBefore; postrun_gpu1 = $gpuAfter }
    prelaunch_resources = $prelaunchResources
    timings = [ordered]@{
        baseline_uninterrupted_seconds = $baselineClock.Elapsed.TotalSeconds
        candidate_phase1_seconds = $phase1Clock.Elapsed.TotalSeconds
        candidate_phase2_seconds = $phase2Clock.Elapsed.TotalSeconds
        candidate_total_seconds = $phase1Clock.Elapsed.TotalSeconds + $phase2Clock.Elapsed.TotalSeconds
    }
    outputs = [ordered]@{
        baseline = [ordered]@{
            store = $baselineStore
            store_tree_sha256 = $baselineHash
            policy_anchor_authority = [ordered]@{
                path = Join-Path $baselineParent $script:PolicyAnchorAuthorityFileName
                sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $baselineParent $script:PolicyAnchorAuthorityFileName)).Hash.ToLowerInvariant()
            }
            files = Get-StoreFileInventory -Path $baselineStore
        }
        candidate = [ordered]@{
            store = $candidateStore
            store_tree_sha256 = $candidateHash
            policy_anchor_authority = [ordered]@{
                path = Join-Path $candidateParent $script:PolicyAnchorAuthorityFileName
                sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $candidateParent $script:PolicyAnchorAuthorityFileName)).Hash.ToLowerInvariant()
            }
            files = Get-StoreFileInventory -Path $candidateStore
        }
    }
    output_hashes_bit_identical = $bitIdentical
}
Write-JsonFile -Value $manifest -Path (Join-Path $root 'identity-manifest.json')
if (-not $passed) {
    if (-not $closeReopenObserved) {
        throw 'beta-zero identity failed: an actual close/reopen marker was not observed'
    }
    throw 'beta-zero identity failed: candidate close/reopen Store differs from the original executable Store'
}
Write-Host "BETA-ZERO IDENTITY PASS store_tree_sha256=$candidateHash evidence=$root"
