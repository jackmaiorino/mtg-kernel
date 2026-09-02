<#
.SYNOPSIS
Test-time-search wrapper, stage S1 feasibility preflight
(LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md Section 5, S1). CP7-free.

.DESCRIPTION
Builds the frozen stratified decision corpus ONCE, then replays it through
the production ModelGuidedSearch selector for each tier of the ladder in
order, and writes a summary carrying the per-tier verdicts.

Nothing here searches, plays, or hashes anything itself: every unit of work
is a child process (tts_s1_corpus_v1, tts_s1_replay_v1) whose exit code this
wrapper captures and whose inputs it proves first. No CP7 panel is contacted
and no GPU is used; S1 is a CPU-only preflight.

The corpus is built once and reused by every tier, which is the whole point:
a tier comparison against different decision populations would measure the
populations, not the tiers. The corpus file is published immutably by the
corpus bin, so a second launch into the same attempt root fails closed
rather than silently redefining the population a report claims to have
measured.

A tier whose p99 decision wall time exceeds the pre-registered 4.0 s SLO, or
whose slowest decision reached the 20.0 s hard protocol timeout, is reported
INFEASIBLE by the replay bin, which exits 4. That is NOT a wrapper failure:
the ladder is measured in full, every tier gets a published report, and the
summary names which tiers are feasible. Zero feasible tiers is a legitimate
negative S1 result and still completes; only a real error (a missing input,
a crash, a failed publication) writes RUN_FAILED.

-DryRun validates every input, writes the provenance record, prints the
exact command line of every child it would run, and launches nothing.
-SkipHostAssertions additionally skips the git and toolchain assertions and
is accepted ONLY together with -DryRun, so a real launch can never quietly
skip them.

Each tier replays WHOLE EPISODES: every episode that contributes a corpus
target is reconstructed from its start and every decision in it is searched,
because the production diagnostics writer republishes the episode file after
every decision and a late decision's publication cost therefore depends on
the whole history behind it. A tier is consequently far more work than the
512-decision corpus suggests, which is why --max-episodes is passed as a
fail-closed guard.

FORMAL versus SMOKE. A run is FORMAL only when it replays the WHOLE frozen
corpus (no -LimitEpisodes) across the WHOLE pre-registered four-tier ladder,
and every tier's own report agrees that it replayed the whole corpus. Only a
formal run writes the TTS_S1_COMPLETE marker, and only a formal run may
close the ladder as a negative result when no tier is feasible. Anything
else is a SMOKE: it still runs, still publishes every report, and still
writes a summary, but its status is TTS_S1_SMOKE, it writes no marker, and
it says in as many words that it carries no feasibility standing. A smoke
that could leave behind the same marker a formal run does is how a partial
measurement gets read later as a finished one.

Terminal state: an empty TTS_S1_COMPLETE marker in the attempt root on a
successful FORMAL run, and a plain-text RUN_FAILED naming the failing step
on any error. A DRY RUN writes neither marker; it writes result.json with
status DRY_RUN_PLANNED, because a run that measured nothing may not leave
behind the file an operator reads as "this preflight finished".

PowerShell 5.1 compatible throughout: no `&&`, no ternary, no
null-coalescing, and no reliance on the host providing Get-FileHash.
#>
param(
    [Parameter(Mandatory = $true)][string]$EvidenceRoot,
    [Parameter(Mandatory = $true)][ValidateSet('original', 'population', 'portable')][string]$StoreKind,
    [Parameter(Mandatory = $true)][string]$StoreRoot,
    # Required for -StoreKind population. Optional for original (absent means
    # the pinned generation-384 authority). Rejected for portable.
    [Nullable[uint64]]$Generation,
    [Parameter(Mandatory = $true)][string]$CorpusExecutable,
    [Parameter(Mandatory = $true)][string]$ReplayExecutable,
    # Indices into MODEL_GUIDED_SEARCH_AUTHORIZED_SEED_BLOCKS_V1. Four blocks
    # are registered for S0 and S1 only; the corpus and the wrapper must not
    # share one, so the self-play draw and the search seeds have independent
    # sources.
    [Parameter(Mandatory = $true)][ValidateRange(0, 3)][int]$CorpusSeedBlock,
    [Parameter(Mandatory = $true)][ValidateRange(0, 3)][int]$ReplaySeedBlock,
    [Parameter(Mandatory = $true)][ValidateRange(1, 4096)][uint64]$Episodes,
    # The ladder, in order. The default is the whole pre-registered ladder
    # (sketch Section 4); a subset is admissible for a smoke, and the summary
    # records exactly which tiers ran.
    [ValidateSet('t512', 't2048', 't8192', 't32768')][string[]]$Tiers = @('t512', 't2048', 't8192', 't32768'),
    # Smoke bound passed through to every tier, in EPISODES. 0 means "every
    # contributing episode", which is the only configuration whose verdict a
    # panel may rely on. It is episodes rather than decisions because the
    # replay runs whole episodes: a decision bound would cut an episode in
    # half and leave its later decisions with a publication history no panel
    # would ever produce.
    [uint64]$LimitEpisodes = 0,
    [string]$RepoRoot,
    [switch]$DryRun,
    [switch]$SkipHostAssertions
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Reused verbatim for Get-ToolchainRecord (rustc, cargo, and the MSVC
# linker identity), which AGENTS.md process rule 1 requires in the run
# manifest. Dot-sourcing it defines functions and sets variables only; it
# launches nothing. The Get-FileHash shadow below is defined AFTER this, so
# every call in the whole stack resolves to the self-contained one.
. (Join-Path $PSScriptRoot '..\regularized_continuation_retest_v1\common.ps1')

$script:TtsS1Ladder = @('t512', 't2048', 't8192', 't32768')
$script:TtsS1SloSeconds = 4.0
$script:TtsS1HardTimeoutSeconds = 20.0

# ---------------------------------------------------------------------------
# Self-contained .NET SHA-256, following the cycle-4 launch stack's own
# precedent: a detached PowerShell host does not reliably have the
# Microsoft.PowerShell.Utility Get-FileHash cmdlet, and a formal run has
# already been lost to exactly that class of host-provided-cmdlet surprise.
# ---------------------------------------------------------------------------
function Get-FileHash {
    param(
        [Parameter(Mandatory = $true)][string]$Algorithm,
        [Parameter(Mandatory = $true)][string]$LiteralPath
    )
    if ($Algorithm -cne 'SHA256') { throw "unsupported hash algorithm: $Algorithm" }
    $resolved = (Resolve-Path -LiteralPath $LiteralPath).Path
    $stream = [System.IO.File]::OpenRead($resolved)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try { $digest = $sha.ComputeHash($stream) }
    finally {
        $sha.Dispose()
        $stream.Dispose()
    }
    return [pscustomobject]@{
        Algorithm = 'SHA256'
        Hash = ([System.BitConverter]::ToString($digest)).Replace('-', '')
        Path = $resolved
    }
}

function Get-TtsS1TextSha256 {
    param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Text)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try { $digest = $sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($Text)) }
    finally { $sha.Dispose() }
    return (([System.BitConverter]::ToString($digest)).Replace('-', '')).ToLowerInvariant()
}

function Get-TtsS1FileRecord {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "required file is missing: $Path"
    }
    $item = Get-Item -LiteralPath $Path
    return [ordered]@{
        path = $item.FullName
        bytes = [uint64]$item.Length
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $item.FullName).Hash.ToLowerInvariant()
    }
}

function Assert-TtsS1LastExitCode {
    param(
        [Parameter(Mandatory = $true)][AllowNull()][Nullable[int]]$ExitCode,
        [Parameter(Mandatory = $true)][string]$What
    )
    if ($null -eq $ExitCode -or $ExitCode -ne 0) {
        throw "$What failed with exit code $ExitCode"
    }
}

function Read-TtsS1Json {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "required JSON document is missing: $Path"
    }
    $text = [System.IO.File]::ReadAllText($Path, [System.Text.UTF8Encoding]::new($false))
    if ($text.Length -gt 0 -and [int][char]$text[0] -eq 65279) { $text = $text.Substring(1) }
    return $text | ConvertFrom-Json
}

function Write-TtsS1JsonFile {
    # Atomic publication by staged sibling then move, so a killed wrapper
    # never leaves a half-written record for the next process to read as
    # authoritative. The Rust side publishes its own artifacts through the
    # crate's durable primitives; this is only for the wrapper's own records.
    param(
        [Parameter(Mandatory = $true)]$Value,
        [Parameter(Mandatory = $true)][string]$Path
    )
    $directory = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($directory)) {
        New-Item -ItemType Directory -Force -Path $directory | Out-Null
    }
    $json = $Value | ConvertTo-Json -Depth 12
    $staged = "$Path.stage-$PID"
    [System.IO.File]::WriteAllText($staged, $json, [System.Text.UTF8Encoding]::new($false))
    try {
        Move-Item -LiteralPath $staged -Destination $Path -Force
    }
    catch {
        if (Test-Path -LiteralPath $staged) { Remove-Item -LiteralPath $staged -Force }
        throw
    }
}

function Get-TtsS1GitRecord {
    # Exact HEAD, a clean-worktree requirement, and hashes of the status and
    # diff so a later reviewer can prove the tree that ran.
    param([Parameter(Mandatory = $true)][string]$RepoRoot)
    $safe = $RepoRoot.Replace('\', '/')
    $status = @(& git -c "safe.directory=$safe" -C $RepoRoot status --porcelain 2>&1)
    Assert-TtsS1LastExitCode $LASTEXITCODE 'git status'
    $head = (@(& git -c "safe.directory=$safe" -C $RepoRoot rev-parse HEAD 2>&1) -join "`n").Trim()
    Assert-TtsS1LastExitCode $LASTEXITCODE 'git rev-parse'
    $branch = (@(& git -c "safe.directory=$safe" -C $RepoRoot rev-parse --abbrev-ref HEAD 2>&1) -join "`n").Trim()
    Assert-TtsS1LastExitCode $LASTEXITCODE 'git rev-parse --abbrev-ref'
    if ($status.Count -ne 0) {
        throw "an S1 launch requires a clean worktree at $RepoRoot"
    }
    $diff = @(& git -c "safe.directory=$safe" -C $RepoRoot diff --binary HEAD 2>&1)
    Assert-TtsS1LastExitCode $LASTEXITCODE 'git diff'
    return [ordered]@{
        repo_root = $RepoRoot
        commit = $head
        branch = $branch
        dirty = $false
        status_sha256 = Get-TtsS1TextSha256 (($status -join "`n"))
        worktree_diff_sha256 = Get-TtsS1TextSha256 (($diff -join "`n"))
    }
}

function Get-TtsS1ToolchainRecord {
    # AGENTS.md process rule 1: the run manifest records the toolchain
    # versions, the LINKER included. This REUSES the existing
    # Get-ToolchainRecord from the regularized_continuation_retest_v1 stack
    # (dot-sourced above, and what the cycle-4 launcher uses) rather than
    # recording a narrower subset of its own: rustc -Vv, cargo -Vv, and the
    # MSVC link.exe path, file version, banner and SHA-256.
    #
    # It fails closed. If vswhere.exe is absent or the linker cannot be
    # resolved, that helper throws, and this wrapper turns it into a
    # message that says which requirement was not met rather than letting a
    # launch proceed with an unpinned toolchain.
    try {
        return Get-ToolchainRecord
    }
    catch {
        throw ("an S1 launch must record the toolchain identity including the linker, and it could not be captured: " + $_.Exception.Message + ". Re-run with -DryRun -SkipHostAssertions to plan without it, or install the MSVC build tools so vswhere.exe can resolve link.exe.")
    }
}

function ConvertTo-TtsS1WindowsArgument {
    # Quotes ONE argument for the Windows command-line parser
    # (CommandLineToArgvW), which is what a Rust `std::env::args_os` reads.
    #
    # PowerShell 5.1 runs on .NET Framework, where ProcessStartInfo has no
    # ArgumentList: the only channel to a child is a single command-line
    # STRING, and Start-Process joins an array into one with plain spaces
    # and no quoting at all. A store root or evidence root containing a
    # space therefore reaches the child as two arguments, and the bin's
    # strict pair parser rejects the launch (or, worse, pairs the wrong
    # flag with the wrong value). So the quoting is done here, once, and
    # both the PLANNED command line and the executed one go through it, so
    # what a dry run prints is what a real launch runs.
    #
    # The rule is the documented MSVC one: backslashes are literal except
    # when they immediately precede a quote, where each must be doubled,
    # and a run of backslashes at the end of a quoted argument must be
    # doubled so it does not escape the closing quote.
    param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Value)
    if ($Value.Length -gt 0 -and $Value -notmatch '[\s"]') { return $Value }
    $builder = New-Object System.Text.StringBuilder
    [void]$builder.Append('"')
    $backslashes = 0
    foreach ($char in $Value.ToCharArray()) {
        if ($char -ceq '\') {
            $backslashes++
            continue
        }
        if ($char -ceq '"') {
            [void]$builder.Append('\' * (2 * $backslashes + 1))
            [void]$builder.Append('"')
            $backslashes = 0
            continue
        }
        if ($backslashes -gt 0) {
            [void]$builder.Append('\' * $backslashes)
            $backslashes = 0
        }
        [void]$builder.Append($char)
    }
    if ($backslashes -gt 0) { [void]$builder.Append('\' * (2 * $backslashes)) }
    [void]$builder.Append('"')
    return $builder.ToString()
}

function Format-TtsS1CommandLine {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][string[]]$Arguments
    )
    $quoted = @($Arguments | ForEach-Object { ConvertTo-TtsS1WindowsArgument -Value $_ })
    return (@(ConvertTo-TtsS1WindowsArgument -Value $FilePath) + $quoted) -join ' '
}

function Format-TtsS1ArgumentString {
    # The child's argument string alone, without the executable, which is
    # what Start-Process wants in -ArgumentList.
    param([Parameter(Mandatory = $true)][AllowEmptyCollection()][string[]]$Arguments)
    return (@($Arguments | ForEach-Object { ConvertTo-TtsS1WindowsArgument -Value $_ }) -join ' ')
}

function Invoke-TtsS1Process {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$StdoutPath,
        [Parameter(Mandatory = $true)][string]$StderrPath
    )
    foreach ($path in @($StdoutPath, $StderrPath)) {
        $directory = Split-Path -Parent $path
        if (-not [string]::IsNullOrWhiteSpace($directory)) {
            New-Item -ItemType Directory -Force -Path $directory | Out-Null
        }
        if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
    }
    # ONE already-quoted string, not an array: Start-Process joins an array
    # with unquoted spaces. See ConvertTo-TtsS1WindowsArgument.
    $argumentString = Format-TtsS1ArgumentString -Arguments $Arguments
    $process = Start-Process -FilePath $FilePath -ArgumentList $argumentString -NoNewWindow -PassThru `
        -RedirectStandardOutput $StdoutPath -RedirectStandardError $StderrPath
    # WaitForExit() then Refresh(), following the cycle-4 launcher: the
    # parameterless overload alone can return before ExitCode is populated.
    $process.WaitForExit()
    $process.Refresh()
    return [int]$process.ExitCode
}

function Write-TtsS1Marker {
    param(
        [Parameter(Mandatory = $true)][string]$AttemptRoot,
        [Parameter(Mandatory = $true)][string]$Name
    )
    [System.IO.File]::WriteAllText((Join-Path $AttemptRoot $Name), '', [System.Text.UTF8Encoding]::new($false))
}

function Write-TtsS1RunFailed {
    param(
        [Parameter(Mandatory = $true)][string]$AttemptRoot,
        [Parameter(Mandatory = $true)][string]$Step,
        [Parameter(Mandatory = $true)][string]$Detail
    )
    $text = "step=$Step`ndetail=$Detail`n"
    [System.IO.File]::WriteAllText((Join-Path $AttemptRoot 'RUN_FAILED'), $text, [System.Text.UTF8Encoding]::new($false))
}

# ---------------------------------------------------------------------------
# Input validation. Everything below runs in a dry run too.
# ---------------------------------------------------------------------------

if ($SkipHostAssertions -and -not $DryRun) {
    throw '-SkipHostAssertions is only accepted together with -DryRun; a real launch never skips the git and toolchain assertions'
}
if ([string]::IsNullOrWhiteSpace($RepoRoot)) {
    $RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
}
if ($CorpusSeedBlock -eq $ReplaySeedBlock) {
    throw '-CorpusSeedBlock and -ReplaySeedBlock must differ, so the self-play draw and the search seeds have independent sources'
}
if ($Tiers.Count -eq 0) { throw '-Tiers must name at least one tier' }
$seen = @{}
foreach ($tier in $Tiers) {
    if ($seen.ContainsKey($tier)) { throw "tier $tier is named more than once" }
    $seen[$tier] = $true
}
# Ladder ORDER is pre-registered; a caller may drop tiers but may not
# reorder them, because a launcher that ran t32768 first would spend the
# whole budget on the tier least likely to pass.
$expectedOrder = @($script:TtsS1Ladder | Where-Object { $seen.ContainsKey($_) })
if (($Tiers -join ',') -cne ($expectedOrder -join ',')) {
    throw "-Tiers must be a subsequence of the pre-registered ladder in order: $($script:TtsS1Ladder -join ', ')"
}
switch ($StoreKind) {
    'population' {
        if ($null -eq $Generation) { throw '-StoreKind population requires -Generation' }
    }
    'portable' {
        if ($null -ne $Generation) { throw '-StoreKind portable takes no -Generation' }
    }
}
foreach ($pair in @(@('CorpusExecutable', $CorpusExecutable), @('ReplayExecutable', $ReplayExecutable))) {
    if (-not (Test-Path -LiteralPath $pair[1] -PathType Leaf)) {
        throw "-$($pair[0]) is not a file: $($pair[1])"
    }
}
if (-not (Test-Path -LiteralPath $StoreRoot)) {
    throw "-StoreRoot does not exist: $StoreRoot"
}

New-Item -ItemType Directory -Force -Path $EvidenceRoot | Out-Null
$stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
$attemptRoot = Join-Path $EvidenceRoot ("tts-s1-{0}-{1}" -f $stamp, $PID)
if (Test-Path -LiteralPath $attemptRoot) { throw "attempt root already exists: $attemptRoot" }
New-Item -ItemType Directory -Force -Path $attemptRoot | Out-Null

$authorityArgs = @()
switch ($StoreKind) {
    'original' {
        $authorityArgs += @('--original-store-root', $StoreRoot)
        if ($null -ne $Generation) { $authorityArgs += @('--generation', [string]$Generation) }
    }
    'population' {
        $authorityArgs += @('--population-store-root', $StoreRoot, '--generation', [string]$Generation)
    }
    'portable' {
        $authorityArgs += @('--portable-derivative-root', $StoreRoot)
    }
}

$corpusPath = Join-Path $attemptRoot 'corpus.json'
$corpusArgs = $authorityArgs + @(
    '--seed-block', [string]$CorpusSeedBlock,
    '--episodes', [string]$Episodes,
    '--output', $corpusPath
)

$tierPlans = @()
foreach ($tier in $Tiers) {
    $reportPath = Join-Path $attemptRoot ("tier-{0}.report.json" -f $tier)
    # One diagnostics directory per tier. The production model-guided
    # diagnostics writer publishes this tier's V4 episode files there, and
    # the replay reads the protocol latency the SLO is classified on back
    # out of them; sharing one directory across tiers would mix two tiers'
    # episode files under the same names.
    $diagnosticsDir = Join-Path $attemptRoot ("tier-{0}.diagnostics" -f $tier)
    $replayArgs = $authorityArgs + @(
        '--corpus', $corpusPath,
        '--tier', $tier,
        '--seed-block', [string]$ReplaySeedBlock,
        '--diagnostics-dir', $diagnosticsDir,
        # The guard is the corpus's own episode count, which this launcher
        # is the one that chose. Contributing episodes can only be a subset
        # of the episodes played, so this is a true upper bound, and a
        # corpus built by someone else with more episodes is refused rather
        # than run for days.
        '--max-episodes', [string]$Episodes,
        '--output', $reportPath
    )
    if ($LimitEpisodes -gt 0) {
        $replayArgs += @('--limit-episodes', [string]$LimitEpisodes)
    }
    $tierPlans += [pscustomobject]@{
        tier = $tier
        report_path = $reportPath
        diagnostics_dir = $diagnosticsDir
        arguments = $replayArgs
        command_line = Format-TtsS1CommandLine -FilePath $ReplayExecutable -Arguments $replayArgs
    }
}

# FORMAL versus SMOKE, decided from the inputs before anything runs. A
# formal run is the whole corpus across the whole pre-registered ladder;
# every tier's own report must additionally agree that it replayed the whole
# corpus, which is checked per tier below.
$isFormalLadder = ($LimitEpisodes -eq 0) -and ($Tiers.Count -eq $script:TtsS1Ladder.Count)

$gitRecord = $null
$toolchainRecord = $null
if (-not $SkipHostAssertions) {
    $gitRecord = Get-TtsS1GitRecord -RepoRoot $RepoRoot
    $toolchainRecord = Get-TtsS1ToolchainRecord
}

$provenance = [ordered]@{
    schema = 'mtg-kernel-tts-s1-launch-provenance/v1'
    stage = 'S1'
    design_sketch = 'LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md'
    attempt_root = $attemptRoot
    utc = $stamp
    dry_run = [bool]$DryRun
    host_assertions_skipped = [bool]$SkipHostAssertions
    git = $gitRecord
    toolchain = $toolchainRecord
    corpus_executable = Get-TtsS1FileRecord -Path $CorpusExecutable
    replay_executable = Get-TtsS1FileRecord -Path $ReplayExecutable
    store_kind = $StoreKind
    store_root = $StoreRoot
    generation = $Generation
    corpus_seed_block = $CorpusSeedBlock
    replay_seed_block = $ReplaySeedBlock
    episodes = $Episodes
    tiers = $Tiers
    limit_episodes = $LimitEpisodes
    max_episodes = $Episodes
    formal_ladder = $isFormalLadder
    slo_seconds = $script:TtsS1SloSeconds
    hard_timeout_seconds = $script:TtsS1HardTimeoutSeconds
    planned_corpus_command = Format-TtsS1CommandLine -FilePath $CorpusExecutable -Arguments $corpusArgs
    planned_tier_commands = @($tierPlans | ForEach-Object { $_.command_line })
}
Write-TtsS1JsonFile -Value $provenance -Path (Join-Path $attemptRoot 'provenance.json')

if ($DryRun) {
    Write-Output "DRY RUN attempt_root=$attemptRoot"
    Write-Output $provenance.planned_corpus_command
    foreach ($line in $provenance.planned_tier_commands) { Write-Output $line }
    Write-TtsS1JsonFile -Value ([ordered]@{
        schema = 'mtg-kernel-tts-s1-summary/v1'
        status = 'DRY_RUN_PLANNED'
        attempt_root = $attemptRoot
        tiers = $Tiers
        planned_corpus_command = $provenance.planned_corpus_command
        planned_tier_commands = $provenance.planned_tier_commands
    }) -Path (Join-Path $attemptRoot 'result.json')
    exit 0
}

# ---------------------------------------------------------------------------
# 1. Build the corpus ONCE.
# ---------------------------------------------------------------------------
try {
    Write-Output "TTS_S1_STEP corpus $($provenance.planned_corpus_command)"
    $exitCode = Invoke-TtsS1Process -FilePath $CorpusExecutable -Arguments $corpusArgs `
        -StdoutPath (Join-Path $attemptRoot 'corpus.stdout.txt') `
        -StderrPath (Join-Path $attemptRoot 'corpus.stderr.txt')
    if ($exitCode -ne 0) {
        throw "tts_s1_corpus_v1 exited with $exitCode; see corpus.stderr.txt"
    }
    $corpus = Read-TtsS1Json -Path $corpusPath
    $corpusRecord = Get-TtsS1FileRecord -Path $corpusPath
    Write-Output "TTS_S1_CORPUS corpus_sha256=$($corpus.corpus_sha256) decisions=$(@($corpus.body.decisions).Count) contributing_episodes=$(@($corpus.body.episodes).Count) natural_episodes=$($corpus.body.natural_terminal_episode_count) truncated_episodes=$($corpus.body.truncated_episode_count)"
}
catch {
    Write-TtsS1RunFailed -AttemptRoot $attemptRoot -Step 'corpus' -Detail $_.Exception.Message
    Write-Error $_.Exception.Message
    exit 1
}

# ---------------------------------------------------------------------------
# 2. Replay every tier, in ladder order. An INFEASIBLE tier (exit 4) is a
#    recorded verdict, not a wrapper failure: the ladder is measured in full.
# ---------------------------------------------------------------------------
$tierResults = @()
foreach ($plan in $tierPlans) {
    try {
        Write-Output "TTS_S1_STEP tier-$($plan.tier) $($plan.command_line)"
        $exitCode = Invoke-TtsS1Process -FilePath $ReplayExecutable -Arguments $plan.arguments `
            -StdoutPath (Join-Path $attemptRoot ("tier-{0}.stdout.txt" -f $plan.tier)) `
            -StderrPath (Join-Path $attemptRoot ("tier-{0}.stderr.txt" -f $plan.tier))
        if ($exitCode -ne 0 -and $exitCode -ne 4) {
            throw "tts_s1_replay_v1 exited with $exitCode for tier $($plan.tier); see tier-$($plan.tier).stderr.txt"
        }
        $report = Read-TtsS1Json -Path $plan.report_path
        if ($report.body.corpus_sha256 -cne $corpus.corpus_sha256) {
            throw "tier $($plan.tier) measured corpus $($report.body.corpus_sha256), not the corpus this attempt built"
        }
        $expectedVerdict = 'FEASIBLE'
        if ($exitCode -eq 4) { $expectedVerdict = 'INFEASIBLE' }
        $observedVerdict = $report.body.verdict.ToUpperInvariant()
        if ($observedVerdict -cne $expectedVerdict) {
            throw "tier $($plan.tier) exited $exitCode but its report says $observedVerdict"
        }
        $tierResults += [ordered]@{
            tier = $plan.tier
            verdict = $observedVerdict
            verdict_reason = $report.body.verdict_reason
            exit_code = $exitCode
            report = Get-TtsS1FileRecord -Path $plan.report_path
            report_sha256_self = $report.report_sha256
            diagnostics_dir = $plan.diagnostics_dir
            episodes_replayed = $report.body.episodes_replayed
            searched_decisions = $report.body.searched_decisions
            corpus_targets_replayed = $report.body.corpus_targets_replayed
            replayed_whole_corpus = $report.body.replayed_whole_corpus
            # The verdict basis: every decision searched.
            protocol_p50_micros = $report.body.whole_episode_view.protocol_wall_time.p50_micros
            protocol_p99_micros = $report.body.whole_episode_view.protocol_wall_time.p99_micros
            protocol_max_micros = $report.body.whole_episode_view.protocol_wall_time.max_micros
            mean_protocol_micros = $report.body.whole_episode_view.mean_protocol_micros
            search_p50_micros = $report.body.whole_episode_view.search_wall_time.p50_micros
            search_p99_micros = $report.body.whole_episode_view.search_wall_time.p99_micros
            search_max_micros = $report.body.whole_episode_view.search_wall_time.max_micros
            decisions_per_second_milli = $report.body.whole_episode_view.decisions_per_second_milli
            # The stratified targets alone, for the strata diagnostics.
            target_protocol_p50_micros = $report.body.corpus_target_view.protocol_wall_time.p50_micros
            target_protocol_p99_micros = $report.body.corpus_target_view.protocol_wall_time.p99_micros
            target_protocol_max_micros = $report.body.corpus_target_view.protocol_wall_time.max_micros
            projected_s2_worker_hours_milli = $report.body.compute_cap.projected_worker_hours_milli
            projected_elapsed_hours_at_workers_milli = $report.body.compute_cap.projected_elapsed_hours_at_workers_milli
            compute_cap_worker_hours_milli = $report.body.compute_cap.cap_worker_hours_milli
            within_compute_cap = $report.body.compute_cap.within_cap
            search_authority_digest_sha256 = $report.body.search_authority_digest_sha256
        }
        Write-Output ("TTS_S1_TIER tier={0} verdict={1} episodes={2} searched_decisions={3} protocol_p99_micros={4} protocol_max_micros={5} decisions_per_second_milli={6} projected_s2_worker_hours_milli={7} within_compute_cap={8}" -f `
            $plan.tier, $observedVerdict, $report.body.episodes_replayed, $report.body.searched_decisions, `
            $report.body.whole_episode_view.protocol_wall_time.p99_micros, `
            $report.body.whole_episode_view.protocol_wall_time.max_micros, `
            $report.body.whole_episode_view.decisions_per_second_milli, `
            $report.body.compute_cap.projected_worker_hours_milli, $report.body.compute_cap.within_cap)
    }
    catch {
        Write-TtsS1RunFailed -AttemptRoot $attemptRoot -Step "tier-$($plan.tier)" -Detail $_.Exception.Message
        Write-Error $_.Exception.Message
        exit 1
    }
}

$feasible = @($tierResults | Where-Object { $_.verdict -ceq 'FEASIBLE' } | ForEach-Object { $_.tier })
# The reports get the last word on whether this was a whole-corpus run: a
# tier that replayed less than the whole corpus makes the run a smoke even
# if the flags said otherwise.
$everyTierWholeCorpus = $true
foreach ($result in $tierResults) {
    if (-not $result.replayed_whole_corpus) { $everyTierWholeCorpus = $false }
}
$isFormal = $isFormalLadder -and $everyTierWholeCorpus
$status = 'TTS_S1_SMOKE'
if ($isFormal) { $status = 'TTS_S1_COMPLETE' }
$summary = [ordered]@{
    schema = 'mtg-kernel-tts-s1-summary/v1'
    status = $status
    formal_ladder = $isFormal
    stage = 'S1'
    attempt_root = $attemptRoot
    utc = $stamp
    git = $gitRecord
    toolchain = $toolchainRecord
    corpus_executable = $provenance.corpus_executable
    replay_executable = $provenance.replay_executable
    corpus = $corpusRecord
    corpus_sha256_self = $corpus.corpus_sha256
    corpus_decision_count = @($corpus.body.decisions).Count
    corpus_candidate_count = $corpus.body.candidate_count
    corpus_seed_block = $CorpusSeedBlock
    replay_seed_block = $ReplaySeedBlock
    episodes = $Episodes
    limit_episodes = $LimitEpisodes
    max_episodes = $Episodes
    corpus_episode_count = @($corpus.body.episodes).Count
    corpus_all_episode_mean_decisions_milli = $corpus.body.all_episode_decisions.mean_decisions_milli
    slo_seconds = $script:TtsS1SloSeconds
    hard_timeout_seconds = $script:TtsS1HardTimeoutSeconds
    ladder = $script:TtsS1Ladder
    tiers = @($tierResults)
    feasible_tiers = $feasible
    feasible_tier_count = $feasible.Count
}
Write-TtsS1JsonFile -Value $summary -Path (Join-Path $attemptRoot 'summary.json')
# ONLY a formal run leaves the marker an operator reads as "this preflight
# finished". A smoke that could leave the same marker is how a partial
# measurement gets read later as a finished one.
if ($isFormal) {
    Write-TtsS1Marker -AttemptRoot $attemptRoot -Name 'TTS_S1_COMPLETE'
}

Write-Output "TTS_S1_SUMMARY attempt_root=$attemptRoot status=$status formal_ladder=$isFormal feasible_tier_count=$($feasible.Count) feasible_tiers=$($feasible -join ',')"
if (-not $isFormal) {
    Write-Output 'TTS_S1_RESULT this run is a SMOKE (a partial corpus or a partial ladder); it carries no feasibility standing, no TTS_S1_COMPLETE marker was written, and it may not be read as closing the ladder either way'
}
elseif ($feasible.Count -eq 0) {
    # A legitimate negative S1 result, loudly stated, and only reachable
    # from a formal run. Not an error: the sketch's own rule is that a tier
    # failing the SLO or the compute cap is dropped, and every tier being
    # dropped closes the ladder as a negative, which is a finding, not a
    # crash.
    Write-Output 'TTS_S1_RESULT no tier is feasible under the pre-registered SLO and compute cap; the ladder closes as a negative S1 result'
}
exit 0
