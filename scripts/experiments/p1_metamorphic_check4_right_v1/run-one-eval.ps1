param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][string]$OutputRoot,
    [Parameter(Mandatory = $true)][uint64]$CandidateSeed,
    [uint64]$Pairs = 256,
    [uint64]$FirstPair = 0
)

# P1-METAMORPHIC-AUDIT-DESIGN-V4.md Check 4, right column (P1-first).
# Candidates: generation-512 seeds 970001/970002/970003 vs promoted(2)
# (pool3\primary, generation 384), on the identical 256 (pair_index,
# environment_seed) roots the left column used (evaluation_seed 982001,
# reveal order ascending pair_index). Every H2H_* binding below except
# H2H_STARTING_PLAYER is verbatim from the left column's launcher
# (post_rung_diagnostics_v1\one-eval.ps1 + common.ps1); H2H_STARTING_PLAYER=P1
# is the one new binding this design authorizes.
#
# DEVIATION, FLAGGED: the left column's binary carried a commit
# ("Export P1 diagnostic episode metadata", f9c05fb) that emitted
# H2H_EPISODE_JSON lines gated on H2H_EPISODE_DIAGNOSTICS_V1. That commit is
# NOT an ancestor of fable/starting-player-authority-v1 (confirmed via
# `git merge-base --is-ancestor f9c05fb HEAD`) -- the two branches diverged
# from a common point before it. H2H_EPISODE_DIAGNOSTICS_V1 is therefore
# inert on this branch. This script instead uses H2H_OUTCOME_JSON, a
# pre-existing, independent, already-hardened artifact mechanism on THIS
# branch (native_science_loop_v1.rs, create-new file, sha256-logged) that
# exports the same per-leg (pair_index, environment_seed, learner_seat,
# terminal_order_rank) rows needed for the per-cluster Check-4 analysis,
# under the "episodes" key. Functionally equivalent for this design's
# purposes; not byte-identical in shape to the left column's episodes.jsonl.

. (Join-Path $PSScriptRoot 'common.ps1')

if (Test-Path -LiteralPath $OutputRoot) {
    throw "refusing to reuse evaluation output: $OutputRoot"
}
New-Item -ItemType Directory -Path $OutputRoot | Out-Null

$actualExecutableSha256 = Get-FileSha256 -Path $Executable
$store = Get-CandidateStore -Seed $CandidateSeed
$head = Get-HeadRecord -Store $store -Generation $script:CandidateGeneration
$log = Join-Path $OutputRoot 'h2h.log'
$started = Get-Date

# Verbatim from the left column's configuration (post_rung_diagnostics_v1),
# with H2H_STARTING_PLAYER=P1 added and MTG_KERNEL_PILOT_CUDA_ORDINAL
# deliberately NOT set (CPU-only build/run; see common.ps1's deviation note).
$env:H2H_CANDIDATE_STORE_ROOT = $store
$env:H2H_CANDIDATE_GEN = [string]$script:CandidateGeneration
$env:H2H_CANDIDATE_BASE_SEED = [string]$CandidateSeed
$env:H2H_CANDIDATE_POOL_JSON = $script:PoolJson
$env:H2H_UPDATES = '512'
$env:H2H_INIT_STORE = $script:InitStore
$env:H2H_INIT_GEN = [string]$script:InitGeneration
$env:H2H_OPPONENT_STORE_ROOT = $script:InitStore
$env:H2H_OPPONENT_GEN = [string]$script:InitGeneration
$env:H2H_PAIRS = [string]$Pairs
$env:H2H_FIRST_PAIR = [string]$FirstPair
$env:H2H_EVAL_SEED = [string]$script:EvaluationSeed
$env:H2H_ENVIRONMENT_RANDOMIZATION_V2 = '1'
$env:H2H_STARTING_PLAYER = 'P1'
$outcomeJsonPath = Join-Path $OutputRoot 'outcome.json'
$env:H2H_OUTCOME_JSON = $outcomeJsonPath

try {
    $previousErrorAction = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & $Executable $script:H2hTest --ignored --exact --nocapture --test-threads=1 2>&1 |
            Tee-Object -FilePath $log |
            Out-Host
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorAction
    }
}
finally {
    @(
        'H2H_CANDIDATE_STORE_ROOT', 'H2H_CANDIDATE_GEN',
        'H2H_CANDIDATE_BASE_SEED', 'H2H_CANDIDATE_POOL_JSON', 'H2H_UPDATES',
        'H2H_INIT_STORE', 'H2H_INIT_GEN', 'H2H_OPPONENT_STORE_ROOT',
        'H2H_OPPONENT_GEN', 'H2H_PAIRS', 'H2H_FIRST_PAIR', 'H2H_EVAL_SEED',
        'H2H_ENVIRONMENT_RANDOMIZATION_V2',
        'H2H_STARTING_PLAYER', 'H2H_OUTCOME_JSON'
    ) | ForEach-Object { Remove-Item -Path "Env:$_" -ErrorAction SilentlyContinue }
}

$text = Get-Content -LiteralPath $log -Raw

# Hardened style: the completion record is written regardless of outcome
# (native exit code is itself part of the record), but a set of binding
# sanity checks must pass before the record is trusted as valid Check-4
# evidence; failures throw with the native exit code preserved in the
# thrown message and nothing partially written is left silently unflagged.
$bindingChecks = [ordered]@{
    opponent_resolved_gen_384_pinned = ($text -match 'H2H opponent_resolved_gen=384 pinned=true')
    envrand_v2_true                  = ($text -match 'H2H envrand_v2=true')
    # crate::ids::PlayerId is a Debug-derived tuple struct (ids.rs:20-24):
    # `pub const P1: PlayerId = PlayerId(1);`. Its {:?} rendering is
    # literally "PlayerId(1)", confirmed by direct source read and by this
    # script's own smoke run. This is the exact, unambiguous P1 marker.
    starting_player_p1               = ($text -match 'H2H starting_player=Some\(PlayerId\(1\)\)')
}

$games = 2 * $Pairs
$overall = [regex]::Match($text, "H2H candidate_gen=$($script:CandidateGeneration) wide=false W/L/D (\d+)/(\d+)/(\d+) of $games")
$p0 = [regex]::Match($text, "H2H candidate_gen=$($script:CandidateGeneration) wide=false learner_seat=P0 W/L/D (\d+)/(\d+)/(\d+) of $Pairs")
$p1 = [regex]::Match($text, "H2H candidate_gen=$($script:CandidateGeneration) wide=false learner_seat=P1 W/L/D (\d+)/(\d+)/(\d+) of $Pairs")

$episodeDiagnosticsPath = $null
$episodeDiagnosticCount = 0
$outcomeArtifactSha256 = $null
if (Test-Path -LiteralPath $outcomeJsonPath) {
    $outcomeDoc = Get-Content -LiteralPath $outcomeJsonPath -Raw | ConvertFrom-Json
    $outcomeArtifactSha256 = Get-FileSha256 -Path $outcomeJsonPath
    $episodes = @($outcomeDoc.episodes)
    $episodeDiagnosticsPath = Join-Path $OutputRoot 'episodes.jsonl'
    $episodeLines = foreach ($row in $episodes) {
        # Normalize to the analysis-facing shape: pair_index, environment_seed,
        # learner_seat, reward (renamed from terminal_order_rank for clarity;
        # same {-1,0,1} value, see native_science_loop_v1.rs outcome_rows).
        [ordered]@{
            pair_index = $row.pair_index
            environment_seed = $row.environment_seed
            learner_seat = $row.learner_seat
            reward = $row.terminal_order_rank
        } | ConvertTo-Json -Depth 4 -Compress
    }
    $episodeLines | Set-Content -LiteralPath $episodeDiagnosticsPath -Encoding UTF8
    $episodeDiagnosticCount = $episodes.Count
}

$normalizedLines = Select-String -LiteralPath $log -Pattern '^H2H ' | ForEach-Object { $_.Line.Trim() }
$normalized = ($normalizedLines -join "`n") + "`n"

$result = [ordered]@{
    schema = 'mtg-kernel-check4-right-column-eval/v1'
    design_doc = 'P1-METAMORPHIC-AUDIT-DESIGN-V4.md Check 4 (right column, P1-first)'
    candidate_seed = $CandidateSeed
    candidate_generation = $script:CandidateGeneration
    candidate_model_parameter_sha256 = $head.model_parameter_sha256
    candidate_store_root = $store
    opponent_store_root = $script:InitStore
    opponent_generation = $script:InitGeneration
    pairs = $Pairs
    games = $games
    first_pair = $FirstPair
    evaluation_seed = $script:EvaluationSeed
    starting_player = 'P1'
    environment_randomization_v2 = $true
    wide = $false
    native_exit_code = $exitCode
    binding_checks = $bindingChecks
    binding_checks_all_passed = -not ($bindingChecks.Values -contains $false)
    wins = if ($overall.Success) { [uint64]$overall.Groups[1].Value } else { $null }
    losses = if ($overall.Success) { [uint64]$overall.Groups[2].Value } else { $null }
    draws = if ($overall.Success) { [uint64]$overall.Groups[3].Value } else { $null }
    p0_wins = if ($p0.Success) { [uint64]$p0.Groups[1].Value } else { $null }
    p0_losses = if ($p0.Success) { [uint64]$p0.Groups[2].Value } else { $null }
    p0_draws = if ($p0.Success) { [uint64]$p0.Groups[3].Value } else { $null }
    p1_wins = if ($p1.Success) { [uint64]$p1.Groups[1].Value } else { $null }
    p1_losses = if ($p1.Success) { [uint64]$p1.Groups[2].Value } else { $null }
    p1_draws = if ($p1.Success) { [uint64]$p1.Groups[3].Value } else { $null }
    normalized_h2h_sha256 = Get-Sha256ForText -Text $normalized
    wall_seconds = [math]::Round(((Get-Date) - $started).TotalSeconds, 3)
    executable_sha256 = $actualExecutableSha256
    episode_diagnostic_count = $episodeDiagnosticCount
    episode_diagnostics_path = $episodeDiagnosticsPath
    outcome_artifact_path = $outcomeJsonPath
    outcome_artifact_sha256 = $outcomeArtifactSha256
    log = $log
}
Write-HardenedJsonRecord -Path (Join-Path $OutputRoot 'result.json') -Record $result
$result | ConvertTo-Json -Depth 8

if ($exitCode -ne 0) {
    throw "head-to-head evaluator failed with native exit code $exitCode; see $log (result.json was still written with this exit code recorded)"
}
if ($bindingChecks.Values -contains $false) {
    throw "one or more required H2H binding markers did not appear in the log; see result.json binding_checks and $log"
}
if (-not $overall.Success -or -not $p0.Success -or -not $p1.Success) {
    throw "a required dynamic H2H result marker is absent; see $log"
}
if ($null -eq $outcomeArtifactSha256 -or $episodeDiagnosticCount -ne $games) {
    throw "H2H_OUTCOME_JSON artifact missing or row count ($episodeDiagnosticCount) != expected games ($games); see $outcomeJsonPath"
}
