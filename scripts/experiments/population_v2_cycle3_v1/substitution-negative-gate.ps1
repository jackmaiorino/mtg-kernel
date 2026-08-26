# Cycle-3 Task 5(b): substitution negative gate. A genesis seeded from the
# WRONG parent (the current-0/DISQUALIFIED-lineage copy, base_seed 975001
# family, at D:\throughput-remeasure-20260825\v2-resume-walk\store-depth2048-cycle2)
# must hard-stop at Layer 2 (Assert-WarmStartGenZeroCycle3V1), because its
# generation-0 model_parameter_sha256 will not equal the pinned cycle-3
# parent (current-1@2048's) value. This script is EXPECTED to throw; a
# successful outcome for this gate is the assert firing, not exit-code-0.
param(
    [string]$StoreParent = 'E:\mtg-kernel-population-v2-cycle3\lineage\substitution-negative-gate-001',
    [uint64]$Seed = 977002,
    [uint64]$Updates = 4
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

Assert-Cycle3SheetIdentity | Out-Null

$evidenceRoot = Join-Path $PSScriptRoot '..\..\..\..\cycle3-genesis-smoke-evidence'
$evidenceRoot = (New-Item -ItemType Directory -Force -Path $evidenceRoot).FullName
$exe = Get-ReleaseTestExecutableCycle3V1 -EvidenceRoot $evidenceRoot -Label 'substitution-gate'
Write-Output "RESOLVED_EXE=$exe"

$saved = Set-Cycle3NativeEnvironment -Seed $Seed -Updates $Updates -StoreParent $StoreParent -GpuOrdinal 0
# Override the parent store to the WRONG (current-0/disqualified) lineage,
# after Set-Cycle3NativeEnvironment has set the correct default.
[Environment]::SetEnvironmentVariable('MULTIRUN_LADDER_INIT_STORE', 'D:\throughput-remeasure-20260825\v2-resume-walk\store-depth2048-cycle2', 'Process')
[Environment]::SetEnvironmentVariable('MULTIRUN_LADDER_INIT_GEN', '2048', 'Process')

$logPath = Join-Path $evidenceRoot 'substitution-gate.log'
$gateFired = $false
$gateMessage = $null
try {
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & $exe $script:RunnerTest --ignored --exact --nocapture --test-threads=1 2>&1 |
            Tee-Object -FilePath $logPath | Out-Null
        $exitCode = $LASTEXITCODE
    }
    finally { $ErrorActionPreference = $previous }
    Write-Output "TRAINING_EXIT_CODE=$exitCode"
    if ($exitCode -ne 0) {
        throw "training process itself failed (exit $exitCode) before Layer 2 could even run -- see $logPath"
    }
    try {
        Assert-WarmStartGenZeroCycle3V1 -StoreParent $StoreParent | Out-Null
    }
    catch {
        $gateFired = $true
        $gateMessage = $_.Exception.Message
    }
}
finally {
    Restore-Cycle3NativeEnvironment -Saved $saved
}

if (-not $gateFired) {
    throw 'SUBSTITUTION GATE FAILED TO FIRE: Assert-WarmStartGenZeroCycle3V1 did not reject the wrong-parent genesis'
}
Write-Output "GATE_FIRED_MESSAGE=$gateMessage"
Write-Output 'SUBSTITUTION NEGATIVE GATE PASSED (Layer 2 correctly hard-stopped the wrong-parent genesis)'
