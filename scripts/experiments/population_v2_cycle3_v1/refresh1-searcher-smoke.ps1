param(
    [string]$StoreParent = 'E:\mtg-kernel-population-v2-cycle3\lineage\attempt-001',
    [uint64]$Seed = 977002
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

Assert-Cycle3SheetIdentity | Out-Null
Assert-SearcherPoolAuthoritySheetIdentity | Out-Null

$manifestDir = 'E:\mtg-kernel-population-v2-cycle3\refresh-manifests'
# Task 7 preflight items 2/3, attempted for real. Per the implementation
# report: this is EXPECTED to fail during chain construction, not training,
# because the real (non-test-only) production decoder
# decode_population_tranche_refresh_manifest_v2 requires the FULL historical
# chain back to refresh_index 0, and tranche-1's own three genesis links
# (refresh_index 0-2) are not archived anywhere on this host. Run anyway to
# capture the EXACT failure point as evidence, rather than only asserting it.
$chainPaths = @(18, 19) | ForEach-Object { Join-Path $manifestDir ("population-v3-refresh-{0:D3}.json" -f $_) }
$refreshChain = [string]::Join(';', $chainPaths)

$slotRoots = @(
    'D:\mtg-kernel-ladder-pilot-20260725\pool3\primary'
    'D:\mtg-kernel-scaled-selfplay-population-v1\replay\three-lineage-replay\attempt-001\wave-00-seed-970002-gpu1\run-0\store'
    'D:\mtg-kernel-denovo-campaign-v1\seed-971221\denovo-1024-screen-build\attempt-001\denovo-1024-store\run-0\store'
    'D:\mtg-kernel-denovo-campaign-v1\seed-971223\denovo-1024-screen-build\attempt-002\denovo-1024-store\run-0\store'
    'C:\mtg-kernel-population-v2-cycle2\active\cycle2-active-interval-0256-0384\attempt-001\seed-975001-store\run-0\store'
    (Join-Path $StoreParent 'run-0\store')
    'D:\mtg-kernel-denovo-campaign-v1\seed-971222\denovo-1024-screen-build\attempt-001\denovo-1024-store\run-0\store'
    'D:\mtg-kernel-denovo-campaign-v1\seed-971221\denovo-1024-screen-build\attempt-001\denovo-1024-store\run-0\store'
)
$slotRootsJoined = [string]::Join(';', $slotRoots)

$evidenceRoot = Join-Path $PSScriptRoot '..\..\..\..\cycle3-refresh1-smoke-evidence'
$evidenceRoot = (New-Item -ItemType Directory -Force -Path $evidenceRoot).FullName
$exe = Get-ReleaseTestExecutableCycle3V1 -EvidenceRoot $evidenceRoot -Label 'refresh1-smoke'
Write-Output "RESOLVED_EXE=$exe"

$saved = Set-Cycle3NativeEnvironment -Seed $Seed -Updates 128 -StoreParent $StoreParent `
    -GpuOrdinal 0 -ResumeExistingStore `
    -StopAfterGeneration 128 -ExpectedResumeGeneration 0 `
    -PopulationRuntime -RefreshChain $refreshChain -SlotRoots $slotRootsJoined

$logPath = Join-Path $evidenceRoot 'refresh1-smoke.log'
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
Write-Output '--- log tail ---'
Get-Content -LiteralPath $logPath -Tail 60
