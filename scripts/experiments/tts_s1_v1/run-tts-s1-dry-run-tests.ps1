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
    including the authority shape for each -StoreKind;
  * -LimitDecisions appears only when it is set;
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

    $resultJson = Get-Content -LiteralPath (Join-Path $attempt 'result.json') -Raw | ConvertFrom-Json
    Assert-True ($resultJson.status -ceq 'DRY_RUN_PLANNED') 'result.json says DRY_RUN_PLANNED'
    Assert-True ($resultJson.planned_tier_commands.Count -eq 4) 'the whole ladder is planned'

    $provenanceJson = Get-Content -LiteralPath (Join-Path $attempt 'provenance.json') -Raw | ConvertFrom-Json
    Assert-True ($provenanceJson.dry_run -eq $true) 'provenance.json records the dry run'
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
        Assert-True (-not ($line -like '*--limit-decisions*')) "tier $($ladder[$index]) has no smoke bound by default"
    }

    # --- 2. -LimitDecisions is threaded to every tier when set.
    $evidence = Join-Path $sandbox 'evidence-limit'
    $parameters = $base.Clone()
    $parameters['EvidenceRoot'] = $evidence
    $parameters['LimitDecisions'] = [uint64]8
    $parameters['Tiers'] = @('t512')
    $result = Invoke-Wrapper -Parameters $parameters
    Assert-True ($null -eq $result.Failure) "a single-tier dry run succeeds ($($result.Failure))"
    $attempt = Get-OnlyAttemptRoot -EvidenceRoot $evidence
    $provenanceJson = Get-Content -LiteralPath (Join-Path $attempt 'provenance.json') -Raw | ConvertFrom-Json
    Assert-True ($provenanceJson.planned_tier_commands.Count -eq 1) 'a tier subset plans only those tiers'
    Assert-True ($provenanceJson.planned_tier_commands[0] -like '*--limit-decisions 8*') 'the smoke bound reaches the tier command'

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
