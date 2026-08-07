Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Shared helpers for the mirror-seat diagnostic (CLAUDE-MIRROR-SEAT-DIAGNOSTIC-V1).
# Status: descriptive diagnostic, no gate, no promotion authority. This file
# intentionally does not pin an ExpectedBaseCommit or a fixed GPU identity:
# unlike the campaign-grade launchers under regularized_continuation_retest_v1
# and scaled_selfplay_population_v1, this diagnostic makes no promotion claim
# and every arm runs CPU-only (CUDA_VISIBLE_DEVICES=-1), so GPU identity is
# not part of its evidence contract. Git commit, dirty status, and the
# resolved test executable's SHA-256 are still recorded on every run.

function Assert-LastExitCode {
    param([Parameter(Mandatory = $true)][int]$ExitCode, [Parameter(Mandatory = $true)][string]$Label)
    if ($ExitCode -ne 0) { throw "$Label failed with exit code $ExitCode" }
}

function Get-TextSha256 {
    param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Text)
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    try {
        $hash = $hasher.ComputeHash($bytes)
        return ([BitConverter]::ToString($hash)).Replace('-', '').ToLowerInvariant()
    } finally { $hasher.Dispose() }
}

function Get-Sha256V1 {
    param([Parameter(Mandatory = $true)][string]$Path)
    (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-FileRecordV1 {
    param([Parameter(Mandatory = $true)][string]$Path)
    $item = Get-Item -LiteralPath $Path
    [ordered]@{ path = $item.FullName; bytes = [uint64]$item.Length; sha256 = Get-Sha256V1 $item.FullName }
}

function Write-NewJsonV1 {
    param([Parameter(Mandatory = $true)]$Value, [Parameter(Mandatory = $true)][string]$Path)
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes(($Value | ConvertTo-Json -Depth 20))
    $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $stream.Write($bytes, 0, $bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
}

function Get-GitRecord {
    param([Parameter(Mandatory = $true)][string]$RepoRoot)
    $safe = $RepoRoot.Replace('\', '/')
    $status = @(& git -c "safe.directory=$safe" -C $RepoRoot status --porcelain 2>&1)
    Assert-LastExitCode $LASTEXITCODE 'git status'
    $head = (@(& git -c "safe.directory=$safe" -C $RepoRoot rev-parse HEAD 2>&1) -join "`n").Trim()
    Assert-LastExitCode $LASTEXITCODE 'git rev-parse'
    $branch = (@(& git -c "safe.directory=$safe" -C $RepoRoot rev-parse --abbrev-ref HEAD 2>&1) -join "`n").Trim()
    Assert-LastExitCode $LASTEXITCODE 'git branch'
    $diff = @(& git -c "safe.directory=$safe" -C $RepoRoot diff --binary HEAD 2>&1)
    Assert-LastExitCode $LASTEXITCODE 'git diff'
    return [ordered]@{
        commit = $head; branch = $branch
        dirty = ($status.Count -ne 0)
        status_sha256 = Get-TextSha256 (($status -join "`n"))
        worktree_diff_sha256 = Get-TextSha256 (($diff -join "`n"))
    }
}

function Get-ReleaseTestExecutableV1 {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$EvidenceRoot,
        [Parameter(Mandatory = $true)][ValidatePattern('^[a-z0-9-]+$')][string]$Label
    )
    New-Item -ItemType Directory -Force -Path $EvidenceRoot | Out-Null
    $jsonPath = Join-Path $EvidenceRoot "cargo-release-build-$Label.jsonl"
    $stderrPath = Join-Path $EvidenceRoot "cargo-release-build-$Label.stderr.log"
    Push-Location $RepoRoot
    try {
        $previous = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try {
            # Deliberately WITHOUT experimental-burn-net8-packed-cuda-v1: the
            # mirror arms only call ladder_head_to_head_eval_v1 (the "pure"
            # candidate-vs-fixed-opponent H2H probe), which is not behind
            # that feature gate, and every arm is CPU-only by contract
            # (CUDA_VISIBLE_DEVICES=-1). Omitting the feature avoids pulling
            # in burn/burn-cuda/cudarc entirely.
            $jsonLines = @(& cargo test -p mtg-kernel --release --lib --no-run --message-format=json 2> $stderrPath)
            $cargoExit = $LASTEXITCODE
        } finally { $ErrorActionPreference = $previous }
        Assert-LastExitCode $cargoExit "cargo release build; see $stderrPath"
    } finally { Pop-Location }
    $jsonLines | Set-Content -LiteralPath $jsonPath -Encoding utf8
    $executables = foreach ($line in $jsonLines) {
        try {
            $item = $line | ConvertFrom-Json
            if ($item.reason -eq 'compiler-artifact' -and $item.target.name -eq 'mtg_kernel' -and $item.target.kind -contains 'lib' -and $null -ne $item.executable) {
                [string]$item.executable
            }
        } catch {}
    }
    $executable = $executables | Select-Object -Last 1
    if ([string]::IsNullOrWhiteSpace($executable) -or -not (Test-Path -LiteralPath $executable)) {
        throw 'mtg_kernel release test executable was not resolved from Cargo JSON artifacts'
    }
    return (Resolve-Path -LiteralPath $executable).Path
}
