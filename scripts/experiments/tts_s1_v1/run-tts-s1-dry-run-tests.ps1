<#
.SYNOPSIS
Dry-run tests for run-tts-s1.ps1. Launches nothing.

.DESCRIPTION
Every case runs the wrapper with -DryRun -SkipHostAssertions against a
throwaway evidence root under the system temp directory, using two stand-in
executable FILES that are never executed (a dry run only hashes them).
NEITHER S1 BIN IS EVER STARTED: no corpus is built, no search runs, no CP7
panel is contacted, and no GPU is touched.

One section is the exception to "starts nothing" and says so where it sits:
the shared child-process primitives are exercised against cmd.exe, because
the property the shard fan-out rests on (several children running at once,
every one waited on, every exit code captured) cannot be observed from a
planned command line at all.

What it proves:
  * a dry run writes provenance.json and result.json with status
    DRY_RUN_PLANNED, and writes NEITHER terminal marker;
  * a dry run produces no corpus and no tier report;
  * the planned command lines carry exactly the flags the two bins declare,
    including the authority shape for each -StoreKind and the per-shard
    diagnostics directory the production writer publishes into;
  * every tier plans -ShardCount shard invocations, each with its own shard
    index, its own diagnostics directory and its own shard report path, plus
    exactly one merge invocation that names the shard directory, the same
    shard count, and the tier report as its output and carries no
    replay-only flag;
  * the launcher starts every shard before waiting on any of them, waits
    with the WaitForExit plus Refresh double call, waits on all of them
    before raising a failure, and fails closed on any non-zero shard exit;
  * the shared child-process primitives really do run several children at
    once, really do report each child's own exit code, and refuse an exit
    code they cannot read rather than casting it to a success;
  * every argument is quoted for the Windows command-line parser, so a
    store root or an evidence root containing a space survives the trip to
    the child;
  * a full-ladder whole-corpus run plans as FORMAL and a partial one plans
    as a SMOKE;
  * the wrapper's pinned report contract (the compute-cap rule, the NESTED
    latency-curve rule, and the gating view) is the current version, and the
    validation the launcher runs on every tier report rejects a stale value
    in any of the three, including a report that declares the current
    compute-cap rule over a curve fitted under a superseded one;
  * -LimitEpisodes appears only when it is set;
  * the input rejections all fire: equal seed blocks, a reordered ladder, a
    duplicated tier, a missing -Generation, a -Generation on the portable
    authority, a missing executable, and -SkipHostAssertions without
    -DryRun.

PowerShell 5.1 compatible: no `&&`, no ternary, no null-coalescing.
#>
param(
    [string]$ScriptPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($ScriptPath)) {
    $ScriptPath = Join-Path $PSScriptRoot 'run-tts-s1.ps1'
}
if (-not (Test-Path -LiteralPath $ScriptPath -PathType Leaf)) {
    throw "wrapper script not found: $ScriptPath"
}

$script:Failures = 0
$script:Checks = 0

# The wrapper's own -ShardCount default, restated so these tests count the
# planned commands the launcher will actually emit. A change to the default
# is meant to break this line and be reviewed, not to slip through.
$defaultShardCount = 8

function Assert-True {
    param(
        [Parameter(Mandatory = $true)][bool]$Condition,
        [Parameter(Mandatory = $true)][string]$Message
    )
    $script:Checks++
    if (-not $Condition) {
        $script:Failures++
        Write-Output "FAIL $Message"
        return
    }
    Write-Output "ok   $Message"
}

function New-TtsS1TestRoot {
    $root = Join-Path ([System.IO.Path]::GetTempPath()) ("tts-s1-dry-{0}-{1}" -f $PID, [System.Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Force -Path $root | Out-Null
    return $root
}

function New-TtsS1StandIn {
    # A stand-in for a built executable. A dry run only hashes it, so its
    # contents are irrelevant; making it a plain text file is deliberate, so
    # that a bug which actually LAUNCHED it would fail loudly instead of
    # silently doing something.
    param([Parameter(Mandatory = $true)][string]$Path)
    [System.IO.File]::WriteAllText($Path, "not an executable; dry-run stand-in`n", [System.Text.UTF8Encoding]::new($false))
    return $Path
}

function Invoke-Wrapper {
    param(
        [Parameter(Mandatory = $true)][hashtable]$Parameters
    )
    $output = @()
    $failure = $null
    try {
        $output = @(& $ScriptPath @Parameters)
    }
    catch {
        $failure = $_.Exception.Message
    }
    return [pscustomobject]@{
        Output = $output
        Failure = $failure
    }
}

function Get-OnlyAttemptRoot {
    param([Parameter(Mandatory = $true)][string]$EvidenceRoot)
    $children = @(Get-ChildItem -LiteralPath $EvidenceRoot -Directory)
    if ($children.Count -ne 1) {
        throw "expected exactly one attempt root under $EvidenceRoot, found $($children.Count)"
    }
    return $children[0].FullName
}

# The pinned contract and the validation the launcher runs, dot-sourced so
# these tests exercise the SAME constants and the SAME functions rather than
# re-implementing either. It defines constants and functions only.
#
# Resolved from the directory of the wrapper UNDER TEST, not from this test
# file's own directory: -ScriptPath may select a wrapper elsewhere (a
# candidate build, a second checkout), and loading this directory's
# common.ps1 would then assert one implementation while the wrapper loaded
# another. The wrapper itself dot-sources `$PSScriptRoot/common.ps1`, so
# taking it from beside the wrapper is exactly what the wrapper loads.
$script:TtsS1WrapperDirectory = Split-Path -Parent (Resolve-Path -LiteralPath $ScriptPath).Path
$script:TtsS1CommonPath = Join-Path $script:TtsS1WrapperDirectory 'common.ps1'
if (-not (Test-Path -LiteralPath $script:TtsS1CommonPath -PathType Leaf)) {
    throw "the wrapper under test has no common.ps1 beside it: $script:TtsS1CommonPath"
}
. $script:TtsS1CommonPath
$wrapperText = [System.IO.File]::ReadAllText($ScriptPath)

# The superseded strings, spelled out so a stale report can be built and
# rejected here rather than discovered after a formal run.
$staleCurveRule = 'pool-adjacent-violators-isotonic-regression-over-decision-ordinal' +
    '-on-pooled-whole-episode-protocol-micros' +
    '-extrapolated-past-the-last-observed-ordinal-at-the-maximum-fitted-slope' +
    '-floored-at-one-micro-per-ordinal' +
    '/v1'
$staleProjectionRule = 'wrapped-games-only' +
    '-3072-root-clusters-times-2-paired-units' +
    '-times-mean-decisions-per-episode-over-natural-and-truncated-episodes' +
    '-times-whole-episode-mean-protocol-decision-wall-time' +
    '-as-aggregate-worker-hours-with-no-worker-division' +
    '/v1'

function New-TtsS1TestReport {
    # A decoded tier report carrying only the fields the contract check
    # reads. Built fresh per case so a mutation cannot leak between them.
    param(
        [string]$VerdictView = $script:TtsS1VerdictView,
        [string]$ProjectionRule = $script:TtsS1ProjectionRule,
        [string]$CurveRule = $script:TtsS1LatencyCurveRule,
        [string]$TopologyRule = $script:TtsS1ShardTopologyRule,
        [int]$MeasuredShardCount = $script:TtsS1FormalShardCount,
        [int]$PinnedShardCount = $script:TtsS1FormalShardCount,
        [int]$PinnedCpusPerShard = $script:TtsS1FormalLogicalCpusPerShard,
        [bool]$MeetsFormalTopology = $true,
        [switch]$OmitLatencyCurve,
        [switch]$OmitShardTopology
    )
    $computeCap = [pscustomobject]@{ rule = $ProjectionRule }
    if (-not $OmitLatencyCurve) {
        $computeCap | Add-Member -NotePropertyName latency_curve -NotePropertyValue ([pscustomobject]@{ rule = $CurveRule })
    }
    $body = [pscustomobject]@{
        verdict_view = $VerdictView
        compute_cap = $computeCap
    }
    if (-not $OmitShardTopology) {
        $body | Add-Member -NotePropertyName shard_topology -NotePropertyValue ([pscustomobject]@{
            rule = $TopologyRule
            shard_count = $MeasuredShardCount
            formal_shard_count = $PinnedShardCount
            formal_logical_cpus_per_shard = $PinnedCpusPerShard
            host_logical_cpus = 32
            host_total_memory_bytes = 137438953472
            meets_formal_topology = $MeetsFormalTopology
            formal_topology_reason = 'synthetic'
        })
    }
    return [pscustomobject]@{ body = $body }
}

function Get-TtsS1ContractRejection {
    # The rejection message for a report the contract check refuses, or
    # $null if it was accepted. It asserts nothing itself: an Assert-True
    # inside here would put its own output on this function's output stream
    # and the caller would receive an array instead of the message.
    param(
        [Parameter(Mandatory = $true)]$Report,
        [switch]$RequireFormalShardTopology
    )
    try {
        Assert-TtsS1TierReportContract -Tier 't512' -Report $Report `
            -RequireFormalShardTopology:$RequireFormalShardTopology
    }
    catch {
        return $_.Exception.Message
    }
    return $null
}

# --- 0. The pinned contract itself, and the validation over synthetic
#        reports. No process is started and no file is written.
Assert-True ($script:TtsS1LatencyCurveRule -cne $script:TtsS1ProjectionRule) 'the curve rule and the compute-cap rule are distinct pins'
Assert-True ($script:TtsS1LatencyCurveRule -like '*/v2') 'the pinned latency-curve rule is the V2 one'
Assert-True ($script:TtsS1ProjectionRule -like '*/v2') 'the pinned compute-cap rule is the V2 one'

$valid = New-TtsS1TestReport
Assert-TtsS1TierReportContract -Tier 't512' -Report $valid
Assert-True $true 'a report declaring every pinned string is accepted'

# THE CASE THIS EXISTS FOR: a partially updated replay binary whose
# compute-cap rule is current but whose NESTED curve was fitted under the
# superseded one. Pinning only the outer rule would accept it.
$message = Get-TtsS1ContractRejection -Report (New-TtsS1TestReport -CurveRule $staleCurveRule)
Assert-True ($null -ne $message) 'the contract check rejects a stale NESTED latency-curve rule'
Assert-True ($message -like '*latency-curve rule*') 'the rejection names the latency-curve rule'
Assert-True ($message -like '*body.compute_cap.latency_curve.rule*') 'the rejection names the nested field path'

$message = Get-TtsS1ContractRejection -Report (New-TtsS1TestReport -ProjectionRule $staleProjectionRule)
Assert-True ($null -ne $message) 'the contract check rejects a stale compute-cap rule'
Assert-True ($message -like '*compute-cap rule*') 'the rejection names the compute-cap rule'

$message = Get-TtsS1ContractRejection -Report (New-TtsS1TestReport -VerdictView 'whole_episode_view')
Assert-True ($null -ne $message) 'the contract check rejects a report gated on the wrong view'
Assert-True ($message -like '*gating view*') 'the rejection names the gating view'

$message = Get-TtsS1ContractRejection -Report (New-TtsS1TestReport -OmitLatencyCurve)
Assert-True ($null -ne $message) 'the contract check rejects a report with no latency curve at all'
Assert-True ($message -like '*missing*latency_curve*') 'the rejection names the missing nested block'

# --- 0b. THE PINNED SHARD TOPOLOGY.
#
# Every latency in a tier report is a wall-time sample taken while K replay
# processes contended for the host, so the concurrency is part of what the
# numbers mean and -ShardCount would otherwise be a knob that can flip a
# verdict. The pin is checked on every report; the MEASURED count is checked
# only when the run could still be formal, because a smoke at another
# fan-out still publishes a readable report.
Assert-True ($script:TtsS1FormalShardCount -eq 8) 'the pinned formal shard count is eight'
Assert-True ($script:TtsS1FormalLogicalCpusPerShard -eq 2) 'the pin requires two logical CPUs per shard'
Assert-True ($script:TtsS1ShardTopologyRule -like '*exactly-8-concurrent-replay-processes*') 'the topology rule names the pinned concurrency'
Assert-True ($script:TtsS1ShardTopologyRule -like '*at-least-2-logical-cpus-per-shard*') 'the topology rule names the host requirement'
Assert-True ($script:TtsS1ShardTopologyRule -like '*/v1') 'the pinned topology rule is the V1 one'

# The RULE and the PIN are checked on every report, formal or not: a report
# whose idea of the formal count is not this one's came from a differently
# pinned binary, and its meets_formal_topology would mean something else.
foreach ($case in @(
    @{ Name = 'a stale shard-topology rule'; Report = (New-TtsS1TestReport -TopologyRule 'pooled-whatever/v0'); Fragment = 'shard-topology rule' },
    @{ Name = 'a differently pinned formal shard count'; Report = (New-TtsS1TestReport -PinnedShardCount 4); Fragment = 'pinned formal shard count' },
    @{ Name = 'a differently pinned CPUs-per-shard'; Report = (New-TtsS1TestReport -PinnedCpusPerShard 1); Fragment = 'pinned logical CPUs per shard' },
    @{ Name = 'a report with no shard topology at all'; Report = (New-TtsS1TestReport -OmitShardTopology); Fragment = 'missing' }
)) {
    foreach ($formal in @($true, $false)) {
        $message = Get-TtsS1ContractRejection -Report $case.Report -RequireFormalShardTopology:$formal
        Assert-True ($null -ne $message) "the contract check rejects $($case.Name) (formal=$formal)"
        Assert-True ($message -like "*$($case.Fragment)*") "the rejection of $($case.Name) names $($case.Fragment)"
    }
}

# THE SMOKE DEMOTION, not a hard rejection: a run at another fan-out is
# readable, and only a run still claiming formal standing is refused.
$offTopology = New-TtsS1TestReport -MeasuredShardCount 4 -MeetsFormalTopology $false
Assert-True ($null -eq (Get-TtsS1ContractRejection -Report $offTopology)) 'a report measured at another fan-out is readable as a smoke'
$message = Get-TtsS1ContractRejection -Report $offTopology -RequireFormalShardTopology
Assert-True ($null -ne $message) 'a run claiming formal standing refuses a report measured at another fan-out'
Assert-True ($message -like '*measured shard count*') 'the refusal names the measured shard count'

# The pinned count measured on a host too small for it: the count matches,
# so only the report's own meets_formal_topology can catch it.
$cramped = New-TtsS1TestReport -MeetsFormalTopology $false
Assert-True ($null -eq (Get-TtsS1ContractRejection -Report $cramped)) 'a cramped-host report is still readable as a smoke'
$message = Get-TtsS1ContractRejection -Report $cramped -RequireFormalShardTopology
Assert-True ($null -ne $message) 'a run claiming formal standing refuses a report the measuring processes said was not formal'
Assert-True ($message -like '*formal topology*') 'the refusal names the formal topology'

# And the pinned topology, measured, is accepted with the switch on.
Assert-True ($null -eq (Get-TtsS1ContractRejection -Report (New-TtsS1TestReport) -RequireFormalShardTopology)) 'a report measured at the pinned topology carries formal standing'

# THE RULE ITSELF, as a pure function, evaluated at CPU counts this host
# may not have. Both branches are covered without depending on the machine
# the suite happens to run on.
foreach ($case in @(
    @{ Shards = 8; Cpus = 16; Admissible = $true },
    @{ Shards = 8; Cpus = 24; Admissible = $true },
    @{ Shards = 8; Cpus = 15; Admissible = $false },
    @{ Shards = 8; Cpus = 1;  Admissible = $false },
    @{ Shards = 1; Cpus = 256; Admissible = $false },
    @{ Shards = 4; Cpus = 256; Admissible = $false },
    @{ Shards = 16; Cpus = 256; Admissible = $false }
)) {
    $verdict = Test-TtsS1FormalShardTopology -ShardCount $case.Shards -HostLogicalCpus $case.Cpus
    Assert-True ($verdict.admissible -eq $case.Admissible) "a $($case.Shards)-shard run on $($case.Cpus) logical CPUs is admissible=$($case.Admissible)"
    Assert-True ($verdict.required_logical_cpus -eq (2 * $case.Shards)) "the requirement for $($case.Shards) shards is $(2 * $case.Shards) logical CPUs"
}
$verdict = Test-TtsS1FormalShardTopology -ShardCount 4 -HostLogicalCpus 256
Assert-True ($verdict.reason -like '*not the pinned 8*') 'a non-pinned count is refused by name'
$verdict = Test-TtsS1FormalShardTopology -ShardCount 8 -HostLogicalCpus 15
Assert-True ($verdict.reason -like '*15 logical CPUs*') 'a cramped host is refused by its own CPU count'
Assert-True ($verdict.reason -like '*below the 16*') 'the cramped-host refusal names the requirement'
$verdict = Test-TtsS1FormalShardTopology -ShardCount 4 -HostLogicalCpus 2
Assert-True ($verdict.reason -like '*not the pinned 8*') 'both failing clauses are named, the count'
Assert-True ($verdict.reason -like '*2 logical CPUs*') 'both failing clauses are named, the host'

# THE LAUNCHER PATH. The wrapper obtains every tier report through
# Read-TtsS1TierReport, which validates before returning, so a report
# missing the nested block can never reach the dereferences that build the
# tier summary. Were the read and the assertion separable, that report would
# die with a bare strict-mode PropertyNotFoundException naming neither the
# tier nor the field.
$contractSandbox = New-TtsS1TestRoot
try {
    $reportPath = Join-Path $contractSandbox 'tier-t512.report.json'
    $json = New-TtsS1TestReport -OmitLatencyCurve | ConvertTo-Json -Depth 8
    [System.IO.File]::WriteAllText($reportPath, $json, [System.Text.UTF8Encoding]::new($false))

    $launcherMessage = $null
    try { [void](Read-TtsS1TierReport -Tier 't512' -Path $reportPath) }
    catch { $launcherMessage = $_.Exception.Message }
    Assert-True ($null -ne $launcherMessage) 'the launcher read path rejects a report with no latency_curve block'
    Assert-True ($launcherMessage -like '*missing*body.compute_cap.latency_curve.rule*') 'the launcher read path names the missing nested path'
    Assert-True (-not ($launcherMessage -like '*PropertyNotFound*')) 'the launcher read path does not surface a bare strict-mode error'

    # And a well-formed report still comes back through the same call.
    $validPath = Join-Path $contractSandbox 'tier-t2048.report.json'
    $json = New-TtsS1TestReport | ConvertTo-Json -Depth 8
    [System.IO.File]::WriteAllText($validPath, $json, [System.Text.UTF8Encoding]::new($false))
    $returned = Read-TtsS1TierReport -Tier 't2048' -Path $validPath
    Assert-True ($returned.body.compute_cap.latency_curve.rule -ceq $script:TtsS1LatencyCurveRule) 'the launcher read path returns a validated report'

    # The wrapper really does obtain its tier reports that way, and no
    # longer carries a second, separable assertion it could drift from.
    Assert-True ($wrapperText -like '*Read-TtsS1TierReport -Tier $plan.tier -Path $plan.report_path*') 'the wrapper reads every tier report through the validating reader'
    Assert-True (-not ($wrapperText -like '*Read-TtsS1Json -Path $plan.report_path*')) 'the wrapper does not read a tier report unvalidated'
}
finally {
    if (Test-Path -LiteralPath $contractSandbox) {
        Remove-Item -LiteralPath $contractSandbox -Recurse -Force -ErrorAction SilentlyContinue
    }
}

$sandbox = New-TtsS1TestRoot
try {
    $corpusExe = New-TtsS1StandIn -Path (Join-Path $sandbox 'tts_s1_corpus_v1.exe')
    $replayExe = New-TtsS1StandIn -Path (Join-Path $sandbox 'tts_s1_replay_v1.exe')
    $storeRoot = Join-Path $sandbox 'store'
    New-Item -ItemType Directory -Force -Path $storeRoot | Out-Null

    $base = @{
        StoreKind = 'population'
        StoreRoot = $storeRoot
        Generation = [uint64]1024
        CorpusExecutable = $corpusExe
        ReplayExecutable = $replayExe
        CorpusSeedBlock = 0
        ReplaySeedBlock = 1
        Episodes = [uint64]64
        DryRun = $true
        SkipHostAssertions = $true
    }

    # --- 1. A full-ladder dry run plans everything and launches nothing.
    $evidence = Join-Path $sandbox 'evidence-full'
    $parameters = $base.Clone()
    $parameters['EvidenceRoot'] = $evidence
    $result = Invoke-Wrapper -Parameters $parameters
    Assert-True ($null -eq $result.Failure) "a full-ladder dry run succeeds ($($result.Failure))"
    $attempt = Get-OnlyAttemptRoot -EvidenceRoot $evidence

    Assert-True (Test-Path -LiteralPath (Join-Path $attempt 'provenance.json') -PathType Leaf) 'the dry run writes provenance.json'
    Assert-True (Test-Path -LiteralPath (Join-Path $attempt 'result.json') -PathType Leaf) 'the dry run writes result.json'
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $attempt 'TTS_S1_COMPLETE'))) 'the dry run writes no completion marker'
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $attempt 'RUN_FAILED'))) 'the dry run writes no failure marker'
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $attempt 'corpus.json'))) 'the dry run builds no corpus'
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $attempt 'summary.json'))) 'the dry run writes no summary'
    $reports = @(Get-ChildItem -LiteralPath $attempt -Filter 'tier-*.report.json' -ErrorAction SilentlyContinue)
    Assert-True ($reports.Count -eq 0) 'the dry run writes no tier report'
    $diagnostics = @(Get-ChildItem -LiteralPath $attempt -Filter 'tier-*.diagnostics' -ErrorAction SilentlyContinue)
    Assert-True ($diagnostics.Count -eq 0) 'the dry run creates no diagnostics directory'
    $shardDirs = @(Get-ChildItem -LiteralPath $attempt -Filter 'tier-*.shards' -ErrorAction SilentlyContinue)
    Assert-True ($shardDirs.Count -eq 0) 'the dry run creates no shard directory'
    $shardReports = @(Get-ChildItem -LiteralPath $attempt -Filter 'shard-*.report.json' -Recurse -ErrorAction SilentlyContinue)
    Assert-True ($shardReports.Count -eq 0) 'the dry run publishes no shard report'

    $resultJson = Get-Content -LiteralPath (Join-Path $attempt 'result.json') -Raw | ConvertFrom-Json
    Assert-True ($resultJson.status -ceq 'DRY_RUN_PLANNED') 'result.json says DRY_RUN_PLANNED'
    Assert-True ($resultJson.planned_tier_merge_commands.Count -eq 4) 'the whole ladder is planned'
    Assert-True ($resultJson.planned_tier_shard_commands.Count -eq 4 * $defaultShardCount) 'every tier plans one command per shard'
    Assert-True ($resultJson.shard_count_requested -eq $defaultShardCount) 'result.json records the requested shard count'

    $provenanceJson = Get-Content -LiteralPath (Join-Path $attempt 'provenance.json') -Raw | ConvertFrom-Json
    Assert-True ($provenanceJson.dry_run -eq $true) 'provenance.json records the dry run'
    Assert-True ($provenanceJson.formal_ladder -eq $true) 'a whole-corpus full-ladder run plans as FORMAL'
    Assert-True ($provenanceJson.shard_count_requested -eq $defaultShardCount) 'provenance.json records the requested shard count'
    Assert-True ($provenanceJson.shard_assignment_rule -ceq $script:TtsS1ShardAssignmentRule) 'provenance.json records the shard assignment rule'
    Assert-True ($provenanceJson.shard_count_rule -like '*min-of-requested-and-planned-contributing-episodes*') 'provenance.json states the clamp rule'
    # Every shard's invocation as its own record, each naming the bin and
    # its hash, so the provenance alone says which binary each of the K
    # processes was to be.
    $plannedShards = @($provenanceJson.planned_tier_shards)
    Assert-True ($plannedShards.Count -eq 4 * $defaultShardCount) 'provenance.json records one invocation record per shard'
    Assert-True (@($plannedShards | Where-Object { $_.tier -ceq 't512' }).Count -eq $defaultShardCount) 'every tier gets its own shard records'
    Assert-True (@($plannedShards | Where-Object { $_.executable_sha256 -cne $provenanceJson.replay_executable.sha256 }).Count -eq 0) 'every shard record names the hashed replay bin'
    Assert-True (@($plannedShards | Where-Object { $_.shard_count -ne $defaultShardCount }).Count -eq 0) 'every shard record carries the fan-out it belongs to'
    Assert-True ((@($plannedShards | Where-Object { $_.tier -ceq 't512' } | ForEach-Object { $_.shard_index }) -join ',') -ceq ((0..($defaultShardCount - 1)) -join ',')) 'a tier shard records run 0 through K-1 once each'

    # The provenance record states the whole pinned contract, so a dry run
    # already says which rules a real launch would accept.
    Assert-True ($provenanceJson.pinned_contract.compute_cap_rule -like '*-isotonic-per-ordinal-protocol-latency-curve-fitted-to-whole-episode-timings*') 'the pinned compute-cap rule is the isotonic one'
    Assert-True ($provenanceJson.pinned_contract.compute_cap_rule -like '*/v2') 'the pinned compute-cap rule is V2'
    Assert-True ($provenanceJson.pinned_contract.latency_curve_rule -like '*-at-the-maximum-rise-between-adjacent-fitted-ordinals*') 'the pinned latency-curve rule names the adjacent-rise extrapolation'
    Assert-True ($provenanceJson.pinned_contract.latency_curve_rule -like '*/v2') 'the pinned latency-curve rule is V2'
    Assert-True ($provenanceJson.pinned_contract.verdict_view -ceq 'corpus_target_view') 'the pinned gating view is the corpus targets'
    Assert-True ($wrapperText -like "*. (Join-Path `$PSScriptRoot 'common.ps1')*") 'the launcher loads the shared pinned contract beside it'
    Assert-True ($provenanceJson.corpus_executable.sha256.Length -eq 64) 'the corpus bin is hashed'
    Assert-True ($provenanceJson.replay_executable.sha256.Length -eq 64) 'the replay bin is hashed'
    Assert-True ($null -eq $provenanceJson.git) 'the git record is skipped under -SkipHostAssertions'

    $corpusCommand = $provenanceJson.planned_corpus_command
    foreach ($fragment in @('--population-store-root', '--generation 1024', '--seed-block 0', '--episodes 64', '--output')) {
        Assert-True ($corpusCommand -like "*$fragment*") "the corpus command carries $fragment"
    }
    Assert-True (-not ($corpusCommand -like '*--tier*')) 'the corpus command carries no tier'

    $ladder = @('t512', 't2048', 't8192', 't32768')
    for ($index = 0; $index -lt $ladder.Count; $index++) {
        $tier = $ladder[$index]
        # The shard commands are flattened in ladder order, K per tier.
        $tierShardCommands = @($provenanceJson.planned_tier_shard_commands |
            Select-Object -Skip ($index * $defaultShardCount) -First $defaultShardCount)
        Assert-True ($tierShardCommands.Count -eq $defaultShardCount) "tier $tier plans $defaultShardCount shards in ladder position $index"
        for ($shard = 0; $shard -lt $defaultShardCount; $shard++) {
            $line = $tierShardCommands[$shard]
            $padded = '{0:0000}' -f $shard
            Assert-True ($line -like "*--tier $tier*") "tier $tier shard $shard names its tier"
            Assert-True ($line -like '*--seed-block 1*') "tier $tier shard $shard uses the replay seed block"
            Assert-True ($line -like '*--corpus*') "tier $tier shard $shard consumes the corpus"
            Assert-True (-not ($line -like '*--limit-episodes*')) "tier $tier shard $shard has no smoke bound by default"
            Assert-True ($line -like '*--max-episodes 64*') "tier $tier shard $shard carries the corpus episode count as its guard"
            Assert-True ($line -like "*--shard-index $shard --shard-count $defaultShardCount*") "tier $tier shard $shard carries both shard flags"
            Assert-True ($line -like "*--diagnostics-dir*tier-$tier.diagnostics*shard-$padded*") "tier $tier shard $shard gets its own diagnostics directory"
            $shardReportPattern = "*shard-{0}-of-{1:0000}.report.json*" -f $padded, $defaultShardCount
            Assert-True ($line -like $shardReportPattern) "tier $tier shard $shard publishes under the name the merge derives"
            Assert-True (-not ($line -like '*--merge-shards*')) "tier $tier shard $shard is not a merge"
        }
        # ONE merge per tier, naming the shard directory and the same count,
        # and carrying no replay-only flag: the merge loads no checkpoint.
        $merge = $provenanceJson.planned_tier_merge_commands[$index]
        Assert-True ($merge -like "*--merge-shards*tier-$tier.shards*") "tier $tier merges its own shard directory"
        Assert-True ($merge -like "*--shard-count $defaultShardCount*") "tier $tier merges exactly the shards it planned"
        Assert-True ($merge -like "*--output*tier-$tier.report.json*") "tier $tier merges into its tier report"
        foreach ($forbidden in @('--tier', '--corpus', '--seed-block', '--max-episodes', '--diagnostics-dir', '--shard-index', '--population-store-root')) {
            Assert-True (-not ($merge -like "*$forbidden*")) "tier $tier merge carries no $forbidden"
        }
    }

    # --- 1a2. THE FORMAL TOPOLOGY, end to end through the launcher. The
    #          provenance records the pin, the host it read, and whether a
    #          formal run is admissible here, and that verdict agrees with
    #          the shared rule evaluated on the same host.
    $observedCpus = [System.Environment]::ProcessorCount
    $expectedFormal = Test-TtsS1FormalShardTopology -ShardCount $defaultShardCount -HostLogicalCpus $observedCpus
    Assert-True ($provenanceJson.formal_shard_count -eq $script:TtsS1FormalShardCount) 'provenance.json records the pinned formal shard count'
    Assert-True ($provenanceJson.formal_logical_cpus_per_shard -eq $script:TtsS1FormalLogicalCpusPerShard) 'provenance.json records the pinned CPUs per shard'
    Assert-True ($provenanceJson.host_logical_cpus -eq $observedCpus) 'provenance.json records the host logical CPU count it read'
    Assert-True ($provenanceJson.formal_topology_admissible -eq $expectedFormal.admissible) 'provenance.json applies the shared formal-topology rule to this host'
    Assert-True ($provenanceJson.formal_topology_required_logical_cpus -eq (2 * $defaultShardCount)) 'provenance.json states the CPU requirement it checked against'
    Assert-True ($provenanceJson.pinned_contract.shard_topology_rule -ceq $script:TtsS1ShardTopologyRule) 'the pinned contract carries the shard-topology rule'
    Assert-True ($provenanceJson.pinned_contract.formal_shard_count -eq 8) 'the pinned contract carries the formal shard count'
    # A full-ladder whole-corpus run at the pinned count plans as FORMAL
    # exactly when this host can host the topology.
    Assert-True ($provenanceJson.formal_ladder -eq $true) 'a whole-corpus full-ladder run at the pinned count plans as FORMAL'
    # A REAL launch on a host too small is refused rather than demoted; a
    # dry run measures nothing, so it plans and says what would be refused.
    Assert-True ($wrapperText -like '*a formal S1 run is refused on this host*') 'the launcher refuses a formal run the host cannot host'
    Assert-True ($wrapperText -like '*TTS_S1_FORMAL_TOPOLOGY_REFUSED*') 'a dry run reports the refusal instead of throwing'
    Assert-True ($wrapperText -like '*else {*') 'the refusal has a real-launch branch'
    Assert-True ($wrapperText -like '*$isFormalLadder = ($LimitEpisodes -eq 0)*') 'the formal ladder is still decided from the corpus and the tiers'
    Assert-True ($wrapperText -like '*-and ($ShardCount -eq $script:TtsS1FormalShardCount)*') 'the shard count is part of what makes a ladder formal'
    Assert-True ($wrapperText -like '*-RequireFormalShardTopology:$isFormalTopology*') 'the launcher demands the formal topology of a report only when the run still claims it'
    Assert-True ($wrapperText -like '*$everyTierFormalTopology*') 'the tier reports get the last word on the topology'

    # --- 1a3. THE SMOKE DEMOTION. Any other -ShardCount is a smoke, and
    #          says so before anything runs.
    foreach ($count in @(1, 4, 16)) {
        $evidence = Join-Path $sandbox ("evidence-smoke-shards-{0}" -f $count)
        $parameters = $base.Clone()
        $parameters['EvidenceRoot'] = $evidence
        $parameters['ShardCount'] = $count
        $result = Invoke-Wrapper -Parameters $parameters
        Assert-True ($null -eq $result.Failure) "a full-ladder dry run at -ShardCount $count succeeds ($($result.Failure))"
        $attemptShards = Get-OnlyAttemptRoot -EvidenceRoot $evidence
        $shardProvenance = Get-Content -LiteralPath (Join-Path $attemptShards 'provenance.json') -Raw | ConvertFrom-Json
        Assert-True ($shardProvenance.formal_ladder -eq $false) "a whole-corpus full ladder at -ShardCount $count is a SMOKE"
        Assert-True ($shardProvenance.formal_topology_admissible -eq $false) "-ShardCount $count is never an admissible formal topology"
        Assert-True ($shardProvenance.formal_topology_reason -like '*not the pinned 8*') "-ShardCount $count is refused formal standing by name"
        Assert-True ($shardProvenance.shard_count_requested -eq $count) "provenance records the requested -ShardCount $count"
    }

    # --- 1b. The shard fan-out is a knob, and the whole ladder of legal
    #         values plans; illegal ones are refused by the parameter
    #         itself rather than discovered at the first shard.
    foreach ($count in @(1, 2, 64)) {
        $evidence = Join-Path $sandbox ("evidence-shards-{0}" -f $count)
        $parameters = $base.Clone()
        $parameters['EvidenceRoot'] = $evidence
        $parameters['ShardCount'] = $count
        $parameters['Tiers'] = @('t512')
        $result = Invoke-Wrapper -Parameters $parameters
        Assert-True ($null -eq $result.Failure) "a dry run at -ShardCount $count succeeds ($($result.Failure))"
        $attempt = Get-OnlyAttemptRoot -EvidenceRoot $evidence
        $shardJson = Get-Content -LiteralPath (Join-Path $attempt 'provenance.json') -Raw | ConvertFrom-Json
        $lines = @($shardJson.planned_tier_shard_commands)
        Assert-True ($lines.Count -eq $count) "-ShardCount $count plans $count shard commands for one tier"
        Assert-True (@($shardJson.planned_tier_merge_commands).Count -eq 1) "-ShardCount $count still plans exactly one merge per tier"
        Assert-True ($shardJson.planned_tier_merge_commands[0] -like "*--shard-count $count*") "-ShardCount $count reaches the merge"
        Assert-True ($lines[0] -like "*--shard-index 0 --shard-count $count*") "-ShardCount $count names shard 0"
        Assert-True ($lines[$count - 1] -like "*--shard-index $($count - 1) --shard-count $count*") "-ShardCount $count names its last shard"
    }
    foreach ($count in @(0, 65, -1)) {
        $parameters = $base.Clone()
        $parameters['EvidenceRoot'] = (Join-Path $sandbox ("evidence-shards-bad-{0}" -f ($count + 2)))
        $parameters['ShardCount'] = $count
        $result = Invoke-Wrapper -Parameters $parameters
        Assert-True ($null -ne $result.Failure) "the wrapper rejects -ShardCount $count"
    }

    # --- 2. -LimitEpisodes is threaded to every tier when set.
    $evidence = Join-Path $sandbox 'evidence-limit'
    $parameters = $base.Clone()
    $parameters['EvidenceRoot'] = $evidence
    $parameters['LimitEpisodes'] = [uint64]8
    $parameters['Tiers'] = @('t512')
    $result = Invoke-Wrapper -Parameters $parameters
    Assert-True ($null -eq $result.Failure) "a single-tier dry run succeeds ($($result.Failure))"
    $attempt = Get-OnlyAttemptRoot -EvidenceRoot $evidence
    $provenanceJson = Get-Content -LiteralPath (Join-Path $attempt 'provenance.json') -Raw | ConvertFrom-Json
    Assert-True (@($provenanceJson.planned_tier_merge_commands).Count -eq 1) 'a tier subset plans only those tiers'
    Assert-True (@($provenanceJson.planned_tier_shard_commands).Count -eq $defaultShardCount) 'a tier subset still plans one command per shard'
    foreach ($line in $provenanceJson.planned_tier_shard_commands) {
        Assert-True ($line -like '*--limit-episodes 8*') 'the smoke bound reaches every shard command'
    }
    Assert-True (-not ($provenanceJson.planned_tier_merge_commands[0] -like '*--limit-episodes*')) 'the smoke bound is not a merge flag'
    Assert-True ($provenanceJson.formal_ladder -eq $false) 'a bounded single-tier run plans as a SMOKE'

    # --- 2b. A full ladder with -LimitEpisodes is still a smoke, and so
    #         is a whole-corpus run over a tier subset.
    $evidence = Join-Path $sandbox 'evidence-smoke-limit'
    $parameters = $base.Clone()
    $parameters['EvidenceRoot'] = $evidence
    $parameters['LimitEpisodes'] = [uint64]4
    $result = Invoke-Wrapper -Parameters $parameters
    Assert-True ($null -eq $result.Failure) "a bounded full-ladder dry run succeeds ($($result.Failure))"
    $attempt = Get-OnlyAttemptRoot -EvidenceRoot $evidence
    $provenanceJson = Get-Content -LiteralPath (Join-Path $attempt 'provenance.json') -Raw | ConvertFrom-Json
    Assert-True ($provenanceJson.formal_ladder -eq $false) 'a bounded full ladder is a SMOKE'

    $evidence = Join-Path $sandbox 'evidence-smoke-subset'
    $parameters = $base.Clone()
    $parameters['EvidenceRoot'] = $evidence
    $parameters['Tiers'] = @('t512', 't2048')
    $result = Invoke-Wrapper -Parameters $parameters
    Assert-True ($null -eq $result.Failure) "a tier-subset dry run succeeds ($($result.Failure))"
    $attempt = Get-OnlyAttemptRoot -EvidenceRoot $evidence
    $provenanceJson = Get-Content -LiteralPath (Join-Path $attempt 'provenance.json') -Raw | ConvertFrom-Json
    Assert-True ($provenanceJson.formal_ladder -eq $false) 'a whole-corpus tier SUBSET is a SMOKE'

    # --- 2c. Windows argument quoting: a path with a space must reach the
    #         child as ONE argument, which means it is quoted in the
    #         planned command line the wrapper also executes.
    $spacedStore = Join-Path $sandbox 'store with space'
    New-Item -ItemType Directory -Force -Path $spacedStore | Out-Null
    $spacedExe = New-TtsS1StandIn -Path (Join-Path $sandbox 'tts s1 corpus.exe')
    $evidence = Join-Path $sandbox 'evidence with space'
    $parameters = $base.Clone()
    $parameters['EvidenceRoot'] = $evidence
    $parameters['StoreRoot'] = $spacedStore
    $parameters['CorpusExecutable'] = $spacedExe
    $parameters['Tiers'] = @('t512')
    $result = Invoke-Wrapper -Parameters $parameters
    Assert-True ($null -eq $result.Failure) "a dry run with spaces in every path succeeds ($($result.Failure))"
    $attempt = Get-OnlyAttemptRoot -EvidenceRoot $evidence
    $provenanceJson = Get-Content -LiteralPath (Join-Path $attempt 'provenance.json') -Raw | ConvertFrom-Json
    $corpusCommand = $provenanceJson.planned_corpus_command
    Assert-True ($corpusCommand -like "*`"$spacedStore`"*") 'a store root with a space is quoted in the corpus command'
    Assert-True ($corpusCommand -like "*`"$spacedExe`"*") 'an executable path with a space is quoted'
    Assert-True ($corpusCommand -like '*"*corpus.json"*') 'an output path under a spaced evidence root is quoted'
    $tierCommand = $provenanceJson.planned_tier_shard_commands[0]
    Assert-True ($tierCommand -like '*"*tier-t512.diagnostics*shard-0000"*') 'a per-shard diagnostics path under a spaced evidence root is quoted'
    Assert-True ($tierCommand -like '*"*tier-t512.shards*shard-0000-of-0008.report.json"*') 'a shard report path under a spaced evidence root is quoted'
    $mergeCommand = $provenanceJson.planned_tier_merge_commands[0]
    Assert-True ($mergeCommand -like '*"*tier-t512.shards"*') 'the merge shard directory under a spaced evidence root is quoted'
    Assert-True ($mergeCommand -like '*--shard-count 8*') 'the merge shard count is not quoted'
    # A flag with no space is NOT quoted: the quoter only quotes what needs
    # it, so a reviewer reading a command line sees the flags plainly.
    Assert-True ($corpusCommand -like '*--seed-block 0*') 'flags and simple values stay unquoted'

    # --- 3. The original and portable authority shapes.
    $evidence = Join-Path $sandbox 'evidence-original'
    $parameters = $base.Clone()
    $parameters['EvidenceRoot'] = $evidence
    $parameters['StoreKind'] = 'original'
    $parameters.Remove('Generation')
    $parameters['Tiers'] = @('t512')
    $result = Invoke-Wrapper -Parameters $parameters
    Assert-True ($null -eq $result.Failure) "the original authority plans ($($result.Failure))"
    $attempt = Get-OnlyAttemptRoot -EvidenceRoot $evidence
    $provenanceJson = Get-Content -LiteralPath (Join-Path $attempt 'provenance.json') -Raw | ConvertFrom-Json
    Assert-True ($provenanceJson.planned_corpus_command -like '*--original-store-root*') 'the original authority names its own flag'
    Assert-True (-not ($provenanceJson.planned_corpus_command -like '*--generation*')) 'the pinned g384 authority takes no generation'
    Assert-True ($provenanceJson.planned_tier_shard_commands[0] -like '*--original-store-root*') 'every shard carries the same authority as the corpus build'

    $evidence = Join-Path $sandbox 'evidence-portable'
    $parameters = $base.Clone()
    $parameters['EvidenceRoot'] = $evidence
    $parameters['StoreKind'] = 'portable'
    $parameters.Remove('Generation')
    $parameters['Tiers'] = @('t512')
    $result = Invoke-Wrapper -Parameters $parameters
    Assert-True ($null -eq $result.Failure) "the portable authority plans ($($result.Failure))"
    $attempt = Get-OnlyAttemptRoot -EvidenceRoot $evidence
    $provenanceJson = Get-Content -LiteralPath (Join-Path $attempt 'provenance.json') -Raw | ConvertFrom-Json
    Assert-True ($provenanceJson.planned_corpus_command -like '*--portable-derivative-root*') 'the portable authority names its own flag'

    # --- 4. Every rejection.
    $rejections = @(
        @{ Name = 'equal seed blocks'; Mutate = { param($p) $p['ReplaySeedBlock'] = $p['CorpusSeedBlock'] } },
        @{ Name = 'a reordered ladder'; Mutate = { param($p) $p['Tiers'] = @('t2048', 't512') } },
        @{ Name = 'a duplicated tier'; Mutate = { param($p) $p['Tiers'] = @('t512', 't512') } },
        @{ Name = 'a population Store with no generation'; Mutate = { param($p) $p.Remove('Generation') } },
        @{ Name = 'a portable authority with a generation'; Mutate = { param($p) $p['StoreKind'] = 'portable' } },
        @{ Name = 'a missing corpus executable'; Mutate = { param($p) $p['CorpusExecutable'] = (Join-Path $sandbox 'absent.exe') } },
        @{ Name = 'a missing store root'; Mutate = { param($p) $p['StoreRoot'] = (Join-Path $sandbox 'absent-store') } },
        @{ Name = '-SkipHostAssertions without -DryRun'; Mutate = { param($p) $p['DryRun'] = $false } }
    )
    # Every rejection below must also hold when paths contain spaces, so a
    # quoting bug cannot turn a rejection into a silent acceptance.
    $index = 0
    foreach ($rejection in $rejections) {
        $parameters = $base.Clone()
        $parameters['EvidenceRoot'] = (Join-Path $sandbox ("evidence-reject-{0}" -f $index))
        & $rejection.Mutate $parameters
        $result = Invoke-Wrapper -Parameters $parameters
        Assert-True ($null -ne $result.Failure) "the wrapper rejects $($rejection.Name)"
        $index++
    }

    # --- 4b. THE SHARD LOOP'S SHAPE, asserted against the wrapper's own
    #         text. Each of these is a way the fan-out could silently become
    #         a serial run or leave orphaned processes behind, and none of
    #         them would fail any other check here. The primitives
    #         themselves are exercised for real in section 4c.
    #
    #         Every shard is started in one loop and waited on in a SECOND
    #         loop. Starting and waiting inside one loop is the old serial
    #         run with more processes, and it is the single most likely way
    #         to lose the parallelism without losing anything else. That
    #         ordering now lives in the shared fan-out, so it is asserted
    #         there and exercised for real in 4c.
    $commonText = [System.IO.File]::ReadAllText($script:TtsS1CommonPath)
    $startIndex = $commonText.IndexOf('process = Start-TtsS1Process -FilePath $job.file_path')
    $waitIndex = $commonText.IndexOf('exit_code = Wait-TtsS1Process -Process $entry.process')
    Assert-True ($startIndex -gt 0) 'the fan-out starts every job through the non-blocking start'
    Assert-True ($waitIndex -gt $startIndex) 'the fan-out waits on the jobs only after starting them all'
    Assert-True ($commonText -like '*foreach ($entry in $running)*') 'the fan-out wait is its own loop over the started processes'
    Assert-True ($commonText -like '*finally {*') 'the fan-out reaps in a finally, so an unwinding start leaves nothing running'
    Assert-True ($commonText -like '*Stop-Process -Id $id -Force*') 'the fan-out stops anything still alive'
    # ONE start path and ONE reap path: the single-child runner goes
    # through the same fan-out, so a corpus build or a merge cannot be
    # orphaned by a route that skips the reaping.
    # .Contains rather than -like: the literal carries [ and ], which -like
    # would read as a wildcard character class.
    Assert-True ($commonText.Contains('$fanOut = Invoke-TtsS1ProcessFanOut -Jobs')) 'the single-child runner goes through the reaping fan-out'
    Assert-True ((([regex]::Matches($commonText, [regex]::Escape('Start-Process -FilePath'))).Count) -eq 1) 'exactly one place in the stack starts a child process'
    Assert-True ($wrapperText -like '*if ($shardFailures.Count -ne 0)*') 'the wrapper raises a shard failure only after every shard has been waited on'
    Assert-True ($wrapperText -like '*shard $($shard.index) exited with $shardExit*') 'the wrapper fails closed on a non-zero shard exit'
    # And nothing it started outlives it: the fan-out reaps in a finally and
    # the catch names what it killed in the attempt RUN_FAILED text.
    Assert-True ($wrapperText -like '*Invoke-TtsS1ProcessFanOut -Jobs*') 'the wrapper starts its shards through the reaping fan-out'
    Assert-True ($wrapperText -like '*Format-TtsS1StoppedChildren -Stopped @($script:TtsS1LastFanOutStopped)*') 'the wrapper records the reaped children in RUN_FAILED'
    Assert-True (-not ($wrapperText -like '*process = Start-TtsS1Process -FilePath $ReplayExecutable*')) 'the wrapper no longer starts shards outside the reaping fan-out'
    Assert-True ($wrapperText -like '*Invoke-TtsS1Process -FilePath $ReplayExecutable -Arguments $plan.merge_arguments*') 'the wrapper runs the merge through the same child runner'
    Assert-True ($wrapperText -like '*--merge-shards*') 'the wrapper plans the merge flag the bin declares'
    # And the merged report goes through the SAME validating reader the
    # unsharded report went through: sharding may not weaken the contract
    # check that gates a tier.
    Assert-True ($wrapperText -like '*Read-TtsS1TierReport -Tier $plan.tier -Path $plan.report_path*') 'the merged tier report is validated through the pinned-contract reader'
    Assert-True ($wrapperText -like '*$corpus.body.contributing_episode_count*') 'the wrapper sizes the run from the corpus stated contributing population'
    Assert-True ($wrapperText -like '*TTS_S1_TIER_COST*') 'the wrapper prints the expected per-tier cost before the tiers run'

    # --- 4c. THE PRIMITIVES THEMSELVES, run for real against cmd.exe.
    #
    #         These are the only child processes this file ever starts, and
    #         they are deliberately NOT either S1 bin: nothing here searches,
    #         no corpus is built, no CP7 panel is contacted and no GPU is
    #         touched. What they prove is the one property the fan-out rests
    #         on and that no planned command line can show: several children
    #         really do run at once, every one is waited on, and every exit
    #         code comes back, including a non-zero one.
    $processSandbox = New-TtsS1TestRoot
    try {
        $comspec = Join-Path $env:SystemRoot 'System32\cmd.exe'
        Assert-True (Test-Path -LiteralPath $comspec -PathType Leaf) 'cmd.exe is available to exercise the child-process primitives'
        # Three children, each sleeping about three seconds, started
        # together. Serialized they would take at least nine; run at once
        # they take about three, so the elapsed time is the evidence of
        # concurrency, and the ceiling sits between the two with room to
        # spare so a loaded host cannot turn it into a flake.
        #
        # Every argument is passed as its OWN element rather than as one
        # shell string, because the quoter would wrap a string with a space
        # in quotes and cmd only strips those when the quoted text names an
        # executable.
        $started = @()
        $watch = [System.Diagnostics.Stopwatch]::StartNew()
        for ($index = 0; $index -lt 3; $index++) {
            $started += Start-TtsS1Process -FilePath $comspec `
                -Arguments @('/c', 'ping', '-n', '4', '127.0.0.1', '>nul', '&', 'exit', [string]$index) `
                -StdoutPath (Join-Path $processSandbox ("child-{0}.stdout.txt" -f $index)) `
                -StderrPath (Join-Path $processSandbox ("child-{0}.stderr.txt" -f $index))
        }
        $codes = @()
        foreach ($process in $started) { $codes += Wait-TtsS1Process -Process $process }
        $watch.Stop()
        Assert-True ($codes.Count -eq 3) 'every started child is waited on'
        Assert-True ($codes[0] -eq 0) 'a successful child reports exit 0'
        Assert-True ($codes[1] -eq 1 -and $codes[2] -eq 2) 'each child reports its OWN exit code after the Refresh'
        Assert-True ($watch.Elapsed.TotalSeconds -lt 7.0) "the children ran concurrently rather than one after another ($([math]::Round($watch.Elapsed.TotalSeconds, 2)) s)"
        # And a non-zero exit is visible, which is what the launcher fails
        # closed on.
        $failing = Invoke-TtsS1Process -FilePath $comspec -Arguments @('/c', 'exit', '7') `
            -StdoutPath (Join-Path $processSandbox 'failing.stdout.txt') `
            -StderrPath (Join-Path $processSandbox 'failing.stderr.txt')
        Assert-True ($failing -eq 7) 'a non-zero child exit code reaches the caller'
        # THE FAIL-CLOSED CLAUSE, over a stand-in that reports no exit code
        # at all. This is not hypothetical: PowerShell 5.1 hands back
        # exactly such an object unless the started process's handle is
        # retained, and a [int] cast would silently turn it into 0, which
        # is the value that means the child succeeded.
        $noExitCode = [pscustomobject]@{ ExitCode = $null }
        $noExitCode | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { }
        $noExitCode | Add-Member -MemberType ScriptMethod -Name Refresh -Value { }
        $unknownExit = $null
        try { [void](Wait-TtsS1Process -Process $noExitCode) }
        catch { $unknownExit = $_.Exception.Message }
        Assert-True ($null -ne $unknownExit) 'an unreadable exit code is refused rather than cast to a success'
        Assert-True ($unknownExit -like '*never a success*') 'the refusal says why an unknown exit is not a pass'

        # THE ORPHAN CASE, which is why the fan-out reaps in a finally.
        # Starting K children is K chances to throw, and a throw on the
        # third start leaves the first two running: if the launcher then
        # exits, they outlive the run that owns them, still searching and
        # still writing into the attempt's directories. Shard 2 of 3 is
        # given an executable that does not exist, and shards 0 and 1 are
        # long-lived cmd.exe children that would otherwise sit there for
        # half a minute after the launcher was gone.
        $longLived = @('/c', 'ping', '-n', '30', '127.0.0.1', '>nul')
        $absent = Join-Path $processSandbox 'no-such-binary-for-shard-2.exe'
        Assert-True (-not (Test-Path -LiteralPath $absent)) 'the failing shard names an executable that really is absent'
        $fanJobs = @(
            [pscustomobject]@{ label = 'shard-0'; file_path = $comspec; arguments = $longLived; stdout_path = (Join-Path $processSandbox 'fan-0.out'); stderr_path = (Join-Path $processSandbox 'fan-0.err') },
            [pscustomobject]@{ label = 'shard-1'; file_path = $comspec; arguments = $longLived; stdout_path = (Join-Path $processSandbox 'fan-1.out'); stderr_path = (Join-Path $processSandbox 'fan-1.err') },
            [pscustomobject]@{ label = 'shard-2'; file_path = $absent; arguments = @('/c'); stdout_path = (Join-Path $processSandbox 'fan-2.out'); stderr_path = (Join-Path $processSandbox 'fan-2.err') }
        )
        $fanFailure = $null
        $fanWatch = [System.Diagnostics.Stopwatch]::StartNew()
        try { [void](Invoke-TtsS1ProcessFanOut -Jobs $fanJobs) }
        catch { $fanFailure = $_.Exception.Message }
        $fanWatch.Stop()
        Assert-True ($null -ne $fanFailure) 'a shard that cannot be started raises'
        $reaped = @($script:TtsS1LastFanOutStopped)
        Assert-True ($reaped.Count -eq 2) 'both already-started shards are reaped when a later start fails'
        Assert-True ((@($reaped | ForEach-Object { $_.label }) -join ',') -ceq 'shard-0,shard-1') 'shards 0 and 1 are the ones stopped'
        # Stopped for real, not merely recorded: neither pid is alive, and
        # the reap did not wait out the thirty-second ping.
        foreach ($entry in $reaped) {
            $stillAlive = $null
            try { $stillAlive = Get-Process -Id $entry.process_id -ErrorAction Stop } catch { $stillAlive = $null }
            Assert-True ($null -eq $stillAlive) "the reaped child $($entry.label) (pid $($entry.process_id)) is gone"
        }
        Assert-True ($fanWatch.Elapsed.TotalSeconds -lt 20.0) "the reap killed the children rather than waiting them out ($([math]::Round($fanWatch.Elapsed.TotalSeconds, 2)) s)"
        # The launcher turns that list into the RUN_FAILED text, so an
        # operator reading it knows nothing was left behind.
        $stoppedClause = Format-TtsS1StoppedChildren -Stopped $reaped
        Assert-True ($stoppedClause -like '*stopped still-running children*') 'the reaped children are named for the RUN_FAILED text'
        Assert-True ($stoppedClause -like '*shard-0*' -and $stoppedClause -like '*shard-1*') 'the RUN_FAILED clause names each reaped shard'
        Assert-True ((Format-TtsS1StoppedChildren -Stopped @()) -ceq '') 'a clean fan-out contributes no clause'

        # A fan-out that finishes normally reaps nothing, so the list is not
        # a leftover from the failure above.
        $clean = Invoke-TtsS1ProcessFanOut -Jobs @(
            [pscustomobject]@{ label = 'ok-0'; file_path = $comspec; arguments = @('/c', 'exit', '0'); stdout_path = (Join-Path $processSandbox 'clean-0.out'); stderr_path = (Join-Path $processSandbox 'clean-0.err') },
            [pscustomobject]@{ label = 'ok-1'; file_path = $comspec; arguments = @('/c', 'exit', '5'); stdout_path = (Join-Path $processSandbox 'clean-1.out'); stderr_path = (Join-Path $processSandbox 'clean-1.err') }
        )
        Assert-True (@($script:TtsS1LastFanOutStopped).Count -eq 0) 'a fan-out that ran to completion reaped nothing'
        Assert-True ($clean.results.Count -eq 2) 'a completed fan-out returns one result per job'
        Assert-True ($clean.results[0].exit_code -eq 0 -and $clean.results[1].exit_code -eq 5) 'a completed fan-out returns each job own exit code'
        Assert-True (($clean.results[0].label -ceq 'ok-0') -and ($clean.results[1].label -ceq 'ok-1')) 'a completed fan-out returns results in job order'
        # The quoter is what carries a spaced path through to the child, so
        # it is checked against the parser it is written for.
        $echoOut = Join-Path $processSandbox 'echo.stdout.txt'
        [void](Invoke-TtsS1Process -FilePath $comspec -Arguments @('/c', 'echo', 'a b') `
                -StdoutPath $echoOut -StderrPath (Join-Path $processSandbox 'echo.stderr.txt'))
        Assert-True (([System.IO.File]::ReadAllText($echoOut)).Trim() -ceq '"a b"') 'an argument containing a space reaches the child as one quoted argument'
    }
    finally {
        if (Test-Path -LiteralPath $processSandbox) {
            Remove-Item -LiteralPath $processSandbox -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    # --- 5. Neither S1 BIN was ever started by this file: the two stand-ins
    #        are still exactly the text files we wrote.
    foreach ($path in @($corpusExe, $replayExe)) {
        $text = [System.IO.File]::ReadAllText($path)
        Assert-True ($text -like 'not an executable*') "the stand-in at $path was never replaced or executed"
    }
}
finally {
    if (Test-Path -LiteralPath $sandbox) {
        Remove-Item -LiteralPath $sandbox -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Write-Output ""
Write-Output ("TTS_S1_DRY_RUN_TESTS checks={0} failures={1}" -f $script:Checks, $script:Failures)
if ($script:Failures -ne 0) { exit 1 }
exit 0
