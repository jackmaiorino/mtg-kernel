Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

<#
Test-time-search wrapper, stage S1: the pinned report contract.

Every string here is a COMPILED CONSTANT on the Rust side
(native_tts_s1_replay_v1.rs). They are restated in this file for one purpose:
so the launcher can reject a tier report produced by a different rule BEFORE
it is summarized and before the run can be marked complete. They are never an
independent source of truth, and a disagreement is always resolved in the
Rust constant's favour by updating this file.

Why the whole contract and not just the top-level rule: the compute-cap block
NESTS the fitted latency curve, and the curve carries its own rule string.
A partially updated replay binary can therefore declare the current
compute-cap rule while the curve inside it was fitted under a superseded one,
which is precisely the shape of report that must not be allowed to mark a run
complete. Pinning only the outer rule would accept it.

This file lives apart from run-tts-s1.ps1 so the dry-run tests can dot-source
the same constants and the same validation the launcher uses, rather than
re-implementing either. It defines constants and functions only; it launches
nothing and has no side effects.

It also holds the launcher's shared child-process primitives (the Windows
argument quoting, the printed command line, and starting and waiting on a
child) for exactly the same reason: the shard fan-out's property, that K
children run at once and every one of them is waited on and its exit code
captured, cannot be observed from a planned command line, so the tests have
to be able to call the real functions.
#>

# native_tts_s1_replay_v1::TTS_S1_S2_PROJECTION_RULE_V2
$script:TtsS1ProjectionRule = 'wrapped-games-only' +
    '-3072-root-clusters-times-2-paired-units' +
    '-isotonic-per-ordinal-protocol-latency-curve-fitted-to-whole-episode-timings' +
    '-extrapolated-past-the-last-observed-ordinal-at-the-maximum-adjacent-fitted-rise' +
    '-every-harvested-episode-natural-and-truncated-costed-at-its-own-length' +
    '-mean-estimated-episode-cost-times-wrapped-games' +
    '-as-aggregate-worker-hours-with-no-worker-division' +
    '/v2'

# native_tts_s1_replay_v1::TTS_S1_LATENCY_CURVE_RULE_V2
$script:TtsS1LatencyCurveRule = 'pool-adjacent-violators-isotonic-regression-over-decision-ordinal' +
    '-on-whole-episode-protocol-micros-pre-aggregated-per-ordinal' +
    '-extrapolated-past-the-last-observed-ordinal' +
    '-at-the-maximum-rise-between-adjacent-fitted-ordinals' +
    '-floored-at-one-micro-per-ordinal' +
    '/v2'

# native_tts_s1_replay_v1::TTS_S1_VERDICT_VIEW_V1
$script:TtsS1VerdictView = 'corpus_target_view'

# native_tts_s1_replay_v1::TTS_S1_SHARD_ASSIGNMENT_RULE_V1
#
# Recorded in the provenance, NOT checked against a tier report: a merged
# tier report is the report an unsharded run would have published and
# therefore says nothing about shards at all. The rule is declared by each
# SHARD report and is checked there, by the merge, on the Rust side.
$script:TtsS1ShardAssignmentRule =
    'contributing-episode-position-in-corpus-order-modulo-shard-count-equals-shard-index/v1'

# native_tts_s1_replay_v1::TTS_S1_MAX_SHARD_COUNT_V1
$script:TtsS1MaxShardCount = 64

# native_tts_s1_replay_v1::TTS_S1_FORMAL_SHARD_COUNT_V1
#
# THE PINNED FORMAL CONCURRENCY, and the reason it is a pin and not a knob:
# every latency in a tier report is a wall-time sample taken while K replay
# processes contended for the host's cores and the disk the production
# diagnostics writer republishes an episode file to after every decision. The
# p99 SLO clause, the hard-timeout clause and the isotonic curve the
# compute-cap projection is fitted to are all such samples, so -ShardCount
# would otherwise be a knob that can flip a tier's verdict. Eight, because
# the CP7 panel host runs the wrapped agent under eight concurrent games and
# a formal S1 latency claim has to be measured at the concurrency the product
# is served at. Any other count still runs and still publishes every report;
# it is a TTS_S1_SMOKE and may never close the ladder either way.
$script:TtsS1FormalShardCount = 8

# native_tts_s1_replay_v1::TTS_S1_FORMAL_LOGICAL_CPUS_PER_SHARD_V1
$script:TtsS1FormalLogicalCpusPerShard = 2

# native_tts_s1_replay_v1::TTS_S1_FORMAL_MIN_FULL_CONCURRENCY_PERMILLE_V1
#
# A declared shard count is an INTENTION: eight processes run one after
# another declare exactly the same eight as eight run together, and every
# latency the sequential eight measured was taken on a near-idle machine.
# The merged report therefore carries a per-decision overlap census, and
# formal standing needs at least this permille of decisions to have been
# searched while every other shard was mid-work. The 5 percent that is not
# required is the tail: shards own different episodes, episodes are
# different lengths, and the shard that runs out of work first stops
# contending while the others finish.
$script:TtsS1FormalMinFullConcurrencyPermille = 950

# native_tts_s1_replay_v1::TTS_S1_SHARD_TOPOLOGY_RULE_V2
$script:TtsS1ShardTopologyRule = 'formal-s1-timings-are-measured-at-exactly-8-concurrent-replay-processes' +
    '-the-concurrency-the-cp7-panel-host-runs-the-wrapped-agent-under' +
    '-on-a-host-with-at-least-2-logical-cpus-per-shard' +
    '-every-shard-released-by-one-start-barrier-before-its-first-decision' +
    '-and-at-least-950-permille-of-decisions-observed-mid-work-in-every-other-shard' +
    '-any-other-shard-count-or-a-smaller-host-or-unproven-overlap-is-a-smoke-and-never-a-feasibility-result' +
    '/v2'

# How long a shard may wait at the start barrier before failing closed.
# The wait covers every OTHER shard's process start and checkpoint load, so
# it is generous; what matters is that it is bounded, because a shard that
# gave up and went ahead alone would measure an idle machine.
$script:TtsS1StartBarrierTimeoutSeconds = 900

function Get-TtsS1PinnedContract {
    # The whole pinned contract, for the provenance and summary records.
    return [ordered]@{
        compute_cap_rule = $script:TtsS1ProjectionRule
        latency_curve_rule = $script:TtsS1LatencyCurveRule
        verdict_view = $script:TtsS1VerdictView
        shard_topology_rule = $script:TtsS1ShardTopologyRule
        formal_shard_count = $script:TtsS1FormalShardCount
        formal_logical_cpus_per_shard = $script:TtsS1FormalLogicalCpusPerShard
        formal_min_full_concurrency_permille = $script:TtsS1FormalMinFullConcurrencyPermille
    }
}

function Test-TtsS1FormalShardTopology {
    # The formal-topology rule, as a pure function of the two numbers it is
    # about, mirroring native_tts_s1_replay_v1::TtsS1ShardTopologyV1::evaluate_v1.
    #
    # It is here, and pure, so the launcher and the dry-run tests apply the
    # same rule: the tests can evaluate it at CPU counts this host does not
    # have, which is the only way to cover both branches without depending
    # on the machine the suite happens to run on. The tier reports remain
    # the authority for a finished run; this is what lets a launch refuse
    # before it starts.
    param(
        [Parameter(Mandatory = $true)][int]$ShardCount,
        [Parameter(Mandatory = $true)][int]$HostLogicalCpus
    )
    $required = $script:TtsS1FormalLogicalCpusPerShard * $ShardCount
    $failures = @()
    if ($ShardCount -ne $script:TtsS1FormalShardCount) {
        $failures += ("the timings would be measured at {0} concurrent replay processes, not the pinned {1}" -f `
            $ShardCount, $script:TtsS1FormalShardCount)
    }
    if ($HostLogicalCpus -lt $required) {
        $failures += ("the host reports {0} logical CPUs, below the {1} a {2}-shard run requires at {3} per shard" -f `
            $HostLogicalCpus, $required, $ShardCount, $script:TtsS1FormalLogicalCpusPerShard)
    }
    $reason = 'the run would be measured at the pinned concurrency on a host large enough for it'
    if ($failures.Count -ne 0) { $reason = ($failures -join '; ') }
    return [pscustomobject]@{
        admissible = ($failures.Count -eq 0)
        shard_count = $ShardCount
        host_logical_cpus = $HostLogicalCpus
        required_logical_cpus = $required
        reason = $reason
    }
}

function Get-TtsS1ReportField {
    # Walks a dotted path through a decoded report and fails closed, by name,
    # on anything missing. Done through PSObject.Properties rather than plain
    # member access so a missing nested block reports WHICH field is absent
    # instead of a strict-mode PropertyNotFound on an inner segment.
    param(
        [Parameter(Mandatory = $true)][AllowNull()]$Report,
        [Parameter(Mandatory = $true)][string]$Path
    )
    $current = $Report
    foreach ($segment in $Path.Split('.')) {
        if ($null -eq $current) {
            throw "tier report is missing $Path"
        }
        $property = $current.PSObject.Properties[$segment]
        if ($null -eq $property) {
            throw "tier report is missing $Path"
        }
        $current = $property.Value
    }
    return $current
}

function Read-TtsS1Json {
    # Decodes a UTF-8 JSON document, tolerating a BOM a producer may have
    # written. Lives here rather than in the launcher so the launcher and
    # these tests read a tier report through the same code.
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "required JSON document is missing: $Path"
    }
    $text = [System.IO.File]::ReadAllText($Path, [System.Text.UTF8Encoding]::new($false))
    if ($text.Length -gt 0 -and [int][char]$text[0] -eq 65279) { $text = $text.Substring(1) }
    return $text | ConvertFrom-Json
}

function Read-TtsS1TierReport {
    # Reads one tier report AND validates its pinned contract, in that order
    # and in one call.
    #
    # The two steps are fused deliberately. Splitting them lets a caller
    # dereference a contract field (say `compute_cap.latency_curve`) between
    # the read and the assertion, and under Set-StrictMode a report missing
    # that block then dies with a bare PropertyNotFoundException naming
    # neither the tier nor the field the contract required. Returning only
    # already-validated reports makes that ordering impossible to get wrong:
    # there is no way to hold an unvalidated one.
    param(
        [Parameter(Mandatory = $true)][string]$Tier,
        [Parameter(Mandatory = $true)][string]$Path,
        # Set only for a run that could still carry feasibility standing.
        # See Assert-TtsS1TierReportContract.
        [switch]$RequireFormalShardTopology
    )
    $report = Read-TtsS1Json -Path $Path
    Assert-TtsS1TierReportContract -Tier $Tier -Report $report `
        -RequireFormalShardTopology:$RequireFormalShardTopology
    return $report
}

function Assert-TtsS1TierReportContract {
    # Every pinned string a tier report must declare, checked before the
    # report is summarized. Case-sensitive throughout: these are identity
    # strings, not prose.
    #
    # -RequireFormalShardTopology additionally demands that the report was
    # measured at the pinned concurrency on a host large enough for it. It
    # is a SWITCH rather than an always-on check because a run at another
    # count is a legitimate smoke that still publishes a full tier report:
    # refusing to read that report would turn "this carries no feasibility
    # standing" into "this run failed". The caller passes the switch exactly
    # when the run is still a candidate for TTS_S1_COMPLETE, so a report
    # that could be read as formal is never accepted at another topology.
    param(
        [Parameter(Mandatory = $true)][string]$Tier,
        [Parameter(Mandatory = $true)][AllowNull()]$Report,
        [switch]$RequireFormalShardTopology
    )
    $checks = @(
        @{ Path = 'body.verdict_view'; Expected = $script:TtsS1VerdictView; What = 'gating view' },
        @{ Path = 'body.compute_cap.rule'; Expected = $script:TtsS1ProjectionRule; What = 'compute-cap rule' },
        @{ Path = 'body.compute_cap.latency_curve.rule'; Expected = $script:TtsS1LatencyCurveRule; What = 'latency-curve rule' },
        # The topology PIN itself, checked on every report whatever its
        # standing: a report whose idea of the formal count is not this
        # one's was produced by a differently pinned binary, and its
        # meets_formal_topology would mean something else.
        @{ Path = 'body.shard_topology.rule'; Expected = $script:TtsS1ShardTopologyRule; What = 'shard-topology rule' },
        @{ Path = 'body.shard_topology.formal_shard_count'; Expected = $script:TtsS1FormalShardCount; What = 'pinned formal shard count' },
        @{ Path = 'body.shard_topology.formal_logical_cpus_per_shard'; Expected = $script:TtsS1FormalLogicalCpusPerShard; What = 'pinned logical CPUs per shard' },
        @{ Path = 'body.shard_topology.min_fully_concurrent_permille'; Expected = $script:TtsS1FormalMinFullConcurrencyPermille; What = 'pinned full-concurrency tolerance' }
    )
    if ($RequireFormalShardTopology) {
        $checks += @{ Path = 'body.shard_topology.shard_count'; Expected = $script:TtsS1FormalShardCount; What = 'measured shard count' }
        # THE MEASURED OVERLAP, not the declared count: eight processes run
        # one after another declare the same eight as eight run together.
        $checks += @{ Path = 'body.shard_topology.every_shard_waited_on_the_barrier'; Expected = $true; What = 'start barrier' }
        $checks += @{ Path = 'body.shard_topology.every_first_decision_after_the_barrier'; Expected = $true; What = 'first decision after the barrier' }
        $checks += @{ Path = 'body.shard_topology.meets_formal_topology'; Expected = $true; What = 'formal topology' }
    }
    foreach ($check in $checks) {
        $observed = Get-TtsS1ReportField -Report $Report -Path $check.Path
        if ($observed -cne $check.Expected) {
            throw ("tier {0} declares {1} '{2}' at {3}, not the pre-registered '{4}'" -f `
                $Tier, $check.What, $observed, $check.Path, $check.Expected)
        }
    }
}

# ---------------------------------------------------------------------------
# The shared child-process primitives: Windows argument quoting, the command
# line a reviewer reads, and starting and waiting on a child.
#
# They live here rather than in the launcher for the same reason the pinned
# contract does: the dry-run tests exercise the SAME functions the launcher
# runs. That matters most for the shard fan-out, where the property under
# test (K children running at once, every one waited on, every exit code
# captured) cannot be observed from a planned command line at all.
# ---------------------------------------------------------------------------

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

function Start-TtsS1Process {
    # Starts one child and returns it WITHOUT waiting, so the caller can
    # hold several at once. The shard fan-out is the only reason this is
    # separate from the wait: K replay processes at one tier have to be
    # running at the same time or there is no parallelism at all.
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
    # READING .Handle IS LOAD-BEARING, and this line is the whole reason a
    # child's exit code is knowable at all.
    #
    # Under PowerShell 5.1 the object Start-Process -PassThru returns does
    # not keep a process handle of its own, and once the child exits its
    # ExitCode reads back as $null rather than a number. Nothing about that
    # is visible at the call site: it waits, it gets $null, and a [int] cast
    # turns $null into 0, so EVERY child looks like a clean success no
    # matter what it did. Touching .Handle while the child is still alive
    # caches a handle on the object, and ExitCode is readable for the rest
    # of the object's life. [`Wait-TtsS1Process`] refuses a $null exit code
    # outright, so if this ever stops working the run fails closed instead
    # of reporting success it did not observe.
    $null = $process.Handle
    return $process
}

function Wait-TtsS1Process {
    param([Parameter(Mandatory = $true)]$Process)
    # WaitForExit() then Refresh(), following the cycle-4 launcher: the
    # parameterless overload alone can return before ExitCode is populated.
    $Process.WaitForExit()
    $Process.Refresh()
    $exitCode = $Process.ExitCode
    # FAIL CLOSED on an unknowable exit. `[int]$null` is 0, which is exactly
    # the value that means "the child succeeded", so a cast here would
    # convert "we could not tell" into "it worked".
    if ($null -eq $exitCode) {
        throw 'a child process exited but its exit code could not be read; an unknown exit is never a success'
    }
    return [int]$exitCode
}

# What the last Invoke-TtsS1ProcessFanOut had to stop, as records carrying a
# label and a process id.
#
# A side channel, and it needs one: the reaping happens in a `finally`, and
# a `finally` cannot return a value to a caller that is about to receive an
# exception instead. The caller's own catch reads this to say, in the
# attempt's RUN_FAILED text, which children it killed. Reset at the top of
# every fan-out so a stale list can never be reported against a later one.
$script:TtsS1LastFanOutStopped = @()

function Invoke-TtsS1ProcessFanOut {
    <#
    Starts every job, waits on every started one, and NEVER leaves a child
    running behind an error.

    THE FAILURE THIS EXISTS FOR: starting K children is K chances to throw,
    and a throw on the third start leaves the first two running. If the
    caller's catch then exits the launcher, those two are orphaned: still
    searching, still writing into the attempt's diagnostics directories,
    outliving the run that is supposed to own them. So the whole fan-out
    sits inside a try/finally whose finally reaps unconditionally: anything
    still alive is stopped and then waited on, whether the fan-out is
    unwinding from a start failure, from a wait failure, or finishing
    normally (in which case there is nothing left to reap and the list is
    empty).

    Nothing in the finally throws. A reap that failed loudly would replace
    the real error with its own.

    `Jobs` are objects carrying label, file_path, arguments, stdout_path and
    stderr_path. Returns the per-job exit codes; interpreting them is the
    caller's business, because "non-zero" means different things to a shard
    and to a merge.
    #>
    param(
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][array]$Jobs,
        # Run ONCE, after every job has started and before any is waited
        # on. It exists for the start barrier: the token may only be
        # published when the whole fan-out is up, and there is no other
        # moment in this function's life when that is true. A hook that
        # throws unwinds like any other failure, so the already-started
        # children are still reaped.
        [scriptblock]$AfterStart
    )
    $script:TtsS1LastFanOutStopped = @()
    $running = @()
    $results = @()
    $stopped = @()
    try {
        foreach ($job in $Jobs) {
            $running += [pscustomobject]@{
                label = $job.label
                process = Start-TtsS1Process -FilePath $job.file_path -Arguments $job.arguments `
                    -StdoutPath $job.stdout_path -StderrPath $job.stderr_path
            }
        }
        if ($null -ne $AfterStart) { & $AfterStart }
        foreach ($entry in $running) {
            $results += [pscustomobject]@{
                label = $entry.label
                exit_code = Wait-TtsS1Process -Process $entry.process
            }
        }
    }
    finally {
        foreach ($entry in $running) {
            $alive = $false
            try { $alive = -not $entry.process.HasExited } catch { $alive = $false }
            if ($alive) {
                $id = -1
                try { $id = [int]$entry.process.Id } catch { $id = -1 }
                try { Stop-Process -Id $id -Force -Confirm:$false -ErrorAction SilentlyContinue } catch { }
                # Waited on AFTER the stop, so this function does not return
                # while a child it killed is still winding down.
                try { $entry.process.WaitForExit() } catch { }
                $stopped += [pscustomobject]@{ label = $entry.label; process_id = $id }
            }
        }
        $script:TtsS1LastFanOutStopped = @($stopped)
    }
    return [pscustomobject]@{
        results = @($results)
        stopped = @($stopped)
    }
}

function Invoke-TtsS1Process {
    # ONE child, run through the same fan-out as K of them, so there is
    # exactly one place in this stack that starts a child and exactly one
    # that reaps: a wait that failed while its child was still running
    # would otherwise orphan it here too, for a corpus build or a merge.
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$StdoutPath,
        [Parameter(Mandatory = $true)][string]$StderrPath
    )
    $fanOut = Invoke-TtsS1ProcessFanOut -Jobs @([pscustomobject]@{
            label = $FilePath
            file_path = $FilePath
            arguments = $Arguments
            stdout_path = $StdoutPath
            stderr_path = $StderrPath
        })
    return $fanOut.results[0].exit_code
}

function Write-TtsS1StartBarrier {
    # Publishes the start token every shard of a tier is waiting on.
    #
    # THE TOKEN'S CONTENT IS ITS RELEASE TIME, in microseconds since the
    # UNIX epoch, and that is the point of writing anything at all: every
    # shard reports the same number back, so the merge can check that no
    # shard's first decision began before the fan-out was complete. A token
    # whose content were the reader's own clock would prove nothing.
    #
    # Published by staged sibling then move, so a shard polling for it can
    # never read a half-written one.
    param([Parameter(Mandatory = $true)][string]$Path)
    $directory = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($directory)) {
        New-Item -ItemType Directory -Force -Path $directory | Out-Null
    }
    $micros = [long](([System.DateTimeOffset]::UtcNow).ToUnixTimeMilliseconds()) * 1000
    $staged = "$Path.stage-$PID"
    [System.IO.File]::WriteAllText($staged, "$micros`n", [System.Text.UTF8Encoding]::new($false))
    try {
        Move-Item -LiteralPath $staged -Destination $Path -Force
    }
    catch {
        if (Test-Path -LiteralPath $staged) { Remove-Item -LiteralPath $staged -Force }
        throw
    }
    return $micros
}

function Format-TtsS1StoppedChildren {
    # The reaped children as one clause for a RUN_FAILED line, or an empty
    # string when nothing had to be stopped.
    param([Parameter(Mandatory = $true)][AllowEmptyCollection()][array]$Stopped)
    if ($Stopped.Count -eq 0) { return '' }
    $named = @($Stopped | ForEach-Object { "$($_.label) (pid $($_.process_id))" })
    return ("; stopped still-running children: " + ($named -join ', '))
}
