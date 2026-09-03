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
exact command line of every child it would run, and launches nothing. The
shard commands are printed at the REQUESTED -ShardCount, which is the only
count knowable before the corpus exists; a real launch can only run fewer,
and says so when it does.
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

SHARDING. -ShardCount K runs K replay processes per tier CONCURRENTLY, each
owning the contributing episodes whose position in the corpus's episode
order is its index modulo K, and then runs one merge process that publishes
the tier report. The split is execution only: the merge recomputes every
statistic over the union through the same code the unsharded replay
finalizes through, so the tier report is the one a single process would have
published. Every tier goes through the shard-and-merge path, K = 1 included,
so there is one code path here rather than two that could drift.

The effective per-tier shard count is min(-ShardCount, the planned
contributing episodes), because a shard owning no episode is refused by the
replay bin rather than published empty. The corpus manifest states its
contributing episode population, so both the clamp and the expected per-tier
cost are known before the first search runs and are printed.

FORMAL versus SMOKE. A run is FORMAL only when it replays the WHOLE frozen
corpus (no -LimitEpisodes) across the WHOLE pre-registered four-tier ladder
AT THE PINNED SHARD TOPOLOGY, and every tier's own report agrees that it
replayed the whole corpus and was measured at that topology. Only a formal
run writes the TTS_S1_COMPLETE marker, and only a formal run may close the
ladder as a negative result when no tier is feasible. Anything else is a
SMOKE: it still runs, still publishes every report, and still writes a
summary, but its status is TTS_S1_SMOKE, it writes no marker, and it says in
as many words that it carries no feasibility standing. A smoke that could
leave behind the same marker a formal run does is how a partial measurement
gets read later as a finished one.

THE PINNED TOPOLOGY is eight concurrent replay processes on a host with at
least two logical CPUs per shard. It is a pin and not a knob because every
latency in a tier report is a wall-time sample taken under whatever
contention the fan-out created: the p99 SLO clause, the hard-timeout clause
and the isotonic curve the compute-cap projection is fitted to are all such
samples, so -ShardCount would otherwise be a knob that can flip a verdict.
Eight, because the CP7 panel host runs the wrapped agent under eight
concurrent games. A run at any other count is a SMOKE and never formal, and
a launch that asks for a formal ladder on a host too small for the topology
is REFUSED before it starts rather than spending hours to publish a smoke
nobody asked for.

AND THE PIN IS NOT TAKEN ON TRUST. A declared count of eight is satisfied
just as well by eight processes run one after another, so the run has to
prove the contention it claims. Each shard announces itself READY once its
checkpoint is loaded and before it waits; a barrier-publisher process (the
replay bin's --publish-start-barrier mode) waits for every announcement on a
bounded deadline and only then stamps the tier's one start token, and a shard
that never announces fails the tier closed after every started child has been
reaped. The token is stamped by that process and not by this launcher on
purpose: its instant is compared, exactly and with no tolerance, against
instants the shards recorded through the crate's own clock, and this
runtime's clock advances on a coarser cadence, so a second clock in that
comparison could invert it. Each shard then records the wall-clock window
of every decision on a shared time base, and the merge censuses how many
shards were mid-work when each decision began. Formal standing needs the
count, the host, the handshake and at least 950 permille of decisions
searched with every other shard mid-work; the merged report publishes all of
it, and this launcher reads the report rather than its own flags.

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
    # How many replay processes run CONCURRENTLY per tier. The tier report
    # is identical for every value of this: it selects how the episodes are
    # divided between processes and nothing else. Clamped down to the
    # planned contributing episode count, because a shard owning no episode
    # is refused rather than published empty.
    [ValidateRange(1, 64)][int]$ShardCount = 8,
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

# The pinned report contract (the compute-cap rule, the NESTED latency-curve
# rule, and the gating view) plus the validation that enforces it, and the
# shared child-process primitives the shard fan-out is built on. Kept in a
# file of its own so this launcher and the dry-run tests exercise the same
# constants, the same check and the same process code, rather than one
# asserting what the other merely describes.
. (Join-Path $PSScriptRoot 'common.ps1')

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

# FORMAL versus SMOKE, decided from the inputs BEFORE the attempt root
# exists. A formal run is the whole corpus across the whole pre-registered
# ladder AT THE PINNED CONCURRENCY; every tier's own report must
# additionally agree that it replayed the whole corpus and that it was
# measured at that topology, which is checked per tier below.
#
# The shard count is part of this and not a free knob: every latency in a
# tier report is a wall-time sample taken under whatever contention the
# fan-out created, so a run at another count measured a different machine.
# See $script:TtsS1FormalShardCount.
$isFormalLadder = ($LimitEpisodes -eq 0) -and ($Tiers.Count -eq $script:TtsS1Ladder.Count) `
    -and ($ShardCount -eq $script:TtsS1FormalShardCount)

# The host, read once, read-only. The tier reports carry the measuring
# processes' own reading and are the authority for a finished run; this is
# what lets a launch that could never be formal say so before it starts.
$hostLogicalCpus = [System.Environment]::ProcessorCount
$formalTopology = Test-TtsS1FormalShardTopology -ShardCount $ShardCount -HostLogicalCpus $hostLogicalCpus
if ($isFormalLadder -and -not $formalTopology.admissible) {
    # REFUSED, not demoted, and refused HERE, with the other input checks,
    # before any directory is created. A throw after the attempt root
    # exists would leave an empty directory carrying neither a summary nor
    # a RUN_FAILED: nonterminal, and indistinguishable from a run that was
    # killed. Every throw past this point writes RUN_FAILED first.
    #
    # An operator who asked for the whole ladder over the whole corpus at
    # the pinned concurrency is asking for a formal result, and this host
    # cannot produce one; spending hours to publish a smoke they did not
    # ask for is worse than saying so now. A smoke is still available at
    # any other -ShardCount.
    $message = "a formal S1 run is refused on this host: $($formalTopology.reason). Re-run with a different -ShardCount for a SMOKE, or use a host with at least $($formalTopology.required_logical_cpus) logical CPUs"
    if ($DryRun) {
        # A dry run measures nothing, so it PLANS and says what a real
        # launch would refuse, rather than refusing to plan.
        Write-Output "TTS_S1_FORMAL_TOPOLOGY_REFUSED $message"
    }
    else {
        throw $message
    }
}

New-Item -ItemType Directory -Force -Path $EvidenceRoot | Out-Null
$stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
$attemptRoot = Join-Path $EvidenceRoot ("tts-s1-{0}-{1}" -f $stamp, $PID)
# This one throw is still bare, and has to be: it fires when the attempt
# root ALREADY EXISTS, and writing a RUN_FAILED into it would overwrite the
# terminal state of whatever run owns it.
if (Test-Path -LiteralPath $attemptRoot) { throw "attempt root already exists: $attemptRoot" }
New-Item -ItemType Directory -Force -Path $attemptRoot | Out-Null

# ---------------------------------------------------------------------------
# FROM HERE THE ATTEMPT ROOT EXISTS, so every failure has to leave a terminal
# state in it. An empty attempt directory carrying neither a summary nor a
# RUN_FAILED is nonterminal: an operator reading it later cannot tell it from
# a run that was killed, and the whole point of the marker discipline is that
# they never have to guess.
#
# The planning steps (the authority shape, the git and toolchain records, the
# provenance write, the dry-run result) are wrapped here; the corpus, sizing
# and tier steps have their own catches below, each naming its own step.
# ---------------------------------------------------------------------------
try {
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

function New-TtsS1TierPlan {
    # Every child one tier needs: K shard invocations and the merge that
    # turns their reports into the tier report.
    #
    # It is a function because the shard count is known TWICE and can
    # differ: once at planning time, from -ShardCount, for the provenance
    # record and the dry run; and once after the corpus exists, clamped to
    # the planned contributing episodes. Building the plan by two separate
    # pieces of code is how the printed command and the executed one drift.
    param(
        [Parameter(Mandatory = $true)][string]$Tier,
        [Parameter(Mandatory = $true)][int]$TierShardCount
    )
    $reportPath = Join-Path $attemptRoot ("tier-{0}.report.json" -f $Tier)
    # One diagnostics tree per tier, and one directory per SHARD inside it.
    # The production model-guided diagnostics writer publishes this tier's
    # V4 episode files there, and the replay reads the protocol latency the
    # SLO is classified on back out of them; sharing one directory across
    # tiers would mix two tiers' episode files under the same names, and
    # sharing one across shards would have two processes writing the same
    # scorer-shaped response file.
    $diagnosticsRoot = Join-Path $attemptRoot ("tier-{0}.diagnostics" -f $Tier)
    $shardRoot = Join-Path $attemptRoot ("tier-{0}.shards" -f $Tier)
    # THE START BARRIER for this tier. Every shard waits on this one token
    # and none of them searches before it appears, so no shard measures the
    # machine before its siblings exist. The launcher publishes it only once
    # every shard process has started; see the fan-out's -AfterStart hook.
    $barrierPath = Join-Path $shardRoot 'start-barrier.token'
    $shards = @()
    for ($index = 0; $index -lt $TierShardCount; $index++) {
        # The same name the merge derives on the Rust side from the index
        # and the count; see tts_s1_shard_report_file_name_v1.
        $shardReportPath = Join-Path $shardRoot ("shard-{0:0000}-of-{1:0000}.report.json" -f $index, $TierShardCount)
        $shardDiagnostics = Join-Path $diagnosticsRoot ("shard-{0:0000}" -f $index)
        $shardArgs = $authorityArgs + @(
            '--corpus', $corpusPath,
            '--tier', $Tier,
            '--seed-block', [string]$ReplaySeedBlock,
            '--diagnostics-dir', $shardDiagnostics,
            # The guard is the corpus's own episode count, which this
            # launcher is the one that chose. Contributing episodes can
            # only be a subset of the episodes played, so this is a true
            # upper bound, and a corpus built by someone else with more
            # episodes is refused rather than run for days. It is the WHOLE
            # run's bound, not the shard's: the guard is about the corpus.
            '--max-episodes', [string]$Episodes,
            '--output', $shardReportPath,
            '--shard-index', [string]$index,
            '--shard-count', [string]$TierShardCount,
            '--start-barrier', $barrierPath,
            '--start-barrier-timeout-seconds', [string]$script:TtsS1StartBarrierTimeoutSeconds
        )
        if ($LimitEpisodes -gt 0) {
            $shardArgs += @('--limit-episodes', [string]$LimitEpisodes)
        }
        $shards += [pscustomobject]@{
            index = $index
            report_path = $shardReportPath
            diagnostics_dir = $shardDiagnostics
            stdout_path = Join-Path $attemptRoot ("tier-{0}.shard-{1:0000}.stdout.txt" -f $Tier, $index)
            stderr_path = Join-Path $attemptRoot ("tier-{0}.shard-{1:0000}.stderr.txt" -f $Tier, $index)
            arguments = $shardArgs
            command_line = Format-TtsS1CommandLine -FilePath $ReplayExecutable -Arguments $shardArgs
        }
    }
    $publishArgs = @(
        '--publish-start-barrier', $barrierPath,
        '--barrier-dir', $shardRoot,
        '--shard-count', [string]$TierShardCount,
        '--readiness-timeout-seconds', [string]$script:TtsS1ShardReadyTimeoutSeconds
    )
    $mergeArgs = @(
        '--merge-shards', $shardRoot,
        '--shard-count', [string]$TierShardCount,
        '--output', $reportPath
    )
    return [pscustomobject]@{
        tier = $Tier
        shard_count = $TierShardCount
        report_path = $reportPath
        shard_root = $shardRoot
        barrier_path = $barrierPath
        barrier_timeout_seconds = $script:TtsS1StartBarrierTimeoutSeconds
        ready_timeout_seconds = $script:TtsS1ShardReadyTimeoutSeconds
        publish_arguments = $publishArgs
        publish_command_line = Format-TtsS1CommandLine -FilePath $ReplayExecutable -Arguments $publishArgs
        publish_stdout_path = Join-Path $attemptRoot ("tier-{0}.barrier.stdout.txt" -f $Tier)
        publish_stderr_path = Join-Path $attemptRoot ("tier-{0}.barrier.stderr.txt" -f $Tier)
        diagnostics_root = $diagnosticsRoot
        shards = $shards
        merge_arguments = $mergeArgs
        merge_command_line = Format-TtsS1CommandLine -FilePath $ReplayExecutable -Arguments $mergeArgs
        merge_stdout_path = Join-Path $attemptRoot ("tier-{0}.merge.stdout.txt" -f $Tier)
        merge_stderr_path = Join-Path $attemptRoot ("tier-{0}.merge.stderr.txt" -f $Tier)
    }
}

# Planned at the REQUESTED shard count. The effective count is settled
# after the corpus exists and can only be smaller; see the clamp below.
$plannedTierPlans = @($Tiers | ForEach-Object { New-TtsS1TierPlan -Tier $_ -TierShardCount $ShardCount })

$gitRecord = $null
$toolchainRecord = $null
if (-not $SkipHostAssertions) {
    $gitRecord = Get-TtsS1GitRecord -RepoRoot $RepoRoot
    $toolchainRecord = Get-TtsS1ToolchainRecord
}

# Hashed ONCE and reused by every shard record below, so the K shard
# invocations provably name the same binary rather than K separately hashed
# ones that a reader would have to compare by eye.
$replayExecutableRecord = Get-TtsS1FileRecord -Path $ReplayExecutable

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
    replay_executable = $replayExecutableRecord
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
    shard_count_requested = $ShardCount
    # The effective per-tier count is settled once the corpus states its
    # contributing episode population, and can only be smaller. The
    # commands below are stated at the REQUESTED count, so a dry run at a
    # count the corpus cannot supply prints more shard commands than a real
    # launch would run; nothing else about them changes.
    shard_count_rule = 'effective-per-tier-shard-count-is-min-of-requested-and-planned-contributing-episodes/v1'
    shard_assignment_rule = $script:TtsS1ShardAssignmentRule
    formal_shard_count = $script:TtsS1FormalShardCount
    formal_logical_cpus_per_shard = $script:TtsS1FormalLogicalCpusPerShard
    formal_min_full_concurrency_permille = $script:TtsS1FormalMinFullConcurrencyPermille
    shard_ready_timeout_seconds = $script:TtsS1ShardReadyTimeoutSeconds
    start_barrier_timeout_seconds = $script:TtsS1StartBarrierTimeoutSeconds
    host_logical_cpus = $hostLogicalCpus
    formal_topology_admissible = $formalTopology.admissible
    formal_topology_reason = $formalTopology.reason
    formal_topology_required_logical_cpus = $formalTopology.required_logical_cpus
    slo_seconds = $script:TtsS1SloSeconds
    hard_timeout_seconds = $script:TtsS1HardTimeoutSeconds
    pinned_contract = Get-TtsS1PinnedContract
    planned_corpus_command = Format-TtsS1CommandLine -FilePath $CorpusExecutable -Arguments $corpusArgs
    planned_tier_shard_commands = @($plannedTierPlans | ForEach-Object { $_.shards } | ForEach-Object { $_.command_line })
    planned_tier_merge_commands = @($plannedTierPlans | ForEach-Object { $_.merge_command_line })
    # The barrier publisher, one per tier: it is the process that waits for
    # every shard's announcement and stamps the token, so its invocation is
    # part of what a reviewer has to be able to see.
    planned_tier_barrier_commands = @($plannedTierPlans | ForEach-Object { $_.publish_command_line })
    # Every shard's invocation as a record of its own, each naming the bin
    # it runs and that bin's hash, so a reviewer reading the provenance
    # alone can see exactly which binary each of the K processes was to be,
    # rather than inferring it from a flat list of command lines.
    planned_tier_shards = @($plannedTierPlans | ForEach-Object {
        $tierPlan = $_
        $tierPlan.shards | ForEach-Object {
            [ordered]@{
                tier = $tierPlan.tier
                shard_index = $_.index
                shard_count = $tierPlan.shard_count
                executable = $replayExecutableRecord.path
                executable_sha256 = $replayExecutableRecord.sha256
                command_line = $_.command_line
                report_path = $_.report_path
                diagnostics_dir = $_.diagnostics_dir
            }
        }
    })
}
Write-TtsS1JsonFile -Value $provenance -Path (Join-Path $attemptRoot 'provenance.json')

if ($DryRun) {
    Write-Output "DRY RUN attempt_root=$attemptRoot shard_count_requested=$ShardCount"
    Write-Output $provenance.planned_corpus_command
    foreach ($line in $provenance.planned_tier_shard_commands) { Write-Output $line }
    foreach ($line in $provenance.planned_tier_barrier_commands) { Write-Output $line }
    foreach ($line in $provenance.planned_tier_merge_commands) { Write-Output $line }
    Write-TtsS1JsonFile -Value ([ordered]@{
        schema = 'mtg-kernel-tts-s1-summary/v1'
        status = 'DRY_RUN_PLANNED'
        attempt_root = $attemptRoot
        tiers = $Tiers
        shard_count_requested = $ShardCount
        planned_corpus_command = $provenance.planned_corpus_command
        planned_tier_shard_commands = $provenance.planned_tier_shard_commands
        planned_tier_merge_commands = $provenance.planned_tier_merge_commands
        planned_tier_barrier_commands = $provenance.planned_tier_barrier_commands
    }) -Path (Join-Path $attemptRoot 'result.json')
    exit 0
}
}
catch {
    Write-TtsS1RunFailed -AttemptRoot $attemptRoot -Step 'provenance' -Detail $_.Exception.Message
    Write-Error $_.Exception.Message
    exit 1
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
    Write-Output "TTS_S1_CORPUS corpus_sha256=$($corpus.corpus_sha256) decisions=$(@($corpus.body.decisions).Count) contributing_episodes=$($corpus.body.contributing_episode_count) contributing_episode_decisions=$($corpus.body.contributing_episode_decisions) natural_episodes=$($corpus.body.natural_terminal_episode_count) truncated_episodes=$($corpus.body.truncated_episode_count)"
    if ([uint64]$corpus.body.contributing_episode_count -ne [uint64]@($corpus.body.episodes).Count) {
        throw "the corpus states $($corpus.body.contributing_episode_count) contributing episodes but carries $(@($corpus.body.episodes).Count)"
    }
}
catch {
    Write-TtsS1RunFailed -AttemptRoot $attemptRoot -Step 'corpus' -Detail $_.Exception.Message
    Write-Error $_.Exception.Message
    exit 1
}

# ---------------------------------------------------------------------------
# 1b. Size the run BEFORE the first search, from the corpus's own stated
#     contributing population.
#
#     THE COST IS PRINTED IN DECISIONS, not in seconds, and deliberately: a
#     per-decision second count is a property of the host and the tier, not
#     of this launcher, and printing an invented one would be worse than
#     printing none. What is published is the number of decision searches
#     each tier owes and the same number weighted by the tier's transition
#     budget relative to t512, which is the ratio the work actually scales
#     by, plus the SLOWEST shard's share, which is what elapsed time
#     tracks once K processes run at once. An operator multiplies by a
#     measured per-decision cost.
# ---------------------------------------------------------------------------
$tierCostEstimates = @()
$tierPlans = @()
$effectiveShardCount = $ShardCount
$effectiveTopology = $formalTopology
$isFormalTopology = $false
$plannedEpisodeCount = 0
$plannedDecisions = [uint64]0
$maxShardEpisodes = [uint64]0
$maxShardDecisions = [uint64]0
try {
    $episodeDecisionCounts = @($corpus.body.episodes | ForEach-Object { [uint64]$_.decision_count })
    $plannedEpisodeCount = $episodeDecisionCounts.Count
    if ($LimitEpisodes -gt 0 -and [uint64]$LimitEpisodes -lt [uint64]$plannedEpisodeCount) {
        $plannedEpisodeCount = [int]$LimitEpisodes
    }
    if ($plannedEpisodeCount -lt 1) {
        throw 'the corpus contributes no episode to replay'
    }
    $plannedEpisodeDecisions = @($episodeDecisionCounts | Select-Object -First $plannedEpisodeCount)
    foreach ($count in $plannedEpisodeDecisions) { $plannedDecisions += [uint64]$count }
    # A shard owning no episode is refused by the replay bin, so the fan-out
    # is clamped to what the corpus can supply rather than failing at the
    # first shard. Stated loudly: a run that silently used one process where
    # eight were asked for would look like a hung tier.
    if ($effectiveShardCount -gt $plannedEpisodeCount) { $effectiveShardCount = $plannedEpisodeCount }
    $shardEpisodeCounts = New-Object 'uint64[]' $effectiveShardCount
    $shardDecisionCounts = New-Object 'uint64[]' $effectiveShardCount
    for ($position = 0; $position -lt $plannedEpisodeDecisions.Count; $position++) {
        $slot = $position % $effectiveShardCount
        $shardEpisodeCounts[$slot] += 1
        $shardDecisionCounts[$slot] += [uint64]$plannedEpisodeDecisions[$position]
    }
    $maxShardEpisodes = ($shardEpisodeCounts | Measure-Object -Maximum).Maximum
    $maxShardDecisions = ($shardDecisionCounts | Measure-Object -Maximum).Maximum
    Write-Output ("TTS_S1_SHARDS requested={0} effective={1} planned_episodes={2} planned_decisions={3} max_shard_episodes={4} max_shard_decisions={5} assignment={6}" -f `
        $ShardCount, $effectiveShardCount, $plannedEpisodeCount, $plannedDecisions, `
        $maxShardEpisodes, $maxShardDecisions, $script:TtsS1ShardAssignmentRule)
    if ($effectiveShardCount -ne $ShardCount) {
        Write-Output ("TTS_S1_SHARDS_CLAMPED the corpus contributes {0} planned episodes, so the fan-out is {1} and not the {2} requested; a shard owning no episode is refused rather than published empty" -f `
            $plannedEpisodeCount, $effectiveShardCount, $ShardCount)
    }
    foreach ($tier in $Tiers) {
        # The tier's transition budget is the number in its own
        # pre-registered tag, so this is read off the ladder rather than
        # being a second copy of the ladder's constants.
        $tierBudget = [uint64]$tier.Substring(1)
        $weight = $tierBudget / [uint64]512
        $estimate = [ordered]@{
            tier = $tier
            transition_budget = $tierBudget
            planned_episodes = $plannedEpisodeCount
            planned_decisions = $plannedDecisions
            budget_weighted_decisions = $plannedDecisions * $weight
            shard_count = $effectiveShardCount
            max_shard_episodes = $maxShardEpisodes
            max_shard_decisions = $maxShardDecisions
            max_shard_budget_weighted_decisions = $maxShardDecisions * $weight
        }
        $tierCostEstimates += $estimate
        Write-Output ("TTS_S1_TIER_COST tier={0} transition_budget={1} planned_episodes={2} planned_decisions={3} budget_weighted_decisions={4} shard_count={5} max_shard_decisions={6} max_shard_budget_weighted_decisions={7}" -f `
            $estimate.tier, $estimate.transition_budget, $estimate.planned_episodes, `
            $estimate.planned_decisions, $estimate.budget_weighted_decisions, `
            $estimate.shard_count, $estimate.max_shard_decisions, $estimate.max_shard_budget_weighted_decisions)
    }
    Write-Output 'TTS_S1_TIER_COST_UNIT decision searches, and the same weighted by the tier transition budget over 512; multiply by a measured per-decision cost for wall time, and read max_shard_* as the elapsed share once the shards run at once'

    # The plans are rebuilt at the EFFECTIVE count, through the same
    # function that produced the planned ones.
    $tierPlans = @($Tiers | ForEach-Object { New-TtsS1TierPlan -Tier $_ -TierShardCount $effectiveShardCount })

    # THE EFFECTIVE topology decides formality, not the requested one: a
    # fan-out clamped down by a small corpus measured a different machine
    # than the pinned eight-process one, whatever the flags asked for.
    $effectiveTopology = Test-TtsS1FormalShardTopology -ShardCount $effectiveShardCount -HostLogicalCpus $hostLogicalCpus
    $isFormalTopology = $isFormalLadder -and $effectiveTopology.admissible
    if ($isFormalLadder -and -not $isFormalTopology) {
        Write-Output ("TTS_S1_FORMAL_TOPOLOGY_LOST this run can no longer be formal: {0}" -f $effectiveTopology.reason)
    }
    Write-Output ("TTS_S1_TOPOLOGY shard_count={0} formal_shard_count={1} host_logical_cpus={2} required_logical_cpus={3} formal={4}" -f `
        $effectiveShardCount, $script:TtsS1FormalShardCount, $hostLogicalCpus, `
        $effectiveTopology.required_logical_cpus, $isFormalTopology)
}
catch {
    Write-TtsS1RunFailed -AttemptRoot $attemptRoot -Step 'sizing' -Detail $_.Exception.Message
    Write-Error $_.Exception.Message
    exit 1
}

# ---------------------------------------------------------------------------
# 2. Replay every tier, in ladder order. An INFEASIBLE tier (exit 4) is a
#    recorded verdict, not a wrapper failure: the ladder is measured in full.
# ---------------------------------------------------------------------------
$tierResults = @()
foreach ($plan in $tierPlans) {
    $shardResults = @()
    try {
        New-Item -ItemType Directory -Force -Path $plan.shard_root | Out-Null
        # EVERY SHARD STARTS BEFORE ANY IS WAITED ON, and nothing this loop
        # started outlives it: Invoke-TtsS1ProcessFanOut reaps in a finally,
        # so a throw partway through the K starts kills the ones already
        # running instead of orphaning them into the attempt directory.
        foreach ($shard in $plan.shards) {
            Write-Output "TTS_S1_STEP tier-$($plan.tier) shard-$($shard.index) $($shard.command_line)"
        }
        $barrierReleasedMicros = 0
        $fanOut = Invoke-TtsS1ProcessFanOut -Jobs @($plan.shards | ForEach-Object {
            [pscustomobject]@{
                label = "tier-$($plan.tier) shard-$($_.index)"
                file_path = $ReplayExecutable
                arguments = $_.arguments
                stdout_path = $_.stdout_path
                stderr_path = $_.stderr_path
            }
        }) -AfterStart {
            # EVERY SHARD IS STARTED, which is not the same as every shard
            # being READY. Process creation is nearly instant; loading a
            # checkpoint is not, and it does not take the same time in every
            # shard. Publishing the token here would release a fast shard to
            # search while a slow sibling was still reading weights, and
            # those head decisions are exactly the cheap ones a p99 would
            # thank you for.
            #
            # So the token waits for every shard's own announcement. This
            # runs inside the fan-out's try, so a shard that never announces
            # raises here, every started child is reaped, and the tier catch
            # writes RUN_FAILED naming both the silent shards and what it
            # killed.
            # THE TOKEN IS NOT STAMPED HERE, and that is the point. Its
            # instant is compared, exactly and with no tolerance, against
            # instants the SHARDS recorded through the crate's own clock; a
            # token stamped from THIS runtime would be a second clock in the
            # comparison, and this runtime's DateTimeOffset::UtcNow can
            # advance on a coarser cadence than the Windows clock behind
            # Rust's SystemTime::now(). So the replay bin's publish mode
            # does both halves: it waits for every announcement and stamps
            # the token from the same function a shard announces with.
            #
            # It is a nested single-child fan-out, which is safe here: the
            # outer fan-out reset the reaped-children channel when it
            # started, has not reached its own finally, and overwrites the
            # channel again when it does.
            $publishExit = Invoke-TtsS1Process -FilePath $ReplayExecutable `
                -Arguments $plan.publish_arguments `
                -StdoutPath $plan.publish_stdout_path -StderrPath $plan.publish_stderr_path
            if ($publishExit -ne 0) {
                throw "tts_s1_replay_v1 --publish-start-barrier exited with $publishExit for tier $($plan.tier); the start barrier was not released; see tier-$($plan.tier).barrier.stderr.txt"
            }
            $script:TtsS1BarrierPublishExit = $publishExit
            # Read back what the publisher stamped, so the launcher records
            # the token it actually released rather than a number of its
            # own.
            $script:TtsS1BarrierReleasedMicros = [long](([System.IO.File]::ReadAllText($plan.barrier_path)).Trim())
            $script:TtsS1LastShardReadiness = @(@(Get-Content -LiteralPath $plan.publish_stdout_path) |
                Where-Object { $_ -like 'TTS_S1_SHARD_READY *' })
            Write-Output ("TTS_S1_SHARDS_READY tier={0} shards={1} released_unix_micros={2}" -f `
                $plan.tier, @($script:TtsS1LastShardReadiness).Count, $script:TtsS1BarrierReleasedMicros)
            # DIAGNOSTIC ONLY, and labelled as such: how far this runtime's
            # clock reads from the one that stamped the token. It gates
            # nothing.
            Write-Output ("TTS_S1_BARRIER_CLOCK_SKEW tier={0} launcher_micros={1} token_micros={2} skew_micros={3}" -f `
                $plan.tier, (Get-TtsS1UnixMicros), $script:TtsS1BarrierReleasedMicros, `
                ((Get-TtsS1UnixMicros) - $script:TtsS1BarrierReleasedMicros))
        }
        if (Test-Path -LiteralPath $plan.barrier_path) {
            $barrierReleasedMicros = $script:TtsS1BarrierReleasedMicros
        }
        Write-Output ("TTS_S1_BARRIER tier={0} token={1} released_unix_micros={2} ready_shards={3} publisher_exit={4} ready_timeout_seconds={5} shard_timeout_seconds={6}" -f `
            $plan.tier, $plan.barrier_path, $barrierReleasedMicros, `
            @($script:TtsS1LastShardReadiness).Count, $script:TtsS1BarrierPublishExit, `
            $plan.ready_timeout_seconds, $plan.barrier_timeout_seconds)
        # EVERY SHARD IS WAITED ON BEFORE ANY FAILURE IS RAISED, so a
        # failing shard never leaves its siblings running unattended behind
        # a thrown launcher.
        $shardFailures = @()
        for ($index = 0; $index -lt $plan.shards.Count; $index++) {
            $shard = $plan.shards[$index]
            $shardExit = $fanOut.results[$index].exit_code
            $shardResults += [ordered]@{
                shard_index = $shard.index
                exit_code = $shardExit
                command_line = $shard.command_line
                diagnostics_dir = $shard.diagnostics_dir
                report_path = $shard.report_path
            }
            if ($shardExit -ne 0) {
                $shardFailures += "shard $($shard.index) exited with $shardExit"
            }
        }
        if ($shardFailures.Count -ne 0) {
            # FAIL CLOSED. A shard has no verdict to exit 4 on, so any
            # non-zero exit is a real failure and the tier has no report.
            throw "tier $($plan.tier): $($shardFailures -join '; '); see tier-$($plan.tier).shard-*.stderr.txt"
        }
        # Hash every shard report the merge is about to consume, so the
        # summary commits to the exact partial artifacts the tier report
        # was assembled from.
        for ($index = 0; $index -lt $shardResults.Count; $index++) {
            $shardResults[$index]['report'] = Get-TtsS1FileRecord -Path $shardResults[$index]['report_path']
        }

        Write-Output "TTS_S1_STEP tier-$($plan.tier) merge $($plan.merge_command_line)"
        $exitCode = Invoke-TtsS1Process -FilePath $ReplayExecutable -Arguments $plan.merge_arguments `
            -StdoutPath $plan.merge_stdout_path -StderrPath $plan.merge_stderr_path
        if ($exitCode -ne 0 -and $exitCode -ne 4) {
            throw "tts_s1_replay_v1 --merge-shards exited with $exitCode for tier $($plan.tier); see tier-$($plan.tier).merge.stderr.txt"
        }
        # Read AND validate in one call. Every dereference below reaches a
        # contract field, so a report missing one must be refused BEFORE the
        # first of them: otherwise a missing `compute_cap.latency_curve`
        # dies with a bare strict-mode PropertyNotFoundException naming
        # neither the tier nor the field the contract required.
        #
        # -RequireFormalShardTopology only when this run is still a
        # candidate for TTS_S1_COMPLETE: a smoke at another fan-out still
        # publishes a full, readable tier report and refusing to read it
        # would turn "carries no feasibility standing" into "the run
        # failed". When the run IS a candidate, a report measured at any
        # other topology is refused outright.
        $report = Read-TtsS1TierReport -Tier $plan.tier -Path $plan.report_path `
            -RequireFormalShardTopology:$isFormalTopology
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
            diagnostics_dir = $plan.diagnostics_root
            shard_count = $plan.shard_count
            shard_root = $plan.shard_root
            merge_command_line = $plan.merge_command_line
            shards = @($shardResults)
            # THE MACHINE THIS TIER'S TIMINGS WERE TAKEN ON, read off the
            # report rather than off this launcher's flags: the measuring
            # processes are the ones that saw the host.
            measured_shard_count = $report.body.shard_topology.shard_count
            formal_shard_count = $report.body.shard_topology.formal_shard_count
            host_logical_cpus = $report.body.shard_topology.host_logical_cpus
            host_total_memory_bytes = $report.body.shard_topology.host_total_memory_bytes
            meets_formal_topology = $report.body.shard_topology.meets_formal_topology
            formal_topology_reason = $report.body.shard_topology.formal_topology_reason
            shard_topology_rule = $report.body.shard_topology.rule
            # THE MEASURED OVERLAP, which is what separates eight processes
            # run together from eight run one after another.
            barrier_path = $plan.barrier_path
            barrier_publisher_command_line = $plan.publish_command_line
            barrier_publisher_exit_code = $script:TtsS1BarrierPublishExit
            barrier_token = Get-TtsS1FileRecord -Path $plan.barrier_path
            barrier_token_micros = $barrierReleasedMicros
            barrier_released_unix_micros = $report.body.shard_topology.start_barrier_released_unix_micros
            shard_readiness = @($report.body.shard_topology.shard_readiness)
            latest_ready_unix_micros = $report.body.shard_topology.latest_ready_unix_micros
            released_after_every_shard_ready = $report.body.shard_topology.released_after_every_shard_ready
            every_shard_waited_on_the_barrier = $report.body.shard_topology.every_shard_waited_on_the_barrier
            every_first_decision_after_the_barrier = $report.body.shard_topology.every_first_decision_after_the_barrier
            censused_decisions = $report.body.shard_topology.censused_decisions
            fully_concurrent_decisions = $report.body.shard_topology.fully_concurrent_decisions
            fully_concurrent_permille = $report.body.shard_topology.fully_concurrent_permille
            min_fully_concurrent_permille = $report.body.shard_topology.min_fully_concurrent_permille
            concurrency_histogram = @($report.body.shard_topology.concurrency_histogram)
            episodes_replayed = $report.body.episodes_replayed
            searched_decisions = $report.body.searched_decisions
            corpus_targets_replayed = $report.body.corpus_targets_replayed
            replayed_whole_corpus = $report.body.replayed_whole_corpus
            # THE VERDICT BASIS: the frozen stratified corpus's own
            # targets, which is the population the sketch defines S1 over.
            # The report names the gating view itself; this wrapper reads
            # it rather than assuming which one it is.
            verdict_view = $report.body.verdict_view
            target_protocol_p50_micros = $report.body.corpus_target_view.protocol_wall_time.p50_micros
            target_protocol_p99_micros = $report.body.corpus_target_view.protocol_wall_time.p99_micros
            target_protocol_max_micros = $report.body.corpus_target_view.protocol_wall_time.max_micros
            target_mean_protocol_micros = $report.body.corpus_target_view.mean_protocol_micros
            target_decisions_per_second_milli = $report.body.corpus_target_view.decisions_per_second_milli
            # DIAGNOSTIC: every decision searched. Not the latency gate.
            protocol_p50_micros = $report.body.whole_episode_view.protocol_wall_time.p50_micros
            protocol_p99_micros = $report.body.whole_episode_view.protocol_wall_time.p99_micros
            protocol_max_micros = $report.body.whole_episode_view.protocol_wall_time.max_micros
            mean_protocol_micros = $report.body.whole_episode_view.mean_protocol_micros
            search_p50_micros = $report.body.whole_episode_view.search_wall_time.p50_micros
            search_p99_micros = $report.body.whole_episode_view.search_wall_time.p99_micros
            search_max_micros = $report.body.whole_episode_view.search_wall_time.max_micros
            decisions_per_second_milli = $report.body.whole_episode_view.decisions_per_second_milli
            # The compute cap, estimated per episode against the fitted
            # per-ordinal latency curve.
            estimated_episode_count = $report.body.compute_cap.estimated_episode_count
            mean_estimated_episode_micros = $report.body.compute_cap.mean_estimated_episode_micros
            max_estimated_episode_micros = $report.body.compute_cap.max_estimated_episode_micros
            extrapolated_ordinals = $report.body.compute_cap.extrapolated_ordinals
            curve_last_observed_ordinal = $report.body.compute_cap.latency_curve.last_observed_ordinal
            curve_extrapolation_slope_micros_per_ordinal = $report.body.compute_cap.latency_curve.extrapolation_slope_micros_per_ordinal
            curve_knot_count = @($report.body.compute_cap.latency_curve.knots).Count
            compute_cap_rule = $report.body.compute_cap.rule
            latency_curve_rule = $report.body.compute_cap.latency_curve.rule
            projected_s2_worker_hours_milli = $report.body.compute_cap.projected_worker_hours_milli
            projected_elapsed_hours_at_workers_milli = $report.body.compute_cap.projected_elapsed_hours_at_workers_milli
            compute_cap_worker_hours_milli = $report.body.compute_cap.cap_worker_hours_milli
            within_compute_cap = $report.body.compute_cap.within_cap
            search_authority_digest_sha256 = $report.body.search_authority_digest_sha256
        }
        Write-Output ("TTS_S1_TIER_TOPOLOGY tier={0} shard_count={1} barrier={2} released_after_every_shard_ready={8} first_decision_after_barrier={3} fully_concurrent_permille={4} of={5} required={6} formal={7}" -f `
            $plan.tier, $report.body.shard_topology.shard_count, `
            $report.body.shard_topology.every_shard_waited_on_the_barrier, `
            $report.body.shard_topology.every_first_decision_after_the_barrier, `
            $report.body.shard_topology.fully_concurrent_permille, `
            $report.body.shard_topology.censused_decisions, `
            $report.body.shard_topology.min_fully_concurrent_permille, `
            $report.body.shard_topology.meets_formal_topology, `
            $report.body.shard_topology.released_after_every_shard_ready)
        Write-Output ("TTS_S1_TIER tier={0} verdict={1} verdict_view={2} episodes={3} searched_decisions={4} shard_count={5} target_protocol_p99_micros={6} target_protocol_max_micros={7} whole_episode_protocol_p99_micros={8} projected_s2_worker_hours_milli={9} extrapolated_ordinals={10} within_compute_cap={11}" -f `
            $plan.tier, $observedVerdict, $report.body.verdict_view, `
            $report.body.episodes_replayed, $report.body.searched_decisions, $plan.shard_count, `
            $report.body.corpus_target_view.protocol_wall_time.p99_micros, `
            $report.body.corpus_target_view.protocol_wall_time.max_micros, `
            $report.body.whole_episode_view.protocol_wall_time.p99_micros, `
            $report.body.compute_cap.projected_worker_hours_milli, `
            $report.body.compute_cap.extrapolated_ordinals, $report.body.compute_cap.within_cap)
    }
    catch {
        # Whatever failed, the fan-out has already reaped anything it left
        # running; the attempt's RUN_FAILED names them, so an operator
        # reading it knows no shard is still writing into the tree.
        $detail = $_.Exception.Message + (Format-TtsS1StoppedChildren -Stopped @($script:TtsS1LastFanOutStopped))
        Write-TtsS1RunFailed -AttemptRoot $attemptRoot -Step "tier-$($plan.tier)" -Detail $detail
        Write-Error $detail
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
# And on the topology too: each tier report carries the measuring
# processes' own reading of the concurrency and the host, and THAT is the
# authority rather than this launcher's flags.
$everyTierFormalTopology = $true
foreach ($result in $tierResults) {
    if (-not $result.meets_formal_topology) { $everyTierFormalTopology = $false }
}
$isFormal = $isFormalTopology -and $everyTierWholeCorpus -and $everyTierFormalTopology
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
    corpus_contributing_episode_count = $corpus.body.contributing_episode_count
    corpus_contributing_episode_decisions = $corpus.body.contributing_episode_decisions
    shard_count_requested = $ShardCount
    shard_count_effective = $effectiveShardCount
    shard_assignment_rule = $script:TtsS1ShardAssignmentRule
    shard_topology_rule = $script:TtsS1ShardTopologyRule
    formal_shard_count = $script:TtsS1FormalShardCount
    formal_logical_cpus_per_shard = $script:TtsS1FormalLogicalCpusPerShard
    formal_min_full_concurrency_permille = $script:TtsS1FormalMinFullConcurrencyPermille
    shard_ready_timeout_seconds = $script:TtsS1ShardReadyTimeoutSeconds
    start_barrier_timeout_seconds = $script:TtsS1StartBarrierTimeoutSeconds
    host_logical_cpus = $hostLogicalCpus
    formal_topology = $isFormalTopology
    formal_topology_reason = $effectiveTopology.reason
    every_tier_formal_topology = $everyTierFormalTopology
    planned_episodes = $plannedEpisodeCount
    planned_decisions = $plannedDecisions
    max_shard_episodes = $maxShardEpisodes
    max_shard_decisions = $maxShardDecisions
    tier_cost_estimates = @($tierCostEstimates)
    corpus_all_episode_count = $corpus.body.all_episode_decisions.episode_count
    corpus_all_episode_mean_decisions_milli = $corpus.body.all_episode_decisions.mean_decisions_milli
    corpus_all_episode_max_decisions = $corpus.body.all_episode_decisions.max_decisions
    slo_seconds = $script:TtsS1SloSeconds
    hard_timeout_seconds = $script:TtsS1HardTimeoutSeconds
    compute_cap_rule = $script:TtsS1ProjectionRule
    latency_curve_rule = $script:TtsS1LatencyCurveRule
    verdict_view = $script:TtsS1VerdictView
    pinned_contract = Get-TtsS1PinnedContract
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

Write-Output "TTS_S1_SUMMARY attempt_root=$attemptRoot status=$status formal_ladder=$isFormal shard_count=$effectiveShardCount formal_shard_count=$($script:TtsS1FormalShardCount) host_logical_cpus=$hostLogicalCpus feasible_tier_count=$($feasible.Count) feasible_tiers=$($feasible -join ',')"
if (-not $isFormal) {
    Write-Output 'TTS_S1_RESULT this run is a SMOKE (a partial corpus, a partial ladder, or a shard topology other than the pinned formal one); it carries no feasibility standing, no TTS_S1_COMPLETE marker was written, and it may not be read as closing the ladder either way'
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
