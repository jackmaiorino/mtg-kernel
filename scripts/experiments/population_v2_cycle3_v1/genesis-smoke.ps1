param(
    [string]$StoreParent = 'E:\mtg-kernel-population-v2-cycle3\lineage\smoke-attempt-001',
    [uint64]$Seed = 977002,
    [uint64]$Updates = 4,
    # FIX (caught 2026-08-26 building the REAL genesis): with no
    # StopAfterGeneration, the underlying loop trains all the way to
    # $Updates before returning -- fine for the smoke-test default (4,
    # fast), but calling this with the real whole-lineage schedule
    # (-Updates 2048, so the run's own persisted schedule matches every
    # later refresh launch) would try to run the ENTIRE 2048-update
    # campaign in this one blocking call (confirmed empirically: still
    # running past 45 minutes, had to be killed). Genesis only needs to
    # PUBLISH generation 0 (the warm-started import) and stop -- the
    # per-refresh launches (refresh1-searcher-smoke.ps1) each separately
    # advance local generation by exactly 128 per call via their own
    # StopAfterGeneration. Nullable so the smoke-test default behavior
    # (no early stop, train the full small -Updates count) is unchanged
    # unless a caller opts in.
    [Nullable[uint64]]$StopAfterGeneration
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

Assert-Cycle3SheetIdentity | Out-Null
Assert-SearcherPoolAuthoritySheetIdentity | Out-Null

$evidenceRoot = Join-Path $PSScriptRoot '..\..\..\..\cycle3-genesis-smoke-evidence'
$evidenceRoot = (New-Item -ItemType Directory -Force -Path $evidenceRoot).FullName
$exe = Get-ReleaseTestExecutableCycle3V1 -EvidenceRoot $evidenceRoot -Label 'genesis-smoke'
Write-Output "RESOLVED_EXE=$exe"

# FIX (caught 2026-08-26, real error surfaced via added diagnostics):
# ExpectedResumeGeneration must stay unset (null) for a fresh genesis
# build. native_science_loop_v1.rs has an explicit, deliberate guard
# ("if expected_resume_generation.is_some() && genesis_required { return
# Err(InputInvalid) }") that rejects supplying an expected-resume
# generation together with a store that still needs genesis published --
# by design, since there is nothing yet to "resume from" at generation 0;
# genesis IS the creation of that state, not a resume of it. An earlier
# revision of this fix wrongly also set ExpectedResumeGeneration=0
# whenever StopAfterGeneration was supplied, which made every real
# genesis attempt fail this guard (empirically confirmed twice). Only
# StopAfterGeneration is needed: the loop's own
# "stop_after_generation == Some(parent_generation)" check (independent
# of expected_resume_generation) still breaks immediately once genesis
# publishes generation 0.
$saved = Set-Cycle3NativeEnvironment -Seed $Seed -Updates $Updates -StoreParent $StoreParent -GpuOrdinal 0 -StopAfterGeneration $StopAfterGeneration
$logPath = Join-Path $evidenceRoot 'genesis-smoke.log'
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
Write-Output "EXIT_CODE=$exitCode"
if ($exitCode -ne 0) {
    Write-Output '--- log tail ---'
    Get-Content -LiteralPath $logPath -Tail 80
    throw "genesis smoke failed with exit code $exitCode"
}

# Task 3 acceptance: generation-0 parameters hash EXACTLY to the pinned
# model_parameter sha. Assert-WarmStartGenZeroCycle3V1 doubles as this check.
$assertResult = Assert-WarmStartGenZeroCycle3V1 -StoreParent $StoreParent
Write-Output ("GEN0_MODEL_PARAMETER_SHA256=" + $assertResult.model_parameter_sha256)
Write-Output 'GENESIS SMOKE PASSED'
