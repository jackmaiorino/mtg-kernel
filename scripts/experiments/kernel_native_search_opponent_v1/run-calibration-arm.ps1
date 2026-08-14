param(
    [Parameter(Mandatory = $true)][string]$Label,
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][ValidateSet('T512', 'T2048', 'T8192', 'T32768')][string]$Tier,
    [Parameter(Mandatory = $true)][ValidateSet(1987001, 1988001, 1989001, 1990001)][uint64]$BaseSeed,
    [Parameter(Mandatory = $true)][string]$CheckpointStoreRoot,
    [Parameter(Mandatory = $true)][uint64]$CheckpointGeneration,
    [uint64]$FirstPairIndex = 0,
    [Parameter(Mandatory = $true)][uint64]$PairCount,
    [uint64]$TimeoutSeconds = 3600,
    [Parameter(Mandatory = $true)][string]$OutcomePath,
    [Parameter(Mandatory = $true)][string]$StdoutPath,
    [Parameter(Mandatory = $true)][string]$StderrPath,
    [Parameter(Mandatory = $true)][string]$CompletionPath
)

# Runs ONE calibration screen of KERNEL-NATIVE-SEARCH-OPPONENT-V1-DESIGN.md
# "Calibration after implementation" via the `--ignored` Rust launcher
# `kernel_native_search_calibration_runner_v1::windows_calibration_tests::
# kernel_native_search_calibration_screen_v1`. This is the same house
# pattern as the mirror-seat diagnostic's `run-mirror-arm.ps1` (env clearing,
# native invocation under `$ErrorActionPreference = 'Continue'` with direct
# stdout/stderr redirection -- the safe way to capture a native process's
# stderr under PowerShell 5.1 without native-command output being wrapped as
# a NativeCommandError -- fresh non-overwritable output paths, and a
# completion JSON written only after the native process exits) and the same
# `-Tier`/seed `-ValidateSet` discipline as this directory's own
# `run-diagnostic-registration-smoke.ps1`.
#
# This script runs exactly ONE arm (one tier, one base seed, one checkpoint,
# one pair range). It does not select a tier or an opponent from outcomes
# and it does not orchestrate multiple arms; per
# KERNEL-NATIVE-SEARCH-OPPONENT-V1-DESIGN.md, the coordinator decides which
# screens to run and reviews every result. Invocation lines for each screen:
#
#   Throughput screen (freely retryable, all four tiers, 16 pairs each):
#     -Tier T512|T2048|T8192|T32768 -BaseSeed 1987001 -PairCount 16 -FirstPairIndex 0
#     -CheckpointStoreRoot/-CheckpointGeneration: any already-validated checkpoint
#     (capacity evidence only; the design names no specific opponent for this screen).
#
#   Matched panel vs promoted(2) (one tier at a time, 256 pairs):
#     -BaseSeed 1988001 -PairCount 256 -FirstPairIndex 0
#     -CheckpointStoreRoot/-CheckpointGeneration: promoted(2)'s own store root/generation.
#
#   Matched panel vs the frozen de-novo line (one tier at a time, 256 pairs):
#     -BaseSeed 1989001 -PairCount 256 -FirstPairIndex 0
#     -CheckpointStoreRoot/-CheckpointGeneration: the frozen de-novo line's store root/generation.
#
#   One-pair smoke (this branch's own required gate, CPU, minutes):
#     -Tier T512 -BaseSeed 1987001 -PairCount 1 -FirstPairIndex 0
#     -CheckpointStoreRoot/-CheckpointGeneration: any already-validated checkpoint.
#
# Base seed 1990001 (the post-bridge-parity CP7 panel) is a valid
# `-BaseSeed` value here because it is on the authority's registered seed
# allowlist, but running that panel is not authorized until bridge parity is
# requalified post-merge; this script enforces no calendar/authorization
# gate itself, matching the design's own placement of that decision with
# the coordinator, not the harness.
#
# KNOWN BLOCKER (see kernel_native_search_calibration_runner_v1.rs module
# docs for the full investigation): on this worktree, EVERY arm currently
# fails at checkpoint load with a `RunDecode` error, because
# `validate_frozen_rev3_authorities_v2` (native_training_store_run_v2.rs)
# unconditionally rejects every `ValidatedTrainRunV2` construction --
# fixture or real -- due to a CardDB-hash/runtime-catalog-hash drift against
# a frozen rev3 gate. This is a pre-existing, worktree-wide defect, not
# something this script or its Rust launcher can route around.

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$testName = 'kernel_native_search_calibration_runner_v1::windows_calibration_tests::kernel_native_search_calibration_screen_v1'
$environmentNames = @(
    'KNS_TIER', 'KNS_BASE_SEED', 'KNS_CHECKPOINT_STORE_ROOT', 'KNS_CHECKPOINT_GEN',
    'KNS_FIRST_PAIR_INDEX', 'KNS_PAIR_COUNT', 'KNS_TIMEOUT_SECONDS', 'KNS_OUTCOME_JSON',
    'CUDA_VISIBLE_DEVICES'
)

function Clear-CalibrationEnvironmentV1 {
    foreach ($name in $environmentNames) { Remove-Item -Path "Env:$name" -ErrorAction SilentlyContinue }
}

function Get-Sha256V1 {
    param([Parameter(Mandatory = $true)][string]$Path)
    (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Write-NewJsonV1 {
    param([Parameter(Mandatory = $true)]$Value, [Parameter(Mandatory = $true)][string]$Path)
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 12))
    $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $stream.Write($bytes, 0, $bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
}

try {
    foreach ($path in @($OutcomePath, $StdoutPath, $StderrPath, $CompletionPath)) {
        if (Test-Path -LiteralPath $path) { throw "refusing to overwrite calibration arm output: $path" }
    }
    $resolvedCheckpointStoreRoot = (Resolve-Path -LiteralPath $CheckpointStoreRoot).Path

    Clear-CalibrationEnvironmentV1
    # CPU-only: the search authority's tree walk and static evaluator are
    # CPU-bound; this harness never installs a GPU scorer for the searcher.
    $env:CUDA_VISIBLE_DEVICES = '-1'
    $env:KNS_TIER = $Tier
    $env:KNS_BASE_SEED = [string]$BaseSeed
    $env:KNS_CHECKPOINT_STORE_ROOT = $resolvedCheckpointStoreRoot
    $env:KNS_CHECKPOINT_GEN = [string]$CheckpointGeneration
    $env:KNS_FIRST_PAIR_INDEX = [string]$FirstPairIndex
    $env:KNS_PAIR_COUNT = [string]$PairCount
    $env:KNS_TIMEOUT_SECONDS = [string]$TimeoutSeconds
    $env:KNS_OUTCOME_JSON = $OutcomePath

    $started = [DateTimeOffset]::UtcNow
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $previous = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    try { & $Executable $testName --ignored --exact --nocapture --test-threads=1 1> $StdoutPath 2> $StderrPath; $nativeExitCode = $LASTEXITCODE }
    finally { $ErrorActionPreference = $previous; Clear-CalibrationEnvironmentV1; $clock.Stop() }

    $outcomeCreated = Test-Path -LiteralPath $OutcomePath -PathType Leaf
    $stdoutCreated = Test-Path -LiteralPath $StdoutPath -PathType Leaf
    $stderrCreated = Test-Path -LiteralPath $StderrPath -PathType Leaf
    $completion = [ordered]@{
        schema = 'mtg-kernel-kernel-native-search-calibration-arm-completion/v1'
        label = $Label
        success = ($nativeExitCode -eq 0 -and $outcomeCreated -and $stdoutCreated -and $stderrCreated)
        tier = $Tier
        base_seed = $BaseSeed
        checkpoint_store_root = $resolvedCheckpointStoreRoot
        checkpoint_generation = $CheckpointGeneration
        first_pair_index = $FirstPairIndex
        pair_count = $PairCount
        timeout_seconds = $TimeoutSeconds
        native_exit_code = $nativeExitCode
        executable_sha256 = Get-Sha256V1 -Path $Executable
        resource_binding = 'cpu-only; CUDA_VISIBLE_DEVICES=-1'
        outcome_created = $outcomeCreated
        outcome_sha256 = if ($outcomeCreated) { Get-Sha256V1 -Path $OutcomePath } else { $null }
        stdout_created = $stdoutCreated; stderr_created = $stderrCreated
        started_utc = $started.ToString('O'); wall_seconds = $clock.Elapsed.TotalSeconds
        completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
    }
    Write-NewJsonV1 -Value $completion -Path $CompletionPath
    if (-not $completion.success) { exit 1 }
} catch { Clear-CalibrationEnvironmentV1; Write-Error $_; exit 1 }
