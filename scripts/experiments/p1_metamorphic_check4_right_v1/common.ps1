Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# P1-METAMORPHIC-AUDIT-DESIGN-V4.md, Check 4: the 1,536-game P1-first right
# column. Every H2H_* binding below except H2H_STARTING_PLAYER is pulled
# VERBATIM from the left column's recorded run configuration, reconstructed
# from the launcher that produced it
# (mtg-kernel-post-rung-diagnostics-v1-codex\scripts\experiments\post_rung_diagnostics_v1\{one-eval,common}.ps1)
# and cross-checked against the left column's own result.json /
# job-set-summary.json fields (opponent_generation=384, pairs=256,
# evaluation_seed=982001, candidate_generation=512). H2H_STARTING_PLAYER=P1
# is the ONLY new binding (P1-METAMORPHIC-AUDIT-DESIGN-V4.md Section 1.2 /
# Check 4).
#
# DEVIATION FROM THE LEFT COLUMN, FLAGGED (not silent): the left column was
# built with `--features experimental-burn-net8-packed-cuda-v1` and pinned
# `MTG_KERNEL_PILOT_CUDA_ORDINAL=1` (CUDA burn-dense backend, GPU ordinal 1).
# This run is directed to be CPU-only (a live training run owns GPU0; build
# WITHOUT the CUDA feature; do not touch GPU state), so this script builds
# without that feature and never sets MTG_KERNEL_PILOT_CUDA_ORDINAL. The
# candidate/opponent model weights, store roots, seeds, and pair roots are
# identical; only the inference numerical backend and wall-clock economics
# differ. See CHECK4-RIGHT-COLUMN-RESULT-V1.md for the full deviation note.

$script:FormalEvidenceRoot = 'D:\mtg-kernel-macro-selfplay-envrand-v2-rung-v1'
$script:PoolRoot = 'D:\mtg-kernel-ladder-pilot-20260725\pool3'
$script:PoolJson = Join-Path $script:PoolRoot 'pool.json'
$script:InitStore = Join-Path $script:PoolRoot 'primary'
$script:InitGeneration = 384
$script:H2hTest = 'native_science_loop_v1::windows_science_loop_tests::ladder_head_to_head_eval_v1'
$script:EvaluationSeed = 982001
$script:CandidateGeneration = 512
$script:FullPairs = 256

function Get-CandidateStore {
    param([Parameter(Mandatory = $true)][uint64]$Seed)

    $store = Join-Path $script:FormalEvidenceRoot "runs\seed-$Seed\run-0\store"
    if (-not (Test-Path -LiteralPath (Join-Path $store 'run.json'))) {
        throw "candidate Store is absent: $store"
    }
    return $store
}

function Get-HeadRecord {
    param(
        [Parameter(Mandatory = $true)][string]$Store,
        [Parameter(Mandatory = $true)][uint64]$Generation
    )

    $headPath = Join-Path $Store ("heads\update-{0:d8}.head.json" -f $Generation)
    if (-not (Test-Path -LiteralPath $headPath)) {
        throw "retained head is absent: $headPath"
    }
    return Get-Content -LiteralPath $headPath -Raw | ConvertFrom-Json
}

function Get-Sha256ForText {
    param([Parameter(Mandatory = $true)][string]$Text)

    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
        return [BitConverter]::ToString($sha.ComputeHash($bytes)).Replace('-', '').ToLowerInvariant()
    }
    finally {
        $sha.Dispose()
    }
}

function Get-FileSha256 {
    param([Parameter(Mandatory = $true)][string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

# Hardened completion-record writer (matching
# scripts\experiments\regularized_continuation_retest_v1\gross-safety-classifier.ps1's
# pattern): atomic CreateNew (fails rather than silently overwriting a prior
# record), no-BOM UTF-8, explicit flush.
function Write-HardenedJsonRecord {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)]$Record
    )

    $json = $Record | ConvertTo-Json -Depth 12
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($json)
    $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush($true)
    }
    finally {
        $stream.Dispose()
    }
}

# Resolves the release --lib test executable WITHOUT the CUDA feature,
# strictly from the cargo JSON build artifacts (house rule: never guess a
# target/ path by name, since a feature-set change silently changes the
# executable hash). Mirrors
# post_rung_diagnostics_v1\common.ps1's Get-ReleaseTestExecutable but omits
# `--features experimental-burn-net8-packed-cuda-v1` deliberately (CPU-only
# directive; GPU0 is owned by a concurrent training run).
function Get-ReleaseTestExecutableNoCuda {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$EvidenceRoot
    )

    New-Item -ItemType Directory -Force -Path $EvidenceRoot | Out-Null
    $jsonPath = Join-Path $EvidenceRoot 'cargo-release-build.jsonl'
    $stderrPath = Join-Path $EvidenceRoot 'cargo-release-build.stderr.log'
    Push-Location $RepoRoot
    try {
        $previousErrorAction = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try {
            $jsonLines = @(& cargo test -p mtg-kernel --release --lib --no-run --message-format=json 2> $stderrPath)
            $cargoExitCode = $LASTEXITCODE
        }
        finally {
            $ErrorActionPreference = $previousErrorAction
        }
        if ($cargoExitCode -ne 0) {
            throw "release build failed; see $stderrPath"
        }
    }
    finally {
        Pop-Location
    }
    $jsonLines | Set-Content -LiteralPath $jsonPath -Encoding utf8
    $executables = foreach ($line in $jsonLines) {
        try {
            $item = $line | ConvertFrom-Json
            if (
                $item.reason -eq 'compiler-artifact' -and
                $item.target.name -eq 'mtg_kernel' -and
                $item.target.kind -contains 'lib' -and
                $null -ne $item.executable
            ) {
                [string]$item.executable
            }
        }
        catch {
        }
    }
    $executable = $executables | Select-Object -Last 1
    if ([string]::IsNullOrWhiteSpace($executable) -or -not (Test-Path -LiteralPath $executable)) {
        throw 'release mtg_kernel lib-test executable was not resolved from Cargo JSON'
    }
    return (Resolve-Path -LiteralPath $executable).Path
}
