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

function Get-TtsS1PinnedContract {
    # The whole pinned contract, for the provenance and summary records.
    return [ordered]@{
        compute_cap_rule = $script:TtsS1ProjectionRule
        latency_curve_rule = $script:TtsS1LatencyCurveRule
        verdict_view = $script:TtsS1VerdictView
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
        [Parameter(Mandatory = $true)][string]$Path
    )
    $report = Read-TtsS1Json -Path $Path
    Assert-TtsS1TierReportContract -Tier $Tier -Report $report
    return $report
}

function Assert-TtsS1TierReportContract {
    # Every pinned string a tier report must declare, checked before the
    # report is summarized. Case-sensitive throughout: these are identity
    # strings, not prose.
    param(
        [Parameter(Mandatory = $true)][string]$Tier,
        [Parameter(Mandatory = $true)][AllowNull()]$Report
    )
    $checks = @(
        @{ Path = 'body.verdict_view'; Expected = $script:TtsS1VerdictView; What = 'gating view' },
        @{ Path = 'body.compute_cap.rule'; Expected = $script:TtsS1ProjectionRule; What = 'compute-cap rule' },
        @{ Path = 'body.compute_cap.latency_curve.rule'; Expected = $script:TtsS1LatencyCurveRule; What = 'latency-curve rule' }
    )
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

function Invoke-TtsS1Process {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$StdoutPath,
        [Parameter(Mandatory = $true)][string]$StderrPath
    )
    return Wait-TtsS1Process -Process (Start-TtsS1Process -FilePath $FilePath -Arguments $Arguments `
            -StdoutPath $StdoutPath -StderrPath $StderrPath)
}
