param(
    # real-attempt-001 (smoke-scale, 4-update schedule) and real-attempt-002
    # (killed mid-build while chasing the genesis stop_after_generation fix)
    # are both deleted; real-attempt-003 is the genuine campaign genesis
    # (requested_successful_updates=2048, generation_index=0, GEN0
    # model_parameter_sha256 confirmed matching the pinned parent).
    [string]$StoreParent = 'E:\mtg-kernel-population-v2-cycle3\lineage\real-attempt-003',
    [uint64]$Seed = 977002,
    [ValidateSet('Both', 'Item2Only', 'Item3Only')]
    [string]$Phase = 'Both'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

Assert-Cycle3SheetIdentity | Out-Null
Assert-SearcherPoolAuthoritySheetIdentity | Out-Null

$manifestDir = 'E:\mtg-kernel-population-v2-cycle3\refresh-manifests'

# Coordinator-located real artifacts (2026-08-26): tranche-1's own genesis
# chain (indices 0-2) and cycle-2's real current-0 store both exist on this
# host; the earlier draft of this script used a two-file [18,19] chain and
# was expected to (and did) fail at InvalidChain. Superseded here by the
# complete real chain and real slot roots.
$slotRoots = @(
    'D:\mtg-kernel-ladder-pilot-20260725\pool3\primary'
    'D:\mtg-kernel-scaled-selfplay-population-v1\replay\three-lineage-replay\attempt-001\wave-00-seed-970002-gpu1\run-0\store'
    'D:\mtg-kernel-denovo-campaign-v1\seed-971221\denovo-1024-screen-build\attempt-001\denovo-1024-store\run-0\store'
    'D:\mtg-kernel-denovo-campaign-v1\seed-971223\denovo-1024-screen-build\attempt-002\denovo-1024-store\run-0\store'
    # current-0 (slot 4, frozen at cycle-2's terminal state): the working
    # D:\ copy, verified byte-exact against the manifest's own declared
    # run_sha256/checkpoint_manifest_sha256/checkpoint_payload_sha256/
    # model_parameter_sha256 (all four independently rehashed and matched
    # before this script was written). Simpler than the E:\ archive
    # original, which needs a \\?\-prefixed literal path for its own
    # length; both are real and equally valid per the resolver's own fixed
    # semantics (SlotRoots is physical location only, never compared to
    # the manifest's frozen store_root string).
    'D:\throughput-remeasure-20260825\v2-resume-walk\store-depth2048-cycle2'
    # slot 5 (current-1): FIX (caught 2026-08-26 on the real refresh-19
    # launch attempt, RunInvalid at the opponent resolver): the opponent
    # pool resolved for a given launch is ALWAYS the manifest chain's
    # "active" (resume-parent) link's OWN 8-slot snapshot -- for refresh-19
    # that is index 18, the real cycle-2 archive's terminal manifest, whose
    # slot 5 declares CYCLE-2's OWN current-1 (seed 975002, generation
    # 2048), not cycle-3's own trainee. cycle-3's own growing trainee is
    # $StoreParent, resumed/trained directly via Set-Cycle3NativeEnvironment
    # below -- an entirely separate mechanism from this opponent-pool slot
    # array. The real cycle-2 seed-975002 store already sits at the
    # coordinator-set-up parent-import location (common.ps1's own
    # $script:Cycle3ParentStoreRoot; independently rehashed here: this
    # store's run.json sha256 = 8d9a8287ef57651d5744d26275d2a8c0dc74cfb69cb7e1b2dd22691b5bd8a504,
    # matching manifest index 18's own slot-5 run_sha256 exactly). Later
    # launches (active = index 19+, cycle-3's own manifests) instead need
    # slot 5 to point at cycle-3's own resulting store from the PRIOR
    # refresh (i.e. $StoreParent itself, by then advanced to that refresh's
    # real local generation) once that refresh's manifest is re-sealed with
    # real hashes -- computed per-call in Invoke-Cycle3Refresh below, not
    # fixed here.
    '__SLOT5_PLACEHOLDER__'
    'D:\mtg-kernel-denovo-campaign-v1\seed-971222\denovo-1024-screen-build\attempt-001\denovo-1024-store\run-0\store'
    'D:\mtg-kernel-denovo-campaign-v1\seed-971221\denovo-1024-screen-build\attempt-001\denovo-1024-store\run-0\store'
)

$evidenceRoot = Join-Path $PSScriptRoot '..\..\..\..\cycle3-refresh1-smoke-evidence'
$evidenceRoot = (New-Item -ItemType Directory -Force -Path $evidenceRoot).FullName
$exe = Get-ReleaseTestExecutableCycle3V1 -EvidenceRoot $evidenceRoot -Label 'refresh1-smoke'
Write-Output "RESOLVED_EXE=$exe"

function Invoke-Cycle3Refresh {
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

    # slot 5 (current-1) is the ONE opponent-pool root that varies by which
    # link is "active" (chain.last(), i.e. index $ChainThroughIndex) for
    # this specific launch: index 18 (the real cycle-2 terminal) declares
    # cycle-2's OWN current-1 (seed 975002, the parent-import store);
    # index 19+ (cycle-3's own re-sealed manifests) declare cycle-3's own
    # growing trainee, which IS $StoreParent by the time that launch runs.
    $slot5Root = if ($ChainThroughIndex -eq 18) { $script:Cycle3ParentStoreRoot } else { (Join-Path $StoreParent 'run-0\store') }
    $callSlotRoots = $slotRoots | ForEach-Object { if ($_ -eq '__SLOT5_PLACEHOLDER__') { $slot5Root } else { $_ } }
    $slotRootsJoined = [string]::Join(';', $callSlotRoots)

    # MULTIRUN_UPDATES is the WHOLE lineage's own schedule target
    # (record.schedule.requested_successful_updates), not a per-refresh
    # delta -- confirmed against the real cycle-2 training log
    # ("MULTIRUN CONFIG ... updates=2048 ... stop_after_generation=128
    # expected_resume_generation=0" for cycle-2's OWN first real interval):
    # 2,048 is cycle-3's own pinned local_updates_total
    # (population_program_v2_cycle3_contract_for_launch_v1's own field),
    # unchanged across every one of the 16 refreshes; only
    # StopAfterGeneration/ExpectedResumeGeneration (both LOCAL) move.
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
    Write-Output "$Label EXIT_CODE=$exitCode"
    if ($exitCode -ne 0) {
        Write-Output "--- $Label log tail ---"
        Get-Content -LiteralPath $logPath -Tail 80
    }
    return $exitCode
}

# FIX (caught 2026-08-26 on the first real attempt, native_science_loop_v1.rs:1761
# assertion left==right failed: left 2432, right 2304): the manifest chain
# supplied to a launch must end at that refresh's PARENT link, i.e. the
# already-sealed state the store is RESUMING FROM (active.global_generation_v2()
# is asserted == expected_start_global, the RESUME point, not the refresh's own
# post-training target). Refresh-19 (local 0->128) resumes from index 18
# (cycle-2's real terminal, global 2304) -- it must NOT include its own
# not-yet-trained index-19 manifest. Only AFTER refresh-19 completes for real
# is index 19 re-sealed with its REAL resulting slot-5 hashes and used as the
# resume-parent for refresh-20's own launch.

if ($Phase -ne 'Item3Only') {
    # Task 7 item 2: the real refresh-19 run, resuming the authored genesis
    # (local generation 0) to local generation 128, against the real
    # already-sealed chain (indices 0-18, ending at cycle-2's real terminal).
    $exit19 = Invoke-Cycle3Refresh -ChainThroughIndex 18 -ExpectedResumeLocal 0 -StopAfterLocal 128 -Label 'refresh-19'
    if ($exit19 -ne 0) { throw "refresh-19 (Task 7 item 2) failed with exit code $exit19" }
    Assert-WarmStartGenZeroCycle3V1 -StoreParent $StoreParent | Out-Null
    Write-Output 'TASK7_ITEM2_REFRESH19_PASSED'
}

if ($Phase -eq 'Item2Only') {
    Write-Output 'PHASE_ITEM2ONLY_DONE_AWAITING_MANIFEST_019_RESEAL'
    return
}

if ($Phase -eq 'Item3Only') {
    Write-Output 'PHASE_ITEM3ONLY_ASSUMES_MANIFEST_019_ALREADY_REAL_SEALED'
}

# Task 7 item 3: the index-20-shaped searcher smoke, continuing local
# generation 128 to 256, against the real chain (indices 0-19, ending at
# the now-really-sealed index 19), landing on index 20 -- cycle-3's own
# refresh 2, a scheduled searcher-heavy window.
$exit20 = Invoke-Cycle3Refresh -ChainThroughIndex 19 -ExpectedResumeLocal 128 -StopAfterLocal 256 -Label 'refresh-20-searcher-smoke'
if ($exit20 -ne 0) { throw "refresh-20 searcher smoke (Task 7 item 3) failed with exit code $exit20" }
Assert-WarmStartGenZeroCycle3V1 -StoreParent $StoreParent | Out-Null
Write-Output 'TASK7_ITEM3_REFRESH20_SEARCHER_SMOKE_PASSED'
