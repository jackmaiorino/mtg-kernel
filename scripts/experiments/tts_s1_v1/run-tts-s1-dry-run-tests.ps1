<#
.SYNOPSIS
Dry-run tests for run-tts-s1.ps1. Launches nothing.

.DESCRIPTION
Every case runs the wrapper with -DryRun -SkipHostAssertions against a
throwaway evidence root under the system temp directory, using two stand-in
executable FILES that are never executed (a dry run only hashes them). No
child process is started, no corpus is built, no search runs, no CP7 panel
is contacted, and no GPU is touched.

What it proves:
  * a dry run writes provenance.json and result.json with status
    DRY_RUN_PLANNED, and writes NEITHER terminal marker;
  * a dry run produces no corpus and no tier report;
  * the planned command lines carry exactly the flags the two bins declare,
    including the authority shape for each -StoreKind and the per-tier
    diagnostics directory the production writer publishes into;
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
# these tests exercise the SAME constants and the SAME function rather than
# re-implementing either. It defines constants and functions only.
. (Join-Path $PSScriptRoot 'common.ps1')
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
        [switch]$OmitLatencyCurve
    )
    $computeCap = [pscustomobject]@{ rule = $ProjectionRule }
    if (-not $OmitLatencyCurve) {
        $computeCap | Add-Member -NotePropertyName latency_curve -NotePropertyValue ([pscustomobject]@{ rule = $CurveRule })
    }
    return [pscustomobject]@{
        body = [pscustomobject]@{
            verdict_view = $VerdictView
            compute_cap = $computeCap
        }
    }
}

function Get-TtsS1ContractRejection {
    # The rejection message for a report the contract check refuses, or
    # $null if it was accepted. It asserts nothing itself: an Assert-True
    # inside here would put its own output on this function's output stream
    # and the caller would receive an array instead of the message.
    param([Parameter(Mandatory = $true)]$Report)
    try {
        Assert-TtsS1TierReportContract -Tier 't512' -Report $Report
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

    $resultJson = Get-Content -LiteralPath (Join-Path $attempt 'result.json') -Raw | ConvertFrom-Json
    Assert-True ($resultJson.status -ceq 'DRY_RUN_PLANNED') 'result.json says DRY_RUN_PLANNED'
    Assert-True ($resultJson.planned_tier_commands.Count -eq 4) 'the whole ladder is planned'

    $provenanceJson = Get-Content -LiteralPath (Join-Path $attempt 'provenance.json') -Raw | ConvertFrom-Json
    Assert-True ($provenanceJson.dry_run -eq $true) 'provenance.json records the dry run'
    Assert-True ($provenanceJson.formal_ladder -eq $true) 'a whole-corpus full-ladder run plans as FORMAL'

    # The provenance record states the whole pinned contract, so a dry run
    # already says which rules a real launch would accept.
    Assert-True ($provenanceJson.pinned_contract.compute_cap_rule -like '*-isotonic-per-ordinal-protocol-latency-curve-fitted-to-whole-episode-timings*') 'the pinned compute-cap rule is the isotonic one'
    Assert-True ($provenanceJson.pinned_contract.compute_cap_rule -like '*/v2') 'the pinned compute-cap rule is V2'
    Assert-True ($provenanceJson.pinned_contract.latency_curve_rule -like '*-at-the-maximum-rise-between-adjacent-fitted-ordinals*') 'the pinned latency-curve rule names the adjacent-rise extrapolation'
    Assert-True ($provenanceJson.pinned_contract.latency_curve_rule -like '*/v2') 'the pinned latency-curve rule is V2'
    Assert-True ($provenanceJson.pinned_contract.verdict_view -ceq 'corpus_target_view') 'the pinned gating view is the corpus targets'
    Assert-True ($wrapperText -like '*Assert-TtsS1TierReportContract -Tier $plan.tier -Report $report*') 'the launcher validates every tier report against the pinned contract'
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
        $line = $provenanceJson.planned_tier_commands[$index]
        Assert-True ($line -like "*--tier $($ladder[$index])*") "tier $($ladder[$index]) is planned in ladder position $index"
        Assert-True ($line -like '*--seed-block 1*') "tier $($ladder[$index]) uses the replay seed block"
        Assert-True ($line -like '*--corpus*') "tier $($ladder[$index]) consumes the corpus"
        Assert-True (-not ($line -like '*--limit-episodes*')) "tier $($ladder[$index]) has no smoke bound by default"
        Assert-True ($line -like '*--max-episodes 64*') "tier $($ladder[$index]) carries the corpus episode count as its guard"
        Assert-True ($line -like "*--diagnostics-dir*tier-$($ladder[$index]).diagnostics*") "tier $($ladder[$index]) gets its own diagnostics directory"
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
    Assert-True ($provenanceJson.planned_tier_commands.Count -eq 1) 'a tier subset plans only those tiers'
    Assert-True ($provenanceJson.planned_tier_commands[0] -like '*--limit-episodes 8*') 'the smoke bound reaches the tier command'
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
    $tierCommand = $provenanceJson.planned_tier_commands[0]
    Assert-True ($tierCommand -like '*"*tier-t512.diagnostics"*') 'a diagnostics path under a spaced evidence root is quoted'
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

    # --- 5. Nothing in this whole test file ever started a child process:
    #        the two stand-ins are still exactly the text files we wrote.
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
