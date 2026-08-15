param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][ValidateSet('T512', 'T2048', 'T8192', 'T32768')][string]$Tier,
    [Parameter(Mandatory = $true)][uint64]$PairCount,
    [Parameter(Mandatory = $true)][ValidateSet(1987001, 1988001, 1989001, 1990001)][uint64]$EvaluationSeed,
    [Parameter(Mandatory = $true)][string]$StdoutPath,
    [Parameter(Mandatory = $true)][string]$StderrPath
)

# Kernel-native search opponent v1 -- arm-script seed whitelist (seven
# registered-lineage layers, item e; KERNEL-NATIVE-SEARCH-OPPONENT-V1-
# DESIGN.md "Runtime and authority integration").
#
# This is a REGISTRATION SMOKE, not a calibration launcher: it runs the
# ignored Rust test `kernel_native_search_opponent_v1::tests::
# kernel_native_search_diagnostic_env_surface_is_registered_and_fails_closed`,
# which validates the env-driven surface (tier name, pair-count bound, and
# eval-seed allowlist membership) that a future calibration launcher would
# resolve against. It does not run a search, a rollout, or any panel.
# Running an actual calibration panel (the 16-pair throughput screen or the
# 256-pair matched panels against promoted(2)/de-novo, base seeds 1988001/
# 1989001/1990001, or the CP7 skill-7 bridge panel at 1990001) is the
# coordinator's job, per KERNEL-NATIVE-SEARCH-OPPONENT-V1-DESIGN.md
# "Calibration after implementation" -- a separate, later, owner-authorized
# phase, not stage-2 implementation.
#
# The `-ValidateSet` on `-EvaluationSeed` is the whitelist itself, mirroring
# the response-exploiter denovo-screen lineage's own arm-script pattern
# (scripts/experiments/response_exploiter_denovo_screen_v1/
# run-denovo-probe-arm.ps1's `-EvaluationSeed` ValidateSet).

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ($PairCount -lt 1 -or $PairCount -gt 256) {
    throw 'diagnostic panel must be the 16-pair throughput screen, the 256-pair matched panel, or a smaller smoke pair count (1..256)'
}

foreach ($path in @($StdoutPath, $StderrPath)) {
    if (Test-Path -LiteralPath $path) { throw "refusing to overwrite smoke output: $path" }
}

$env:KERNEL_SEARCH_DIAGNOSTIC_TIER = $Tier
$env:KERNEL_SEARCH_DIAGNOSTIC_PAIRS = [string]$PairCount
$env:KERNEL_SEARCH_DIAGNOSTIC_EVAL_SEED = [string]$EvaluationSeed

$test = 'kernel_native_search_opponent_v1::tests::kernel_native_search_diagnostic_env_surface_is_registered_and_fails_closed'
$previous = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
try {
    & $Executable $test --ignored --exact --nocapture --test-threads=1 1> $StdoutPath 2> $StderrPath
    $nativeExitCode = $LASTEXITCODE
}
finally {
    $ErrorActionPreference = $previous
    Remove-Item -Path 'Env:KERNEL_SEARCH_DIAGNOSTIC_TIER' -ErrorAction SilentlyContinue
    Remove-Item -Path 'Env:KERNEL_SEARCH_DIAGNOSTIC_PAIRS' -ErrorAction SilentlyContinue
    Remove-Item -Path 'Env:KERNEL_SEARCH_DIAGNOSTIC_EVAL_SEED' -ErrorAction SilentlyContinue
}

if ($nativeExitCode -ne 0) { exit 1 }
exit 0
