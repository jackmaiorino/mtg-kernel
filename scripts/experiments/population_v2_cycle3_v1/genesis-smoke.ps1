param(
    [string]$StoreParent = 'E:\mtg-kernel-population-v2-cycle3\lineage\smoke-attempt-001',
    [uint64]$Seed = 977002,
    [uint64]$Updates = 4
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

Assert-Cycle3SheetIdentity | Out-Null

$evidenceRoot = Join-Path $PSScriptRoot '..\..\..\..\cycle3-genesis-smoke-evidence'
$evidenceRoot = (New-Item -ItemType Directory -Force -Path $evidenceRoot).FullName
$exe = Get-ReleaseTestExecutableCycle3V1 -EvidenceRoot $evidenceRoot -Label 'genesis-smoke'
Write-Output "RESOLVED_EXE=$exe"

$saved = Set-Cycle3NativeEnvironment -Seed $Seed -Updates $Updates -StoreParent $StoreParent -GpuOrdinal 0
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
