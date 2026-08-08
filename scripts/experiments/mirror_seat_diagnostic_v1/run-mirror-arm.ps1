param(
    [Parameter(Mandatory = $true)][string]$Label,
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][string]$StoreRoot,
    [Parameter(Mandatory = $true)][uint64]$Generation,
    [Parameter(Mandatory = $true)][uint64]$PairCount,
    [Parameter(Mandatory = $true)][uint64]$EvaluationSeed,
    [Parameter(Mandatory = $true)][string]$OutcomePath,
    [Parameter(Mandatory = $true)][string]$StdoutPath,
    [Parameter(Mandatory = $true)][string]$StderrPath,
    [Parameter(Mandatory = $true)][string]$CompletionPath
)

# Runs ONE mirror arm of CLAUDE-MIRROR-SEAT-DIAGNOSTIC-V1: the SAME checkpoint
# (StoreRoot/Generation) is bound as both the H2H "candidate" and the H2H
# "opponent" of native_science_loop_v1::windows_science_loop_tests::
# ladder_head_to_head_eval_v1 ("pure" H2H mode). That test already produces
# CRN seat-swapped pairs (one shared environment_seed per pair, one leg with
# the candidate as P0, one leg with the candidate as P1); binding candidate
# == opponent turns that fixed-opponent probe into a mirror match with no
# source change. CPU-only: CUDA_VISIBLE_DEVICES=-1.

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$testName = 'native_science_loop_v1::windows_science_loop_tests::ladder_head_to_head_eval_v1'
$environmentNames = @(
    'H2H_CANDIDATE_STORE_ROOT', 'H2H_CANDIDATE_GEN', 'H2H_CANDIDATE_USE_STORE_RUN',
    'H2H_CANDIDATE_BASE_SEED', 'H2H_CANDIDATE_POOL_JSON',
    'H2H_OPPONENT_STORE_ROOT', 'H2H_OPPONENT_GEN', 'H2H_PAIRS', 'H2H_EVAL_SEED',
    'H2H_ENVIRONMENT_RANDOMIZATION_V2', 'H2H_OUTCOME_JSON', 'H2H_UPDATES',
    'H2H_INIT_STORE', 'H2H_INIT_GEN', 'WIDE', 'CUDA_VISIBLE_DEVICES'
)

function Clear-MirrorEnvironmentV1 {
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
        if (Test-Path -LiteralPath $path) { throw "refusing to overwrite mirror arm output: $path" }
    }
    $resolvedStoreRoot = (Resolve-Path -LiteralPath $StoreRoot).Path

    Clear-MirrorEnvironmentV1
    $env:CUDA_VISIBLE_DEVICES = '-1'
    $env:H2H_CANDIDATE_STORE_ROOT = $resolvedStoreRoot
    $env:H2H_CANDIDATE_GEN = [string]$Generation
    $env:H2H_CANDIDATE_USE_STORE_RUN = '1'
    $env:H2H_OPPONENT_STORE_ROOT = $resolvedStoreRoot
    $env:H2H_OPPONENT_GEN = [string]$Generation
    $env:H2H_PAIRS = [string]$PairCount
    $env:H2H_EVAL_SEED = [string]$EvaluationSeed
    $env:H2H_ENVIRONMENT_RANDOMIZATION_V2 = '1'
    $env:H2H_OUTCOME_JSON = $OutcomePath

    $started = [DateTimeOffset]::UtcNow
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $previous = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    try { & $Executable $testName --ignored --exact --nocapture --test-threads=1 1> $StdoutPath 2> $StderrPath; $nativeExitCode = $LASTEXITCODE }
    finally { $ErrorActionPreference = $previous; Clear-MirrorEnvironmentV1; $clock.Stop() }

    $outcomeCreated = Test-Path -LiteralPath $OutcomePath -PathType Leaf
    $stdoutCreated = Test-Path -LiteralPath $StdoutPath -PathType Leaf
    $stderrCreated = Test-Path -LiteralPath $StderrPath -PathType Leaf
    $completion = [ordered]@{
        schema = 'mtg-kernel-mirror-seat-diagnostic-arm-completion/v1'
        label = $Label
        success = ($nativeExitCode -eq 0 -and $outcomeCreated -and $stdoutCreated -and $stderrCreated)
        candidate_store_root = $resolvedStoreRoot
        candidate_generation = $Generation
        opponent_store_root = $resolvedStoreRoot
        opponent_generation = $Generation
        mirror_binding = 'candidate_store_root == opponent_store_root and candidate_generation == opponent_generation'
        pair_count = $PairCount
        evaluation_seed = $EvaluationSeed
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
} catch { Clear-MirrorEnvironmentV1; Write-Error $_; exit 1 }
