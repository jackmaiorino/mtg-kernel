param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][ValidateSet('T2048')][string]$Tier,
    [Parameter(Mandatory = $true)][ValidateSet(2001001)][uint64]$PoolActionSeed,
    [Parameter(Mandatory = $true)][string]$StdoutPath,
    [Parameter(Mandatory = $true)][string]$StderrPath
)

# Kernel-native search opponent v1 -- pool-registration arm-script seed
# whitelist (CLAUDE-SEARCHER-POOL-AUTHORITY-SHEET-V1.md, countersigned
# 6a0db07d, Section 5 layer 5). Pool-scoped sibling of
# run-diagnostic-registration-smoke.ps1: same shape, narrower allowlists.
#
# This is a REGISTRATION SMOKE, not a training launcher: it runs the
# ignored Rust test `kernel_native_search_opponent_v1::tests::
# kernel_native_search_pool_env_surface_is_registered_and_fails_closed`,
# which validates the env-driven pool surface (tier and action_seed
# allowlist membership) a future pool-manifest builder would resolve
# against. It does not run a search, a rollout, a panel, or any training.
# `#[ignore]`-gated (like its run-level sibling) not because it needs a
# Store or a panel, but because it is env-var-driven and would otherwise
# fail by default in an ordinary `cargo test` run that does not set these
# two variables.
#
# The `-Tier` ValidateSet narrows to `('T2048')` only: T8192 is a reserved
# but NOT-enabled pool tier (design sheet Section 9.3), and T512/T32768 are
# never pool-eligible (Section 4). The `-PoolActionSeed` ValidateSet is the
# pool seed whitelist itself, mirroring
# run-diagnostic-registration-smoke.ps1's own `-EvaluationSeed` pattern, but
# naming the pool-specific placeholder literal
# (`KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1`), never one of the four
# calibration seeds. PLACEHOLDER, per the sheet's Section 13 open item: 2001001
# is not the real cycle-3 launch base_seed; replace this literal (and the
# matching Rust array) with the real, separately authorized value before any
# real cycle-3 training run.

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

foreach ($path in @($StdoutPath, $StderrPath)) {
    if (Test-Path -LiteralPath $path) { throw "refusing to overwrite smoke output: $path" }
}

$env:KERNEL_SEARCH_POOL_TIER = $Tier
$env:KERNEL_SEARCH_POOL_ACTION_SEED = [string]$PoolActionSeed

$test = 'kernel_native_search_opponent_v1::tests::kernel_native_search_pool_env_surface_is_registered_and_fails_closed'
$previous = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
try {
    & $Executable $test --ignored --exact --nocapture --test-threads=1 1> $StdoutPath 2> $StderrPath
    $nativeExitCode = $LASTEXITCODE
}
finally {
    $ErrorActionPreference = $previous
    Remove-Item -Path 'Env:KERNEL_SEARCH_POOL_TIER' -ErrorAction SilentlyContinue
    Remove-Item -Path 'Env:KERNEL_SEARCH_POOL_ACTION_SEED' -ErrorAction SilentlyContinue
}

if ($nativeExitCode -ne 0) { exit 1 }
exit 0
