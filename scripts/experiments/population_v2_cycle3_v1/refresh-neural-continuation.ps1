param(
    [string]$StoreParent = 'E:\mtg-kernel-population-v2-cycle3\lineage\real-attempt-003',
    [uint64]$Seed = 977002,
    [Parameter(Mandatory = $true)][int]$ChainThroughIndex,
    [Parameter(Mandatory = $true)][uint64]$ExpectedResumeLocal,
    [Parameter(Mandatory = $true)][uint64]$StopAfterLocal,
    [Parameter(Mandatory = $true)][string]$Label
)

# Sibling of refresh1-searcher-smoke.ps1 (which owns refreshes 19-21, the
# warm-start-binding, chain-length, and searcher-schedule-timeout fixes),
# for the fresh-interval-start, all-neural refreshes that follow (22, 23,
# 24, ...): a single parameterized launch of exactly one refresh, no
# multi-phase logic, so each can be started as its own independent
# detached process (per Task 1's recipe) without re-running any earlier
# refresh. Setup (sheet identities, exe resolution, slot roots,
# Invoke-Cycle3Refresh) is copied verbatim from refresh1-searcher-smoke.ps1
# rather than shared via a new common.ps1 function, to keep this addition
# purely additive against the already-countersigned launch stack -- no
# existing file's behavior changes.

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

Assert-Cycle3SheetIdentity | Out-Null
Assert-SearcherPoolAuthoritySheetIdentity | Out-Null

$manifestDir = 'E:\mtg-kernel-population-v2-cycle3\refresh-manifests'

# Same real slot-root array as refresh1-searcher-smoke.ps1 (see that file's
# own comments for provenance of each root); slot 5 (current-1) again
# resolved per-call below, since $ChainThroughIndex is never 18 for these
# refreshes (cycle-3's own trainee, not cycle-2's terminal current-1).
$slotRoots = @(
    'D:\mtg-kernel-ladder-pilot-20260725\pool3\primary'
    'D:\mtg-kernel-scaled-selfplay-population-v1\replay\three-lineage-replay\attempt-001\wave-00-seed-970002-gpu1\run-0\store'
    'D:\mtg-kernel-denovo-campaign-v1\seed-971221\denovo-1024-screen-build\attempt-001\denovo-1024-store\run-0\store'
    'D:\mtg-kernel-denovo-campaign-v1\seed-971223\denovo-1024-screen-build\attempt-002\denovo-1024-store\run-0\store'
    'D:\throughput-remeasure-20260825\v2-resume-walk\store-depth2048-cycle2'
    '__SLOT5_PLACEHOLDER__'
    'D:\mtg-kernel-denovo-campaign-v1\seed-971222\denovo-1024-screen-build\attempt-001\denovo-1024-store\run-0\store'
    'D:\mtg-kernel-denovo-campaign-v1\seed-971221\denovo-1024-screen-build\attempt-001\denovo-1024-store\run-0\store'
)

$evidenceRoot = Join-Path $PSScriptRoot '..\..\..\..\cycle3-refresh1-smoke-evidence'
$evidenceRoot = (New-Item -ItemType Directory -Force -Path $evidenceRoot).FullName
$exe = Get-ReleaseTestExecutableCycle3V1 -EvidenceRoot $evidenceRoot -Label "refresh-neural-$Label"
Write-Output "RESOLVED_EXE=$exe"

function Invoke-Cycle3RefreshNeural {
    param(
        [Parameter(Mandatory = $true)][int]$ChainThroughIndex,
        [Parameter(Mandatory = $true)][uint64]$ExpectedResumeLocal,
        [Parameter(Mandatory = $true)][uint64]$StopAfterLocal,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $chainPaths = 0..$ChainThroughIndex | ForEach-Object { Join-Path $manifestDir ("population-v3-refresh-{0:D3}.json" -f $_) }
    foreach ($p in $chainPaths) {
        if (-not (Test-Path -LiteralPath $p)) { throw "missing chain manifest: $p" }
    }
    $refreshChain = [string]::Join(';', $chainPaths)

    $slot5Root = if ($ChainThroughIndex -eq 18) { $script:Cycle3ParentStoreRoot } else { (Join-Path $StoreParent 'run-0\store') }
    $callSlotRoots = $slotRoots | ForEach-Object { if ($_ -eq '__SLOT5_PLACEHOLDER__') { $slot5Root } else { $_ } }
    $slotRootsJoined = [string]::Join(';', $callSlotRoots)

    # Amendment 7 A7.2 item 4: Layer 2 cross-check, before dispatch.
    Assert-ResumePositionMatchesStoreCycle3V1 -StoreParent $StoreParent -ExpectedResumeGeneration $ExpectedResumeLocal | Out-Null

    $saved = Set-Cycle3NativeEnvironment -Seed $Seed -Updates 2048 `
        -StoreParent $StoreParent -GpuOrdinal 0 -ResumeExistingStore `
        -StopAfterGeneration $StopAfterLocal -ExpectedResumeGeneration $ExpectedResumeLocal `
        -PopulationRuntime -RefreshChain $refreshChain -SlotRoots $slotRootsJoined

    $logPath = Join-Path $evidenceRoot "$Label.log"
    try {
        $previous = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try {
            & $exe $script:RunnerTest --ignored --exact --nocapture --test-threads=1 2>&1 |
                Tee-Object -FilePath $logPath | Out-Null
            $exitCode = $LASTEXITCODE
        }
        finally { $ErrorActionPreference = $previous }
    }
    finally {
        Restore-Cycle3NativeEnvironment -Saved $saved
    }
    Write-Host "$Label EXIT_CODE=$exitCode"
    if ($exitCode -ne 0) {
        Write-Host "--- $Label log tail ---"
        Get-Content -LiteralPath $logPath -Tail 80 | ForEach-Object { Write-Host $_ }
    }
    return $exitCode
}

$exitCode = Invoke-Cycle3RefreshNeural -ChainThroughIndex $ChainThroughIndex -ExpectedResumeLocal $ExpectedResumeLocal -StopAfterLocal $StopAfterLocal -Label $Label
if ($exitCode -ne 0) { throw "$Label failed with exit code $exitCode" }
Assert-WarmStartGenZeroCycle3V1 -StoreParent $StoreParent | Out-Null
Write-Output "$Label PASSED"
