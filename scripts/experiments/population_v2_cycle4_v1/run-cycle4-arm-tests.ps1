<#
.SYNOPSIS
Dry-run tests for run-cycle4-arm.ps1.

.DESCRIPTION
Builds a synthetic campaign under a temporary root -- a genesis refresh
manifest, a run record, eight slot store directories, a parent store with the
three genesis-seeding artifacts, and placeholder executables -- then drives the
wrapper in -DryRun -SkipHostAssertions and asserts:

  * the genesis sequence -- the arm bin's `--bootstrap-genesis` (with no
    interval flag on it) followed by the builder's `--genesis` -- on a
    campaign where neither the Store nor the genesis manifest exists yet,
  * the exact command line of every child it would run (the arm bin's per
    interval --stop-generation and --payoff-panel, the panel runner's fixed
    G = 256 and its per-refresh disjoint seed, the builder's --next-generation
    and --output),
  * that both locator files are written from one slot table and agree,
  * that STATIC-RB runs the panel and never builds a manifest,
  * that the preflight ladder plans two independent prefixes under the attempt
    root with the bounded --preflight/--preflight-updates pair,
  * and the fail-closed rejections, each of which must also leave a RUN_FAILED
    marker naming the failing phase.

Nothing here launches a child process, touches a GPU, or trains.

Run it with:  powershell -NoProfile -File run-cycle4-arm-tests.ps1
#>
param([string]$WorkRoot)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:Failures = 0
$script:Checks = 0

function Assert-That {
    param(
        [Parameter(Mandatory = $true)][bool]$Condition,
        [Parameter(Mandatory = $true)][string]$Message
    )
    $script:Checks++
    if (-not $Condition) {
        $script:Failures++
        Write-Host "FAIL: $Message"
    }
}

function Assert-Throws {
    param(
        [Parameter(Mandatory = $true)][scriptblock]$Action,
        [Parameter(Mandatory = $true)][string]$ExpectedSubstring,
        [Parameter(Mandatory = $true)][string]$Message
    )
    $script:Checks++
    $caught = $null
    try { & $Action | Out-Null }
    catch { $caught = $_.Exception.Message }
    if ($null -eq $caught) {
        $script:Failures++
        Write-Host "FAIL: $Message (nothing was thrown)"
        return
    }
    if ($caught -notlike "*$ExpectedSubstring*") {
        $script:Failures++
        Write-Host "FAIL: $Message (threw '$caught', expected to contain '$ExpectedSubstring')"
    }
}

# ---------------------------------------------------------------------------
# Synthetic campaign
# ---------------------------------------------------------------------------

if ([string]::IsNullOrWhiteSpace($WorkRoot)) {
    $WorkRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("cycle4-wrapper-tests-{0}-{1}" -f $PID, [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
}
New-Item -ItemType Directory -Force -Path $WorkRoot | Out-Null

$wrapper = Join-Path $PSScriptRoot 'run-cycle4-arm.ps1'
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path

# The resume fixtures below plant interval journals, and section 6 drives the
# genesis binding check directly, so the shared helpers are needed here rather
# than only inside the wrapper's own scope.
. (Join-Path $PSScriptRoot 'common.ps1')

function New-SyntheticHash {
    param([Parameter(Mandatory = $true)][int]$Tag)
    return (('{0:x2}' -f $Tag) * 32)
}

function New-SyntheticSlot {
    param(
        [Parameter(Mandatory = $true)][int]$Index,
        [Parameter(Mandatory = $true)][string]$Role
    )
    return [ordered]@{
        slot_index = $Index
        role = $Role
        occupant_class = $(if ($Index -ge 6) { 'historical-fallback' } else { 'policy' })
        source_base_seed = 977002
        source_run_sha256 = New-SyntheticHash -Tag (0x10 + $Index)
        source_generation = 384
        checkpoint_manifest_sha256 = New-SyntheticHash -Tag (0x20 + $Index)
        checkpoint_payload_sha256 = New-SyntheticHash -Tag (0x30 + $Index)
        model_parameter_sha256 = New-SyntheticHash -Tag (0x40 + $Index)
        train_state_sha256 = New-SyntheticHash -Tag (0x50 + $Index)
        weight_units = 125000
    }
}

$roles = @('anchor-0', 'anchor-1', 'historical-0', 'historical-1', 'current-0', 'current-1', 'exploiter-0', 'exploiter-1')
$traineeRunSha256 = New-SyntheticHash -Tag 0xaa
$traineeBaseSeed = 977102

# The two slots the wrapper verifies against the Store itself (historical-1's
# rotation phase, and historical-0 while it is still the cycle-3 lineage) need
# real checkpoint records to verify, so the synthetic stores carry them and the
# synthetic manifests take those slots' four content hashes from them.
$script:Slot2Identity = $null
$script:HistoricalOneIdentities = @()

function New-SyntheticStoreCheckpoint {
    # One checkpoint record in the exact shape Get-Cycle4CheckpointIdentity
    # reads: generation_index, payload.sha256, and the two train_state hashes.
    # checkpoint_manifest_sha256 is the SHA-256 of the written bytes, so it can
    # only be computed after the file exists.
    param(
        [Parameter(Mandatory = $true)][string]$StoreRoot,
        [Parameter(Mandatory = $true)][uint64]$Generation,
        [Parameter(Mandatory = $true)][int]$Tag
    )
    $payload = New-SyntheticHash -Tag $Tag
    $model = New-SyntheticHash -Tag ($Tag + 1)
    $state = New-SyntheticHash -Tag ($Tag + 2)
    $path = Join-Path (Join-Path $StoreRoot 'checkpoints') ('update-{0:d8}.checkpoint.json' -f $Generation)
    Write-SyntheticJson -Value ([ordered]@{
        schema = 'mtg-kernel-native-checkpoint/v3'
        generation_index = $Generation
        train_state = [ordered]@{ model_parameter_sha256 = $model; state_sha256 = $state }
        payload = [ordered]@{ sha256 = $payload }
    }) -Path $path
    return [ordered]@{
        checkpoint_manifest_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
        checkpoint_payload_sha256 = $payload
        model_parameter_sha256 = $model
        train_state_sha256 = $state
    }
}

function Write-SyntheticJson {
    param(
        [Parameter(Mandatory = $true)]$Value,
        [Parameter(Mandatory = $true)][string]$Path
    )
    $directory = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($directory)) {
        New-Item -ItemType Directory -Force -Path $directory | Out-Null
    }
    $json = $Value | ConvertTo-Json -Depth 12
    [System.IO.File]::WriteAllText($Path, $json, [System.Text.UTF8Encoding]::new($false))
}

function New-SyntheticManifest {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][uint64]$RefreshIndex,
        [switch]$DuplicateIdentity
    )
    $slots = @(
        foreach ($index in 0..7) { New-SyntheticSlot -Index $index -Role $roles[$index] }
    )
    # Slot 5 (current-1) is always the trainee's own run, and slot 2
    # (historical-0) becomes the trainee's own run from refresh 4 -- the same
    # rule validate_slot_assignment_cycle4_v1 enforces. Both are what make the
    # wrapper substitute the arm's own Store root, and both must therefore
    # carry the baseline chain directory on a v4 arm.
    $slots[5].source_run_sha256 = $traineeRunSha256
    $slots[5].source_base_seed = $traineeBaseSeed
    $slots[5].source_generation = (896 + ($RefreshIndex * 128))
    # anchor-1 shares its Store with historical-1's middle rotation phase, at a
    # DIFFERENT pinned generation: one physical Store, two occupants. That is
    # the real roster's shape (970002 at 1536 and at 1024) and the case the
    # locator writer must accept.
    $slots[1].source_generation = 1536
    # historical-1 rotates by refresh_index mod 3, so its four content hashes
    # are that phase's Store's, not one fixed set.
    if (@($script:HistoricalOneIdentities).Count -eq 3) {
        $rotation = [int]($RefreshIndex % 3)
        foreach ($field in @('checkpoint_manifest_sha256', 'checkpoint_payload_sha256', 'model_parameter_sha256', 'train_state_sha256')) {
            $slots[3].$field = [string]$script:HistoricalOneIdentities[$rotation].$field
        }
    }
    if ($RefreshIndex -le 3 -and $null -ne $script:Slot2Identity) {
        foreach ($field in @('checkpoint_manifest_sha256', 'checkpoint_payload_sha256', 'model_parameter_sha256', 'train_state_sha256')) {
            $slots[2].$field = [string]$script:Slot2Identity.$field
        }
    }
    if ($RefreshIndex -ge 4) {
        $slots[2].source_run_sha256 = $traineeRunSha256
        $slots[2].source_base_seed = $traineeBaseSeed
        $slots[2].source_generation = (896 + ($RefreshIndex * 128) - 512)
    }
    if ($DuplicateIdentity) {
        $slots[7].checkpoint_manifest_sha256 = $slots[6].checkpoint_manifest_sha256
    }
    $document = [ordered]@{
        schema = 'mtg-kernel-population-refresh-manifest-cycle4/v1'
        refresh_index = $RefreshIndex
        trainee_local_generation = (896 + ($RefreshIndex * 128))
        trainee_run_sha256 = $traineeRunSha256
        trainee_base_seed = $traineeBaseSeed
        weight_total_units = 1000000
        slots = $slots
    }
    Write-SyntheticJson -Value $document -Path $Path
}

$slotStoreRoots = @(
    foreach ($index in 0..7) {
        $path = Join-Path $WorkRoot ("slot-{0}" -f $index)
        New-Item -ItemType Directory -Force -Path $path | Out-Null
        (Resolve-Path -LiteralPath $path).Path
    }
)

# historical-1's three rotation Stores, in rotation order. The middle one is
# also anchor-1's Store (slot 1), which is exactly the shared-root case.
$historicalOneRoots = @(
    foreach ($phase in 0..2) {
        $path = Join-Path $WorkRoot ("historical-one-{0}" -f $phase)
        New-Item -ItemType Directory -Force -Path $path | Out-Null
        (Resolve-Path -LiteralPath $path).Path
    }
)
$script:HistoricalOneIdentities = @(
    foreach ($phase in 0..2) {
        New-SyntheticStoreCheckpoint -StoreRoot $historicalOneRoots[$phase] -Generation ([uint64]384) -Tag (0x60 + ($phase * 8))
    }
)
# anchor-1 is the middle rotation Store at generation 1536, so its own
# checkpoint lives beside the rotation one in the same Store.
New-SyntheticStoreCheckpoint -StoreRoot $historicalOneRoots[1] -Generation ([uint64]1536) -Tag 0x90 | Out-Null
$slotStoreRoots[1] = $historicalOneRoots[1]
$slotStoreRoots[3] = $historicalOneRoots[0]
$script:Slot2Identity = New-SyntheticStoreCheckpoint -StoreRoot $slotStoreRoots[2] -Generation ([uint64]384) -Tag 0xa0

$refreshChainDir = Join-Path $WorkRoot 'refresh-chain'
New-SyntheticManifest -Path (Join-Path $refreshChainDir 'refresh-00.manifest.json') -RefreshIndex ([uint64]0)
# Manifests 1..16 and their panels, so a dry run can walk the whole campaign
# without any child ever having produced one.
foreach ($index in 1..16) {
    New-SyntheticManifest -Path (Join-Path $refreshChainDir ('refresh-{0:d2}.manifest.json' -f $index)) -RefreshIndex ([uint64]$index)
    [System.IO.File]::WriteAllText((Join-Path $refreshChainDir ('refresh-{0:d2}.panel.json' -f $index)), '{}', [System.Text.UTF8Encoding]::new($false))
}

# The genesis parent is the cycle-3 focal run at STORE generation 896 -- the
# pre-registered trainee-local start, not the lineage tip 2048.
$parentGeneration = [uint64]896
$parentStore = Join-Path $WorkRoot 'cycle3-parent'
$parentCheckpoints = Join-Path $parentStore 'checkpoints'
New-Item -ItemType Directory -Force -Path $parentCheckpoints | Out-Null
[System.IO.File]::WriteAllText((Join-Path $parentStore 'run.json'), '{"schema":"parent"}', [System.Text.UTF8Encoding]::new($false))
$parentArtifacts = [ordered]@{}
foreach ($suffix in @('checkpoint.json', 'sidecar.json', 'state.f32le')) {
    $artifact = Join-Path $parentCheckpoints ('update-{0:d8}.{1}' -f $parentGeneration, $suffix)
    [System.IO.File]::WriteAllText($artifact, "parent-$suffix", [System.Text.UTF8Encoding]::new($false))
    $parentArtifacts[$suffix] = (Get-FileHash -Algorithm SHA256 -LiteralPath $artifact).Hash.ToLowerInvariant()
}
$parentRunSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $parentStore 'run.json')).Hash.ToLowerInvariant()

# The run record a real launch DERIVES with cycle4_run_record_v1. The synthetic
# copy carries the two things the wrapper reads from it: the schedule, and the
# pinned genesis origin it cross-checks -GenesisParentGeneration against.
function Write-SyntheticRunRecord {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][uint64]$Generation,
        [string]$CheckpointSha256,
        [string]$RunSha256
    )
    if ([string]::IsNullOrWhiteSpace($CheckpointSha256)) { $CheckpointSha256 = $parentArtifacts['checkpoint.json'] }
    if ([string]::IsNullOrWhiteSpace($RunSha256)) { $RunSha256 = $parentRunSha256 }
    Write-SyntheticJson -Value ([ordered]@{
        schema = 'mtg-kernel-native-train-run/v2'
        schedule = [ordered]@{
            base_seed = $traineeBaseSeed
            checkpoint_segment_updates = 4
            batch_episodes = 64
        }
        contracts = [ordered]@{
            opponent_ladder_initialization = [ordered]@{
                source_run_sha256 = $RunSha256
                generation = $Generation
                checkpoint_sha256 = $CheckpointSha256
                sidecar_sha256 = $parentArtifacts['sidecar.json']
                state_sha256 = $parentArtifacts['state.f32le']
                derived_model_parameter_sha256 = (New-SyntheticHash -Tag 0xd1)
            }
        }
    }) -Path $Path
}

$runRecord = Join-Path $WorkRoot 'run.json'
Write-SyntheticRunRecord -Path $runRecord -Generation $parentGeneration

$binRoot = Join-Path $WorkRoot 'bin'
New-Item -ItemType Directory -Force -Path $binRoot | Out-Null
$armExecutable = Join-Path $binRoot 'cycle4_arm_v1.exe'
$builderExecutable = Join-Path $binRoot 'cycle4_refresh_build_v1.exe'
$panelExecutable = Join-Path $binRoot 'mtg_kernel-tests.exe'
$pythonExecutable = Join-Path $binRoot 'python.exe'
$runRecordExecutable = Join-Path $binRoot 'cycle4_run_record_v1.exe'
foreach ($path in @($armExecutable, $builderExecutable, $panelExecutable, $pythonExecutable, $runRecordExecutable)) {
    [System.IO.File]::WriteAllText($path, 'placeholder', [System.Text.UTF8Encoding]::new($false))
}

$rosterDir = Join-Path $WorkRoot 'slot-identities'
New-Item -ItemType Directory -Force -Path $rosterDir | Out-Null
foreach ($index in 0..16) {
    Write-SyntheticJson -Value ([ordered]@{
        schema = 'mtg-kernel-cycle4-slot-identities/v1'
        slots = @(
            foreach ($slot in 0..7) {
                [ordered]@{
                    slot_index = $slot
                    source_base_seed = 977002
                    source_run_sha256 = New-SyntheticHash -Tag (0x10 + $slot)
                    source_generation = 384
                    checkpoint_manifest_sha256 = New-SyntheticHash -Tag (0x20 + $slot)
                    checkpoint_payload_sha256 = New-SyntheticHash -Tag (0x30 + $slot)
                    model_parameter_sha256 = New-SyntheticHash -Tag (0x40 + $slot)
                    train_state_sha256 = New-SyntheticHash -Tag (0x50 + $slot)
                }
            }
        )
    }) -Path (Join-Path $rosterDir ('refresh-{0:d2}.slot-identities.json' -f $index))
}

$panelBaseSeed = [uint64]4000000000

function New-WrapperArguments {
    param(
        [Parameter(Mandatory = $true)][string]$Mode,
        [Parameter(Mandatory = $true)][string]$Arm,
        [Parameter(Mandatory = $true)][string]$EvidenceRoot,
        [string]$ChainDirOverride
    )
    $arguments = @{
        Mode = $Mode
        Arm = $Arm
        EvidenceRoot = $EvidenceRoot
        RunRecord = $runRecord
        RefreshChainDir = $refreshChainDir
        SlotIdentitiesRosterDir = $rosterDir
        SlotStoreRoots = $slotStoreRoots
        HistoricalOneStoreRoots = $historicalOneRoots
        GenesisParentStoreRoot = (Resolve-Path -LiteralPath $parentStore).Path
        GenesisParentGeneration = $parentGeneration
        ArmExecutable = $armExecutable
        RunRecordExecutable = $runRecordExecutable
        RefreshBuilderExecutable = $builderExecutable
        PanelExecutable = $panelExecutable
        PythonExecutable = $pythonExecutable
        PanelBaseSeed = $panelBaseSeed
        RepoRoot = $repoRoot
        Device = 1
        DryRun = $true
        SkipHostAssertions = $true
    }
    if ($Mode -ceq 'formal') {
        $arguments['StoreRoot'] = (Join-Path $EvidenceRoot 'arm-prefix\store')
        $arguments['ChainDir'] = (Join-Path $EvidenceRoot 'arm-prefix\chain')
    }
    if (-not [string]::IsNullOrWhiteSpace($ChainDirOverride)) {
        $arguments['ChainDir'] = $ChainDirOverride
    }
    return $arguments
}

function Get-LatestAttemptRoot {
    param(
        [Parameter(Mandatory = $true)][string]$EvidenceRoot,
        [Parameter(Mandatory = $true)][string]$GateName
    )
    $gateRoot = Join-Path $EvidenceRoot $GateName
    return (Get-ChildItem -LiteralPath $gateRoot -Directory | Sort-Object Name -Descending | Select-Object -First 1).FullName
}

function Get-CommandRecords {
    param([Parameter(Mandatory = $true)][string]$AttemptRoot)
    $path = Join-Path $AttemptRoot 'commands.jsonl'
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { return @() }
    return @(
        foreach ($line in [System.IO.File]::ReadAllLines($path)) {
            if (-not [string]::IsNullOrWhiteSpace($line)) { $line | ConvertFrom-Json }
        }
    )
}

# ---------------------------------------------------------------------------
# 1. TREATMENT-RB, full dry run through refresh 16
# ---------------------------------------------------------------------------

$evidence = Join-Path $WorkRoot 'evidence-treatment'
$treatmentArguments = New-WrapperArguments -Mode 'formal' -Arm 'treatment-rb' -EvidenceRoot $evidence
& $wrapper @treatmentArguments *>&1 | Out-Null
$attempt = Get-LatestAttemptRoot -EvidenceRoot $evidence -GateName 'cycle4-treatment-rb-formal'

Assert-That -Condition (-not (Test-Path -LiteralPath (Join-Path $attempt 'TRAINING_COMPLETE'))) `
    -Message 'a dry run never publishes TRAINING_COMPLETE'
Assert-That -Condition (-not (Test-Path -LiteralPath (Join-Path $attempt 'RUN_FAILED'))) `
    -Message 'a passing dry run publishes no RUN_FAILED'
$treatmentResult = Get-Content -Raw -LiteralPath (Join-Path $attempt 'result.json') | ConvertFrom-Json
Assert-That -Condition ([string]$treatmentResult.status -ceq 'DRY_RUN_PLANNED') `
    -Message 'a dry run says plainly that it only planned'

$records = Get-CommandRecords -AttemptRoot $attempt
$bootstrapCommands = @($records | Where-Object { $_.label -ceq 'bootstrap-genesis' })
$genesisBuildCommands = @($records | Where-Object { $_.label -ceq 'genesis-build' })
$armCommands = @($records | Where-Object { $_.label -like 'arm-interval-*' })
$panelCommands = @($records | Where-Object { $_.label -like 'panel-interval-*' })
$buildCommands = @($records | Where-Object { $_.label -like 'refresh-build-*' })

Assert-That -Condition ($bootstrapCommands.Count -eq 1) -Message "an unseeded Store is bootstrapped exactly once (saw $($bootstrapCommands.Count))"
Assert-That -Condition ($genesisBuildCommands.Count -eq 0) -Message 'an existing genesis manifest is never rebuilt'
Assert-That -Condition ($armCommands.Count -eq 16) -Message "16 intervals run the arm bin (saw $($armCommands.Count))"
Assert-That -Condition ($panelCommands.Count -eq 16) -Message "16 intervals run the panel (saw $($panelCommands.Count))"
Assert-That -Condition ($buildCommands.Count -eq 16) -Message "16 intervals build the next manifest (saw $($buildCommands.Count))"
Assert-That -Condition (@($records | Where-Object { $_.dry_run -ne $true }).Count -eq 0) `
    -Message 'every dry-run command is marked dry_run'

# ---------------------------------------------------------------------------
# 1a. The derived run record (round E item 1)
# ---------------------------------------------------------------------------
$runRecordCommands = @($records | Where-Object { $_.label -ceq 'run-record' })
Assert-That -Condition ($runRecordCommands.Count -eq 1) `
    -Message "the run record is derived exactly once per launch (saw $($runRecordCommands.Count))"
$runRecordCommand = $runRecordCommands[0].command_line
Assert-That -Condition ($runRecordCommand -like "*`"--arm`" `"treatment-rb`"*") -Message 'the run-record builder is told which arm it is building'
Assert-That -Condition ($runRecordCommand -like "*`"--parent-store-root`"*cycle3-parent*") -Message 'the run-record builder is pointed at the pinned parent Store'
Assert-That -Condition ($runRecordCommand -like "*`"--parent-generation`" `"896`"*") -Message 'the run-record builder is given the trainee-local start 896, not the lineage tip'
Assert-That -Condition ($runRecordCommand -like "*`"--output`"*run.json*") -Message 'the run-record builder writes the arm run record'
Assert-That -Condition ($runRecordCommand -notlike '*--force*') -Message 'a launch never forces a run-record overwrite'
Assert-That -Condition ($runRecordCommand -notlike '*--base-seed*') -Message "the arm's base seed is the builder's own literal, never a wrapper flag"
Assert-That -Condition ($runRecordCommand -like "*`"--arm-executable`" `"$armExecutable`"*") `
    -Message 'the run-record builder is handed the arm launcher whose build provenance the record declares'
$genesisParentBinding = Get-Content -Raw -LiteralPath (Join-Path $attempt 'genesis-parent-binding.json') | ConvertFrom-Json
Assert-That -Condition ([uint64]$genesisParentBinding.parent_generation -eq 896) `
    -Message 'the wrapper records the parent generation it proved against the run record'

$bootstrap = $bootstrapCommands[0].command_line
Assert-That -Condition ($bootstrap -like '*"--bootstrap-genesis"*') -Message 'the bootstrap passes the value-less marker'
Assert-That -Condition ($bootstrap -notlike '*--refresh-manifest*') -Message 'the bootstrap passes no refresh manifest'
Assert-That -Condition ($bootstrap -notlike '*--stop-generation*') -Message 'the bootstrap passes no stop generation'
Assert-That -Condition ($bootstrap -notlike '*--payoff-panel*') -Message 'the bootstrap passes no payoff panel'
Assert-That -Condition ($bootstrap -notlike '*--preflight*') -Message 'the bootstrap passes no preflight flag'
Assert-That -Condition ($bootstrap -like '*"--slot-locator"*bootstrap-slot-locator.json*') -Message 'the bootstrap gets its own locator naming the genesis parent'
$bootstrapLocator = Get-Content -Raw -LiteralPath (Join-Path $attempt 'bootstrap-slot-locator.json') | ConvertFrom-Json
Assert-That -Condition ([string]$bootstrapLocator.genesis_parent_store_root -ceq (Resolve-Path -LiteralPath $parentStore).Path) `
    -Message 'the bootstrap locator carries the genesis parent store root'
Assert-That -Condition ([string]$bootstrapLocator.stores[5].store_root -ceq [string]$treatmentArguments['StoreRoot']) `
    -Message "the bootstrap locator puts the arm's own Store in the own-run slot"

$armZero = $armCommands[0].command_line
Assert-That -Condition ($armZero -like "*`"--arm`" `"treatment-rb`"*") -Message 'the arm bin is told which arm it is'
Assert-That -Condition ($armZero -like '*"--stop-generation" "128"*') -Message 'interval 0 stops at store generation 128'
Assert-That -Condition ($armZero -notlike '*--payoff-panel*') -Message 'genesis takes no --payoff-panel'
Assert-That -Condition ($armZero -like '*"--refresh-manifest"*refresh-00.manifest.json*') -Message 'interval 0 uses the genesis manifest'
Assert-That -Condition ($armZero -like '*"--device" "1"*') -Message 'the arm bin is pinned to the requested device'
Assert-That -Condition ($armZero -notlike '*--preflight*') -Message 'a formal interval never passes a preflight flag'

$armOne = $armCommands[1].command_line
Assert-That -Condition ($armOne -like '*"--stop-generation" "256"*') -Message 'interval 1 stops at store generation 256'
Assert-That -Condition ($armOne -like '*"--payoff-panel"*refresh-01.panel.json*') -Message 'interval 1 binds refresh 1 panel bytes'
Assert-That -Condition ($armOne -like '*"--refresh-manifest"*refresh-01.manifest.json*') -Message 'interval 1 uses refresh 1 manifest'

$armLast = $armCommands[15].command_line
Assert-That -Condition ($armLast -like '*"--stop-generation" "2048"*') -Message 'the last interval stops at the program end 2048'

$panelZero = $panelCommands[0].command_line
Assert-That -Condition ($panelZero -like '*run_payoff_panel_v1.py*') -Message 'the panel runs through the round-C runner'
Assert-That -Condition ($panelZero -like '*"--games-per-matchup" "256"*') -Message 'the panel always runs the pre-registered G = 256'
$expectedSeedZero = $panelBaseSeed + [uint64]32000000
Assert-That -Condition ($panelZero -like "*`"--base-seed`" `"$expectedSeedZero`"*") -Message 'panel 1 uses its own disjoint seed window'
$expectedSeedOne = $panelBaseSeed + [uint64]64000000
Assert-That -Condition ($panelCommands[1].command_line -like "*`"--base-seed`" `"$expectedSeedOne`"*") -Message 'panel 2 uses the next disjoint seed window'
Assert-That -Condition ($panelZero -like '*"--manifest"*refresh-00.manifest.json*') -Message 'the panel evaluates the interval manifest roster'

$buildZero = $buildCommands[0].command_line
Assert-That -Condition ($buildZero -like '*"--next-generation" "1"*') -Message 'the first build produces refresh 1'
Assert-That -Condition ($buildZero -like '*"--output"*refresh-01.manifest.json*') -Message 'the first build writes refresh-01.manifest.json'
Assert-That -Condition ($buildZero -like '*"--panel"*refresh-01.panel.json*') -Message 'the first build binds refresh-01.panel.json'
Assert-That -Condition ($buildZero -like "*`"--trainee-run-sha256`" `"$traineeRunSha256`"*") -Message 'the build binds the arm run identity from the manifest chain'
Assert-That -Condition ($buildZero -like "*`"--trainee-base-seed`" `"$traineeBaseSeed`"*") -Message 'the build binds the arm base seed from the manifest chain'
Assert-That -Condition ($buildCommands[15].command_line -like '*"--next-generation" "16"*') -Message 'the last build produces refresh 16'

# Both locators, written from one slot table, for every interval.
$armLocator = Get-Content -Raw -LiteralPath (Join-Path $attempt 'interval-00\arm-slot-locator.json') | ConvertFrom-Json
$panelLocator = Get-Content -Raw -LiteralPath (Join-Path $attempt 'interval-00\panel-slot-locator.json') | ConvertFrom-Json
Assert-That -Condition ([string]$armLocator.schema -ceq 'mtg-kernel-cycle4-arm-slot-locator/v1') -Message 'the arm locator declares its own identity-keyed schema'
Assert-That -Condition ([string]$panelLocator.schema -ceq 'mtg-kernel-cycle4-slot-locator/v1') -Message 'the panel locator declares the index-keyed schema'
Assert-That -Condition (@($armLocator.stores).Count -eq 8) -Message 'the arm locator carries eight stores'
Assert-That -Condition ([string]$armLocator.genesis_parent_store_root -ceq (Resolve-Path -LiteralPath $parentStore).Path) `
    -Message 'the arm locator carries the genesis parent store root'
function Get-PanelSlotStoreRoot {
    # A panel-locator slot entry is either a bare store-root string or an
    # object carrying store_root plus the optional baseline_chain_dir.
    param([Parameter(Mandatory = $true)]$Entry)
    if ($Entry -is [string]) { return [string]$Entry }
    return [string]$Entry.store_root
}

function Get-PanelSlotBaselineChainDir {
    param([Parameter(Mandatory = $true)]$Entry)
    if ($Entry -is [string]) { return $null }
    if ($Entry.PSObject.Properties.Name -notcontains 'baseline_chain_dir') { return $null }
    return [string]$Entry.baseline_chain_dir
}

$genesisManifestSlots = (Get-Content -Raw -LiteralPath (Join-Path $refreshChainDir 'refresh-00.manifest.json') | ConvertFrom-Json).slots
$agree = $true
foreach ($index in 0..7) {
    # Slot 5 is the arm's own run in the synthetic roster, so the wrapper
    # substitutes the arm's own Store for the operator's table entry. Slot 3
    # comes from the rotation triple, whose phase-0 entry is the fixed table's
    # slot-3 root, so refresh 0 agrees with the table there too.
    if ($index -eq 5) { $expectedRoot = [string]$treatmentArguments['StoreRoot'] }
    else { $expectedRoot = [string]$slotStoreRoots[$index] }
    if ([string]$armLocator.stores[$index].store_root -cne $expectedRoot) { $agree = $false }
    # The identity is keyed from the MANIFEST, which for the two verified slots
    # carries the identity read off their Stores rather than a synthetic tag.
    if ([string]$armLocator.stores[$index].checkpoint_manifest_sha256 -cne [string]$genesisManifestSlots[$index].checkpoint_manifest_sha256) { $agree = $false }
    if ((Get-PanelSlotStoreRoot -Entry $panelLocator.stores."$index") -cne $expectedRoot) { $agree = $false }
}
Assert-That -Condition $agree -Message "both locators name the same store for the same slot, with the arm's own Store substituted into its own slot"

# The genesis authority record lives with the campaign and is re-verified.
$authorityPath = Join-Path $evidence 'arm-prefix\chain\cycle4-genesis-authority-treatment-rb.json'
Assert-That -Condition (Test-Path -LiteralPath $authorityPath) -Message 'the genesis authority record is published beside the bin arm-origin record in the baseline chain directory'
$authority = Get-Content -Raw -LiteralPath $authorityPath | ConvertFrom-Json
Assert-That -Condition ([string]$authority.schema -ceq 'mtg-kernel-cycle4-genesis-authority/v1') -Message 'the genesis authority declares its schema'
Assert-That -Condition (([string]$authority.parent_checkpoint_sha256).Length -eq 64) -Message 'the genesis authority records the parent checkpoint hash'

# ---------------------------------------------------------------------------
# 1b. A fresh campaign: no Store, no genesis manifest
# ---------------------------------------------------------------------------

$freshChain = Join-Path $WorkRoot 'refresh-chain-fresh'
New-Item -ItemType Directory -Force -Path $freshChain | Out-Null
$freshEvidence = Join-Path $WorkRoot 'evidence-fresh'
$freshArguments = New-WrapperArguments -Mode 'formal' -Arm 'treatment-rb' -EvidenceRoot $freshEvidence
$freshArguments['RefreshChainDir'] = $freshChain
& $wrapper @freshArguments *>&1 | Out-Null
$freshAttempt = Get-LatestAttemptRoot -EvidenceRoot $freshEvidence -GateName 'cycle4-treatment-rb-formal'
$freshRecords = Get-CommandRecords -AttemptRoot $freshAttempt

Assert-That -Condition (@($freshRecords | Where-Object { $_.label -ceq 'bootstrap-genesis' }).Count -eq 1) `
    -Message 'a fresh campaign plans exactly one bootstrap'
$freshBuild = @($freshRecords | Where-Object { $_.label -ceq 'genesis-build' })
Assert-That -Condition ($freshBuild.Count -eq 1) -Message 'a fresh campaign plans exactly one genesis manifest build'
Assert-That -Condition ($freshBuild[0].command_line -like '*"--genesis"*') -Message 'the genesis manifest is built with the builder --genesis mode'
Assert-That -Condition ($freshBuild[0].command_line -notlike '*--chain-dir*') -Message 'a genesis build passes no chain directory'
Assert-That -Condition ($freshBuild[0].command_line -notlike '*--panel*') -Message 'a genesis build binds no panel'
Assert-That -Condition ($freshBuild[0].command_line -notlike '*--next-generation*') -Message 'a genesis build declares no next generation'
Assert-That -Condition ($freshBuild[0].command_line -like '*"--output"*refresh-00.manifest.json*') -Message 'the genesis build writes refresh-00.manifest.json'
Assert-That -Condition ($freshBuild[0].command_line -like "*`"--trainee-base-seed`" `"$traineeBaseSeed`"*") -Message 'the genesis build takes the base seed from the run record'
Assert-That -Condition ($freshBuild[0].command_line -like '*from-arm-origin.record.json-after-bootstrap*') `
    -Message 'a dry run says plainly that the run identity comes from the record the bootstrap publishes'
Assert-That -Condition ($freshBuild[0].command_line -like '*"--slot-identities"*refresh-00.slot-identities.json*') -Message 'the genesis build stages the operator roster'
Assert-That -Condition (@($freshRecords | Where-Object { $_.label -like 'arm-interval-*' }).Count -eq 0) `
    -Message 'a dry run over a fresh campaign plans no interval it cannot read a manifest for'
$freshResult = Get-Content -Raw -LiteralPath (Join-Path $freshAttempt 'result.json') | ConvertFrom-Json
Assert-That -Condition ([string]$freshResult.dry_run_stopped_after -ceq 'genesis-manifest') `
    -Message 'the result says exactly where the dry run stopped'
Assert-That -Condition ([string]$freshResult.status -ceq 'DRY_RUN_PLANNED') `
    -Message 'the fresh-campaign dry run reports a plan, not a completion'
Assert-That -Condition (-not (Test-Path -LiteralPath (Join-Path $freshAttempt 'TRAINING_COMPLETE'))) `
    -Message 'the fresh-campaign dry run publishes no TRAINING_COMPLETE either'

$noRoster = Join-Path $WorkRoot 'slot-identities-no-genesis'
New-Item -ItemType Directory -Force -Path $noRoster | Out-Null
$missingRoster = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (Join-Path $WorkRoot 'evidence-reject-no-roster')
$missingRoster['SlotIdentitiesRosterDir'] = $noRoster
Assert-Throws -Action { & $wrapper @missingRoster *>&1 | Out-Null } `
    -ExpectedSubstring 'genesis slot-identities roster is missing' `
    -Message 'the genesis roster is a required operator input'

# ---------------------------------------------------------------------------
# 1c. The panel locator's per-slot baseline_chain_dir
#
# The payoff probe loads a v4 arm's trained own-run checkpoints through the
# baseline-aware loader, which needs that arm's chain directory. The
# index-keyed panel locator therefore carries it, on exactly the slots the
# manifest binds to the arm's own run, and only for the arms that have a chain.
# ---------------------------------------------------------------------------

function Get-PanelLocator {
    param(
        [Parameter(Mandatory = $true)][string]$AttemptRoot,
        [Parameter(Mandatory = $true)][int]$Interval
    )
    $path = Join-Path $AttemptRoot ('interval-{0:d2}\panel-slot-locator.json' -f $Interval)
    return (Get-Content -Raw -LiteralPath $path | ConvertFrom-Json)
}

function Assert-PanelLocatorChainDirs {
    param(
        [Parameter(Mandatory = $true)]$Locator,
        [AllowEmptyCollection()][int[]]$ExpectedSlots = @(),
        [string]$ExpectedChainDir,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $carried = @()
    $wrongValue = @()
    foreach ($index in 0..7) {
        $entry = $Locator.stores."$index"
        $chain = Get-PanelSlotBaselineChainDir -Entry $entry
        if ($null -ne $chain) {
            $carried += $index
            if ($chain -cne ([System.IO.Path]::GetFullPath($ExpectedChainDir))) { $wrongValue += $index }
        }
    }
    Assert-That -Condition (@(Compare-Object -ReferenceObject @($ExpectedSlots) -DifferenceObject @($carried)).Count -eq 0) `
        -Message "$Label carries baseline_chain_dir on exactly slots $($ExpectedSlots -join ',') (saw $($carried -join ','))"
    if ($ExpectedSlots.Count -ne 0) {
        Assert-That -Condition ($wrongValue.Count -eq 0) `
            -Message "$Label names the arm's own chain directory on every slot that carries the field"
    }
}

# TREATMENT-RB at genesis: only current-1 is the arm's own run.
Assert-PanelLocatorChainDirs `
    -Locator (Get-PanelLocator -AttemptRoot $attempt -Interval 0) `
    -ExpectedSlots @(5) `
    -ExpectedChainDir ([string]$treatmentArguments['ChainDir']) `
    -Label 'the treatment-rb genesis panel locator'

# TREATMENT-RB from refresh 4: historical-0 is the arm's own run too, and both
# slots must carry the field.
Assert-PanelLocatorChainDirs `
    -Locator (Get-PanelLocator -AttemptRoot $attempt -Interval 4) `
    -ExpectedSlots @(2, 5) `
    -ExpectedChainDir ([string]$treatmentArguments['ChainDir']) `
    -Label 'the treatment-rb refresh-4 panel locator'

# The slots that do not carry it are still bare strings, so a reader that
# knows nothing of the field still reads them.
$treatmentPanelZero = Get-PanelLocator -AttemptRoot $attempt -Interval 0
$plain = $true
foreach ($index in @(0, 1, 2, 3, 4, 6, 7)) {
    if (-not ($treatmentPanelZero.stores."$index" -is [string])) { $plain = $false }
}
Assert-That -Condition $plain -Message 'slots without a chain directory stay bare store-root strings'
Assert-That -Condition ((Get-PanelSlotStoreRoot -Entry $treatmentPanelZero.stores."5") -ceq [string]$treatmentArguments['StoreRoot']) `
    -Message 'a slot carrying the field still names its store root'

# CONTROL-R has no baseline chain, so no slot carries the field at all.
$controlEvidence = Join-Path $WorkRoot 'evidence-control'
$controlArguments = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot $controlEvidence
& $wrapper @controlArguments *>&1 | Out-Null
$controlAttempt = Get-LatestAttemptRoot -EvidenceRoot $controlEvidence -GateName 'cycle4-control-r-formal'
Assert-That -Condition (-not (Test-Path -LiteralPath (Join-Path $controlAttempt 'TRAINING_COMPLETE'))) `
    -Message 'control-r plans its dry run without publishing a completion marker'
Assert-That -Condition (-not (Test-Path -LiteralPath (Join-Path $controlAttempt 'RUN_FAILED'))) `
    -Message "control-r's dry run passes"
Assert-PanelLocatorChainDirs `
    -Locator (Get-PanelLocator -AttemptRoot $controlAttempt -Interval 0) `
    -ExpectedSlots @() `
    -Label 'the control-r genesis panel locator'
Assert-PanelLocatorChainDirs `
    -Locator (Get-PanelLocator -AttemptRoot $controlAttempt -Interval 4) `
    -ExpectedSlots @() `
    -Label 'the control-r refresh-4 panel locator'
$controlPanelZero = Get-PanelLocator -AttemptRoot $controlAttempt -Interval 0
$allPlain = $true
foreach ($index in 0..7) {
    if (-not ($controlPanelZero.stores."$index" -is [string])) { $allPlain = $false }
}
Assert-That -Condition $allPlain -Message "a control-r panel locator is byte-identical in shape to one written before the field existed"

# ---------------------------------------------------------------------------
# 2. STATIC-RB never advances the manifest
# ---------------------------------------------------------------------------

$staticChain = Join-Path $WorkRoot 'refresh-chain-static'
New-Item -ItemType Directory -Force -Path $staticChain | Out-Null
Copy-Item -LiteralPath (Join-Path $refreshChainDir 'refresh-00.manifest.json') -Destination (Join-Path $staticChain 'refresh-00.manifest.json')

$staticEvidence = Join-Path $WorkRoot 'evidence-static'
$staticArguments = New-WrapperArguments -Mode 'formal' -Arm 'static-rb' -EvidenceRoot $staticEvidence
$staticArguments['RefreshChainDir'] = $staticChain
& $wrapper @staticArguments *>&1 | Out-Null
$staticAttempt = Get-LatestAttemptRoot -EvidenceRoot $staticEvidence -GateName 'cycle4-static-rb-formal'

Assert-That -Condition (-not (Test-Path -LiteralPath (Join-Path $staticAttempt 'RUN_FAILED'))) -Message 'static-rb passes its dry run'
Assert-That -Condition (-not (Test-Path -LiteralPath (Join-Path $staticAttempt 'TRAINING_COMPLETE'))) -Message 'static-rb publishes no completion marker on a dry run'
$staticRecords = Get-CommandRecords -AttemptRoot $staticAttempt
Assert-That -Condition (@($staticRecords | Where-Object { $_.label -like 'arm-interval-*' }).Count -eq 16) -Message 'static-rb still runs all 16 training intervals'
Assert-That -Condition (@($staticRecords | Where-Object { $_.label -like 'panel-interval-*' }).Count -eq 16) -Message 'static-rb still runs the panel every interval'
Assert-That -Condition (@($staticRecords | Where-Object { $_.label -like 'refresh-build-*' }).Count -eq 0) -Message 'static-rb never builds a manifest'
Assert-That -Condition (-not (Test-Path -LiteralPath (Join-Path $staticChain 'refresh-01.manifest.json'))) -Message 'static-rb leaves no manifest past genesis in the chain'
$staticArm = @($staticRecords | Where-Object { $_.label -like 'arm-interval-*' })
$staticUsesGenesis = $true
foreach ($record in $staticArm) {
    if ($record.command_line -notlike '*refresh-00.manifest.json*') { $staticUsesGenesis = $false }
    if ($record.command_line -like '*--payoff-panel*') { $staticUsesGenesis = $false }
}
Assert-That -Condition $staticUsesGenesis -Message 'every static-rb interval reuses the genesis manifest and binds no panel'
# STATIC-RB is a v4 arm, so its own-run slot carries the chain directory even
# though its manifest never advances past genesis.
Assert-PanelLocatorChainDirs `
    -Locator (Get-PanelLocator -AttemptRoot $staticAttempt -Interval 0) `
    -ExpectedSlots @(5) `
    -ExpectedChainDir ([string]$staticArguments['ChainDir']) `
    -Label 'the static-rb genesis panel locator'
Assert-PanelLocatorChainDirs `
    -Locator (Get-PanelLocator -AttemptRoot $staticAttempt -Interval 7) `
    -ExpectedSlots @(5) `
    -ExpectedChainDir ([string]$staticArguments['ChainDir']) `
    -Label 'the static-rb refresh-7 panel locator'

# A manifest that appeared past genesis stops static-rb before it trains.
$intruder = Join-Path $staticChain 'refresh-01.manifest.json'
New-SyntheticManifest -Path $intruder -RefreshIndex ([uint64]1)
$intrudedEvidence = Join-Path $WorkRoot 'evidence-static-intruded'
$intrudedArguments = New-WrapperArguments -Mode 'formal' -Arm 'static-rb' -EvidenceRoot $intrudedEvidence
$intrudedArguments['RefreshChainDir'] = $staticChain
Assert-Throws -Action { & $wrapper @intrudedArguments *>&1 | Out-Null } `
    -ExpectedSubstring 'never advances the manifest past genesis' `
    -Message 'static-rb refuses to run once an advanced manifest exists'
$intrudedAttempt = Get-LatestAttemptRoot -EvidenceRoot $intrudedEvidence -GateName 'cycle4-static-rb-formal'
Assert-That -Condition (Test-Path -LiteralPath (Join-Path $intrudedAttempt 'RUN_FAILED')) -Message 'a rejection publishes RUN_FAILED'
$failure = [System.IO.File]::ReadAllText((Join-Path $intrudedAttempt 'RUN_FAILED'))
Assert-That -Condition ($failure -like '*phase=interval-0*') -Message 'RUN_FAILED names the failing step'
Remove-Item -LiteralPath $intruder -Force

# ---------------------------------------------------------------------------
# 3. CONTROL preflight ladder
# ---------------------------------------------------------------------------

$preflightEvidence = Join-Path $WorkRoot 'evidence-preflight'
$preflightArguments = New-WrapperArguments -Mode 'preflight' -Arm 'control-r' -EvidenceRoot $preflightEvidence
& $wrapper @preflightArguments *>&1 | Out-Null
$preflightAttempt = Get-LatestAttemptRoot -EvidenceRoot $preflightEvidence -GateName 'cycle4-control-r-preflight'

Assert-That -Condition (-not (Test-Path -LiteralPath (Join-Path $preflightAttempt 'PREFLIGHT_COMPLETE'))) `
    -Message 'a dry-run ladder compared nothing, so it publishes no PREFLIGHT_COMPLETE either'
$preflightResult = Get-Content -Raw -LiteralPath (Join-Path $preflightAttempt 'result.json') | ConvertFrom-Json
Assert-That -Condition ([string]$preflightResult.status -ceq 'DRY_RUN_PLANNED') -Message 'the ladder dry run reports a plan'
Assert-That -Condition (-not (Test-Path -LiteralPath (Join-Path $preflightAttempt 'TRAINING_COMPLETE'))) -Message 'the ladder never publishes TRAINING_COMPLETE'
$preflightRecords = Get-CommandRecords -AttemptRoot $preflightAttempt
$preflightBootstraps = @($preflightRecords | Where-Object { $_.label -like 'preflight-bootstrap-*' })
$preflightBuilds = @($preflightRecords | Where-Object { $_.label -like 'preflight-genesis-build-*' })
$preflightTraining = @($preflightRecords | Where-Object { $_.label -like 'preflight-rung-*' })
Assert-That -Condition ($preflightBootstraps.Count -eq 2) -Message "each rung is bootstrapped on its own (saw $($preflightBootstraps.Count))"
Assert-That -Condition ($preflightBuilds.Count -eq 2) -Message "each rung builds its own genesis manifest (saw $($preflightBuilds.Count))"
Assert-That -Condition ($preflightTraining.Count -eq 2) -Message "the ladder runs exactly two rungs (saw $($preflightTraining.Count))"
Assert-That -Condition ($preflightBootstraps[0].command_line -like '*ladder\a\store*' -and $preflightBootstraps[1].command_line -like '*ladder\b\store*') `
    -Message 'each rung bootstraps its own throwaway Store'
Assert-That -Condition ($preflightBuilds[0].command_line -like '*ladder\a\refresh-00.manifest.json*' -and $preflightBuilds[1].command_line -like '*ladder\b\refresh-00.manifest.json*') `
    -Message "each rung's genesis manifest stays inside that rung"
$rungA = $preflightTraining[0].command_line
$rungB = $preflightTraining[1].command_line
# checkpoint_segment_updates is 4 in the synthetic run record, so the smallest
# window that is both at least two updates and a whole number of segments is 4.
Assert-That -Condition ($rungA -like '*"--preflight" "--preflight-updates" "4"*') -Message 'the ladder passes both halves of the preflight pair'
Assert-That -Condition ($rungA -like '*"--stop-generation" "4"*') -Message 'the ladder stops at the relaxed window, not 128'
Assert-That -Condition ($rungA -like '*ladder\a\store*' -and $rungB -like '*ladder\b\store*') -Message 'the two rungs use two independent throwaway Store prefixes under the attempt root'
Assert-That -Condition ($rungA -like "*`"--run-record`" `"$runRecord`"*" -and $rungB -like "*`"--run-record`" `"$runRecord`"*") -Message 'both rungs are seeded from the same run record'
Assert-That -Condition ($rungA -like '*ladder\a\refresh-00.manifest.json*' -and $rungB -like '*ladder\b\refresh-00.manifest.json*') `
    -Message "each rung trains against its own genesis manifest, never the other's"
Assert-That -Condition (Test-Path -LiteralPath (Join-Path $preflightAttempt 'ladder\a\bootstrap-slot-locator.json')) -Message 'each rung gets its own bootstrap locator'
$rungPanelLocator = Join-Path $preflightAttempt 'ladder\a\panel-slot-locator.json'
if (Test-Path -LiteralPath $rungPanelLocator -PathType Leaf) {
    Assert-PanelLocatorChainDirs `
        -Locator (Get-Content -Raw -LiteralPath $rungPanelLocator | ConvertFrom-Json) `
        -ExpectedSlots @() `
        -Label 'the CONTROL preflight rung panel locator'
}
Assert-That -Condition (Test-Path -LiteralPath (Join-Path $preflightAttempt 'cycle4-genesis-authority-control-r.json')) `
    -Message 'a preflight keeps its genesis authority copy inside the throwaway attempt root'

# ---------------------------------------------------------------------------
# 4. Fail-closed rejections
# ---------------------------------------------------------------------------

# Each rejection gets its own evidence root: the genesis authority record is
# campaign-scoped and re-verified on every launch, so two cases that differ in
# their refresh chain would otherwise trip the drift check instead of the
# rejection under test (which is itself the authority record working).
function New-RejectionEvidenceRoot {
    param([Parameter(Mandatory = $true)][string]$Name)
    return (Join-Path $WorkRoot "evidence-reject-$Name")
}

$noDryRun = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'no-dry-run')
$noDryRun['DryRun'] = $false
Assert-Throws -Action { & $wrapper @noDryRun *>&1 | Out-Null } `
    -ExpectedSubstring '-SkipHostAssertions is only accepted together with -DryRun' `
    -Message 'host assertions can never be skipped outside a dry run'

$shortTable = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'short-table')
$shortTable['SlotStoreRoots'] = @($slotStoreRoots[0..6])
Assert-Throws -Action { & $wrapper @shortTable *>&1 | Out-Null } `
    -ExpectedSubstring 'exactly 8 store roots' `
    -Message 'a seven-slot table is rejected'

$noStoreRoot = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'no-store-root')
$noStoreRoot.Remove('StoreRoot') | Out-Null
Assert-Throws -Action { & $wrapper @noStoreRoot *>&1 | Out-Null } `
    -ExpectedSubstring 'formal mode requires -StoreRoot' `
    -Message 'formal mode requires a Store root'

$wrongArm = New-WrapperArguments -Mode 'preflight' -Arm 'treatment-rb' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'wrong-arm')
Assert-Throws -Action { & $wrapper @wrongArm *>&1 | Out-Null } `
    -ExpectedSubstring '-Arm must be control-r' `
    -Message 'the preflight ladder is the CONTROL ladder only'

$duplicateChain = Join-Path $WorkRoot 'refresh-chain-duplicate'
New-SyntheticManifest -Path (Join-Path $duplicateChain 'refresh-00.manifest.json') -RefreshIndex ([uint64]0) -DuplicateIdentity
$duplicateIdentity = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'duplicate-identity')
$duplicateIdentity['RefreshChainDir'] = $duplicateChain
Assert-Throws -Action { & $wrapper @duplicateIdentity *>&1 | Out-Null } `
    -ExpectedSubstring 'repeats a checkpoint_manifest_sha256' `
    -Message 'a roster with a repeated identity cannot key an identity-keyed locator'

$badRoleChain = Join-Path $WorkRoot 'refresh-chain-bad-role'
New-SyntheticManifest -Path (Join-Path $badRoleChain 'refresh-00.manifest.json') -RefreshIndex ([uint64]0)
$badRoleDocument = Get-Content -Raw -LiteralPath (Join-Path $badRoleChain 'refresh-00.manifest.json') | ConvertFrom-Json
$badRoleDocument.slots[3].role = 'current-9'
Write-SyntheticJson -Value $badRoleDocument -Path (Join-Path $badRoleChain 'refresh-00.manifest.json')
$badRole = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'bad-role')
$badRole['RefreshChainDir'] = $badRoleChain
Assert-Throws -Action { & $wrapper @badRole *>&1 | Out-Null } `
    -ExpectedSubstring 'expected historical-1' `
    -Message 'a manifest whose roles drift from the pre-registered roster is rejected'

$relativeSlot = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'relative-slot')
$relativeRoots = @($slotStoreRoots)
$relativeRoots[2] = 'slot-2-relative'
$relativeSlot['SlotStoreRoots'] = $relativeRoots
Assert-Throws -Action { & $wrapper @relativeSlot *>&1 | Out-Null } `
    -ExpectedSubstring 'must be a non-empty absolute path' `
    -Message 'a relative slot store root never reaches a locator'

$duplicateRoots = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'duplicate-roots')
$twinned = @($slotStoreRoots)
$twinned[7] = $twinned[6]
$duplicateRoots['SlotStoreRoots'] = $twinned
Assert-Throws -Action { & $wrapper @duplicateRoots *>&1 | Out-Null } `
    -ExpectedSubstring 'two slots to the same store root' `
    -Message 'one store may not occupy two slots'

$badThrough = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'bad-through')
$badThrough['ThroughRefreshIndex'] = [uint64]17
Assert-Throws -Action { & $wrapper @badThrough *>&1 | Out-Null } `
    -ExpectedSubstring '-ThroughRefreshIndex must be 1..16' `
    -Message 'the campaign never plans past refresh 16'

$badWindow = New-WrapperArguments -Mode 'preflight' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'bad-window')
$badWindow['PreflightUpdates'] = [uint64]2
Assert-Throws -Action { & $wrapper @badWindow *>&1 | Out-Null } `
    -ExpectedSubstring 'not a whole number of checkpoint segments' `
    -Message 'a preflight window that cannot land on its own stop is rejected'

$tooLargeWindow = New-WrapperArguments -Mode 'preflight' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'too-large-window')
$tooLargeWindow['PreflightUpdates'] = [uint64]16
Assert-Throws -Action { & $wrapper @tooLargeWindow *>&1 | Out-Null } `
    -ExpectedSubstring '-PreflightUpdates must be 0 (derive) or 1..8' `
    -Message 'the preflight window stays inside the bin bound'

# ---------------------------------------------------------------------------
# 5. Resuming an interrupted attempt
#
# An attempt can stop anywhere, and latest.json alone cannot say where: a
# boundary generation means either "this interval is finished" or "training
# finished and its panel and next manifest are not", and a generation inside an
# interval means training is mid-flight toward a stop only the launching
# attempt knew. Each interruption point below plants the campaign state a real
# interruption would have left -- chain contents, Store position, and the
# hash-chained journal of a previous (non-dry) attempt -- and asserts the plan.
# ---------------------------------------------------------------------------

$script:ResumeCase = 0

function New-InterruptionCampaign {
    <#
      Builds one interrupted campaign and returns the wrapper arguments for it.
      -ChainThroughManifest / -ChainThroughPanel say how far the refresh chain
      got, -StoreGeneration where the Store stopped, and -JournalPhases the
      per-interval phases a previous attempt journalled.
    #>
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$ArmKind,
        [Parameter(Mandatory = $true)][uint64]$ThroughRefreshIndex,
        [Parameter(Mandatory = $true)][int]$ChainThroughManifest,
        [Parameter(Mandatory = $true)][int]$ChainThroughPanel,
        [Parameter(Mandatory = $true)][uint64]$StoreGeneration,
        [Parameter(Mandatory = $true)][hashtable]$JournalPhases,
        [switch]$JournalFromDryRunAttempt
    )
    $script:ResumeCase++
    $caseRoot = Join-Path $WorkRoot ("resume-{0:d2}-{1}" -f $script:ResumeCase, $Name)
    $chain = Join-Path $caseRoot 'refresh-chain'
    $store = Join-Path $caseRoot 'store'
    $evidence = Join-Path $caseRoot 'evidence'

    foreach ($index in 0..$ChainThroughManifest) {
        New-SyntheticManifest -Path (Join-Path $chain ('refresh-{0:d2}.manifest.json' -f $index)) -RefreshIndex ([uint64]$index)
    }
    if ($ChainThroughPanel -ge 1) {
        foreach ($index in 1..$ChainThroughPanel) {
            $panel = Join-Path $chain ('refresh-{0:d2}.panel.json' -f $index)
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $panel) | Out-Null
            [System.IO.File]::WriteAllText($panel, "{}", [System.Text.UTF8Encoding]::new($false))
        }
    }
    New-Item -ItemType Directory -Force -Path $store | Out-Null
    Write-SyntheticJson -Value ([ordered]@{ generation_index = $StoreGeneration }) -Path (Join-Path $store 'latest.json')

    # The journal a previous attempt would have left, written with the wrapper's
    # own writer so its hash chain is genuine rather than hand-rolled.
    $gate = Join-Path $evidence "cycle4-$ArmKind-formal"
    $planted = Join-Path $gate 'attempt-001'
    New-Item -ItemType Directory -Force -Path $planted | Out-Null
    Write-SyntheticJson -Value ([ordered]@{
        schema = 'mtg-kernel-cycle4-arm-launch-manifest/v1'
        dry_run = [bool]$JournalFromDryRunAttempt
    }) -Path (Join-Path $planted 'launch-manifest.json')
    $journals = @{}
    foreach ($index in @($JournalPhases.Keys | Sort-Object)) {
        $target = [string]$JournalPhases[$index]
        $refresh = $(if ($ArmKind -ceq 'static-rb') { [uint64]0 } else { [uint64]$index })
        foreach ($transition in @('training-started', 'training-complete', 'panel-complete', 'manifest-complete')) {
            Add-Cycle4IntervalPhase `
                -Journals $journals `
                -AttemptRoot $planted `
                -Arm $ArmKind `
                -IntervalIndex ([uint64]$index) `
                -RefreshIndex $refresh `
                -StopGeneration ((([uint64]$index) + [uint64]1) * [uint64]128) `
                -Phase $transition | Out-Null
            if ($transition -ceq $target) { break }
        }
    }

    $arguments = New-WrapperArguments -Mode 'formal' -Arm $ArmKind -EvidenceRoot $evidence
    $arguments['RefreshChainDir'] = $chain
    $arguments['StoreRoot'] = $store
    $arguments['ChainDir'] = (Join-Path $caseRoot 'baseline-chain')
    $arguments['ThroughRefreshIndex'] = $ThroughRefreshIndex
    return [ordered]@{
        arguments = $arguments
        chain = $chain
        store = $store
        evidence = $evidence
        gate = $gate
        planted_attempt = $planted
    }
}

function Invoke-ResumeCase {
    param([Parameter(Mandatory = $true)]$Case)
    $arguments = $Case.arguments
    & $wrapper @arguments *>&1 | Out-Null
    $attempt = Get-LatestAttemptRoot -EvidenceRoot $Case.evidence -GateName (Split-Path -Leaf $Case.gate)
    return [ordered]@{
        attempt = $attempt
        records = @(Get-CommandRecords -AttemptRoot $attempt)
        result = (Get-Content -Raw -LiteralPath (Join-Path $attempt 'result.json') | ConvertFrom-Json)
    }
}

# (i) Interrupted mid-training: latest.json sits on a checkpoint segment inside
# interval 3, which the old boundary-only rule rejected outright.
$midTraining = New-InterruptionCampaign -Name 'mid-training' -ArmKind 'treatment-rb' `
    -ThroughRefreshIndex ([uint64]4) -ChainThroughManifest 3 -ChainThroughPanel 3 `
    -StoreGeneration ([uint64]388) -JournalPhases @{ 0 = 'manifest-complete'; 1 = 'manifest-complete'; 2 = 'manifest-complete'; 3 = 'training-started' }
$run = Invoke-ResumeCase -Case $midTraining
$arm = @($run.records | Where-Object { $_.label -like 'arm-interval-*' })
Assert-That -Condition ($arm.Count -eq 1 -and $arm[0].label -ceq 'arm-interval-3') `
    -Message 'a mid-interval Store resumes exactly the interrupted interval'
Assert-That -Condition ($arm[0].command_line -like '*"--stop-generation" "512"*') `
    -Message 'a mid-interval resume uses the interval original stop generation, not the Store position plus 128'
Assert-That -Condition (@($run.records | Where-Object { $_.label -like 'panel-interval-*' }).Count -eq 1) `
    -Message 'a mid-interval resume still owes its panel'
Assert-That -Condition (@($run.records | Where-Object { $_.label -like 'refresh-build-*' }).Count -eq 1) `
    -Message 'a mid-interval resume still owes its next manifest'

# (ii) Interrupted after training, before the panel. This is the reported
# defect: the old logic mapped the boundary to the NEXT interval, skipped the
# unfinished panel and build, and at the program end published
# TRAINING_COMPLETE with both missing.
$afterTraining = New-InterruptionCampaign -Name 'after-training' -ArmKind 'treatment-rb' `
    -ThroughRefreshIndex ([uint64]4) -ChainThroughManifest 3 -ChainThroughPanel 3 `
    -StoreGeneration ([uint64]512) -JournalPhases @{ 0 = 'manifest-complete'; 1 = 'manifest-complete'; 2 = 'manifest-complete'; 3 = 'training-complete' }
$run = Invoke-ResumeCase -Case $afterTraining
Assert-That -Condition (@($run.records | Where-Object { $_.label -like 'arm-interval-*' }).Count -eq 0) `
    -Message 'an interval already trained to its stop generation is not retrained'
$panel = @($run.records | Where-Object { $_.label -like 'panel-interval-*' })
$build = @($run.records | Where-Object { $_.label -like 'refresh-build-*' })
Assert-That -Condition ($panel.Count -eq 1 -and $panel[0].label -ceq 'panel-interval-3') `
    -Message 'the final interval pending panel is run before anything completes'
Assert-That -Condition ($build.Count -eq 1 -and $build[0].command_line -like '*"--next-generation" "4"*') `
    -Message 'the final interval pending manifest is built before anything completes'
Assert-That -Condition (@($run.result.intervals_planned).Count -eq 1) `
    -Message 'the result records exactly the interval that still owed work'

# (iii) Interrupted after the panel reached the chain, before the build.
$afterPanel = New-InterruptionCampaign -Name 'after-panel' -ArmKind 'treatment-rb' `
    -ThroughRefreshIndex ([uint64]4) -ChainThroughManifest 3 -ChainThroughPanel 4 `
    -StoreGeneration ([uint64]512) -JournalPhases @{ 0 = 'manifest-complete'; 1 = 'manifest-complete'; 2 = 'manifest-complete'; 3 = 'panel-complete' }
$run = Invoke-ResumeCase -Case $afterPanel
Assert-That -Condition (@($run.records | Where-Object { $_.label -like 'arm-interval-*' }).Count -eq 0) `
    -Message 'a published panel does not retrain its interval'
Assert-That -Condition (@($run.records | Where-Object { $_.label -like 'panel-interval-*' }).Count -eq 0) `
    -Message 'a panel already in the refresh chain is never re-run'
$build = @($run.records | Where-Object { $_.label -like 'refresh-build-*' })
Assert-That -Condition ($build.Count -eq 1 -and $build[0].command_line -like '*"--next-generation" "4"*') `
    -Message 'only the missing manifest is built'

# (iv) Nothing left: every interval trained, every panel and manifest present.
$complete = New-InterruptionCampaign -Name 'complete' -ArmKind 'treatment-rb' `
    -ThroughRefreshIndex ([uint64]4) -ChainThroughManifest 4 -ChainThroughPanel 4 `
    -StoreGeneration ([uint64]512) -JournalPhases @{ 0 = 'manifest-complete'; 1 = 'manifest-complete'; 2 = 'manifest-complete'; 3 = 'manifest-complete' }
$run = Invoke-ResumeCase -Case $complete
Assert-That -Condition (@($run.records | Where-Object { $_.label -cne 'run-record' }).Count -eq 0) `
    -Message 'a finished campaign plans no work beyond re-deriving its run record'
Assert-That -Condition ([string]$run.result.status -ceq 'DRY_RUN_PLANNED') `
    -Message 'a finished campaign still only plans under -DryRun'

# (v) STATIC-RB, whose panel never enters the refresh chain, so only the
# journal can say whether it ran.
$staticPending = New-InterruptionCampaign -Name 'static-pending' -ArmKind 'static-rb' `
    -ThroughRefreshIndex ([uint64]2) -ChainThroughManifest 0 -ChainThroughPanel 0 `
    -StoreGeneration ([uint64]256) -JournalPhases @{ 0 = 'panel-complete'; 1 = 'training-complete' }
$run = Invoke-ResumeCase -Case $staticPending
$panel = @($run.records | Where-Object { $_.label -like 'panel-interval-*' })
Assert-That -Condition ($panel.Count -eq 1 -and $panel[0].label -ceq 'panel-interval-1') `
    -Message "static-rb re-runs only the interval whose panel the journal does not record"
Assert-That -Condition (@($run.records | Where-Object { $_.label -like 'refresh-build-*' }).Count -eq 0) `
    -Message 'static-rb never builds a manifest on resume either'

# The same state, but the journal came from a DRY RUN: it plans work rather
# than performing it, so its records must never be read back as progress.
$staticDryJournal = New-InterruptionCampaign -Name 'static-dry-journal' -ArmKind 'static-rb' `
    -ThroughRefreshIndex ([uint64]2) -ChainThroughManifest 0 -ChainThroughPanel 0 `
    -StoreGeneration ([uint64]256) -JournalPhases @{ 0 = 'panel-complete'; 1 = 'panel-complete' } -JournalFromDryRunAttempt
$run = Invoke-ResumeCase -Case $staticDryJournal
Assert-That -Condition (@($run.records | Where-Object { $_.label -like 'panel-interval-*' }).Count -eq 2) `
    -Message 'a dry-run journal is never mistaken for completed work'

# (vi) A journal that has been edited or truncated stops the launch.
$tampered = New-InterruptionCampaign -Name 'tampered-journal' -ArmKind 'treatment-rb' `
    -ThroughRefreshIndex ([uint64]4) -ChainThroughManifest 3 -ChainThroughPanel 3 `
    -StoreGeneration ([uint64]512) -JournalPhases @{ 0 = 'manifest-complete'; 1 = 'manifest-complete'; 2 = 'manifest-complete'; 3 = 'training-complete' }
$tamperedPath = Join-Path $tampered.planted_attempt 'interval-02.phase.json'
$tamperedJournal = Get-Content -Raw -LiteralPath $tamperedPath | ConvertFrom-Json
$tamperedJournal.records[1].utc = '2020-01-01T00:00:00.0000000+00:00'
Write-SyntheticJson -Value $tamperedJournal -Path $tamperedPath
$tamperedArguments = $tampered.arguments
Assert-Throws -Action { & $wrapper @tamperedArguments *>&1 | Out-Null } `
    -ExpectedSubstring 'does not match its own digest' `
    -Message 'an edited journal record is refused'

$truncated = New-InterruptionCampaign -Name 'truncated-journal' -ArmKind 'treatment-rb' `
    -ThroughRefreshIndex ([uint64]4) -ChainThroughManifest 3 -ChainThroughPanel 3 `
    -StoreGeneration ([uint64]512) -JournalPhases @{ 0 = 'manifest-complete'; 1 = 'manifest-complete'; 2 = 'manifest-complete'; 3 = 'training-complete' }
Remove-Item -LiteralPath (Join-Path $truncated.planted_attempt 'interval-01.phase.json') -Force
$truncatedArguments = $truncated.arguments
Assert-Throws -Action { & $wrapper @truncatedArguments *>&1 | Out-Null } `
    -ExpectedSubstring 'not chained to its predecessor' `
    -Message 'a journal with an interval removed is refused'

# (vii) A journal that claims a panel the refresh chain no longer holds.
$lostPanel = New-InterruptionCampaign -Name 'lost-panel' -ArmKind 'treatment-rb' `
    -ThroughRefreshIndex ([uint64]4) -ChainThroughManifest 3 -ChainThroughPanel 3 `
    -StoreGeneration ([uint64]512) -JournalPhases @{ 0 = 'manifest-complete'; 1 = 'manifest-complete'; 2 = 'manifest-complete'; 3 = 'panel-complete' }
$lostPanelArguments = $lostPanel.arguments
Assert-Throws -Action { & $wrapper @lostPanelArguments *>&1 | Out-Null } `
    -ExpectedSubstring 'refusing to silently re-run a 28-matchup panel' `
    -Message 'a journalled panel missing from the chain stops the launch instead of costing a panel'

# (viii) A Store that stopped somewhere the Store itself could never stop.
$offSegment = New-InterruptionCampaign -Name 'off-segment' -ArmKind 'treatment-rb' `
    -ThroughRefreshIndex ([uint64]4) -ChainThroughManifest 3 -ChainThroughPanel 3 `
    -StoreGeneration ([uint64]385) -JournalPhases @{ 0 = 'manifest-complete'; 1 = 'manifest-complete'; 2 = 'manifest-complete'; 3 = 'training-started' }
$offSegmentArguments = $offSegment.arguments
Assert-Throws -Action { & $wrapper @offSegmentArguments *>&1 | Out-Null } `
    -ExpectedSubstring 'not a whole number of checkpoint segments' `
    -Message 'a Store position no checkpoint segment could produce is refused'

$pastEnd = New-InterruptionCampaign -Name 'past-end' -ArmKind 'treatment-rb' `
    -ThroughRefreshIndex ([uint64]4) -ChainThroughManifest 4 -ChainThroughPanel 4 `
    -StoreGeneration ([uint64]640) -JournalPhases @{ 0 = 'manifest-complete' }
$pastEndArguments = $pastEnd.arguments
Assert-Throws -Action { & $wrapper @pastEndArguments *>&1 | Out-Null } `
    -ExpectedSubstring 'past the refresh 4 end' `
    -Message 'a Store past the requested end is refused'

# ---------------------------------------------------------------------------
# 6. The genesis binding assertion itself
#
# The wrapper's whole reason for bootstrapping first is that the genesis
# manifest must bind the checkpoint the Store actually published. Drive that
# assertion directly against a synthetic Store, origin record and manifest,
# since a dry run never produces the real three.
# ---------------------------------------------------------------------------

$bindingRoot = Join-Path $WorkRoot 'genesis-binding'
$bindingStore = Join-Path $bindingRoot 'store'
$bindingChain = Join-Path $bindingRoot 'chain'
New-Item -ItemType Directory -Force -Path (Join-Path $bindingStore 'checkpoints') | Out-Null
New-Item -ItemType Directory -Force -Path $bindingChain | Out-Null

$payloadSha = New-SyntheticHash -Tag 0xb1
$modelSha = New-SyntheticHash -Tag 0xb2
$stateSha = New-SyntheticHash -Tag 0xb3
$checkpointPath = Join-Path $bindingStore 'checkpoints\update-00000000.checkpoint.json'
Write-SyntheticJson -Value ([ordered]@{
    schema = 'mtg-kernel-native-checkpoint/v3'
    generation_index = 0
    train_state = [ordered]@{ model_parameter_sha256 = $modelSha; state_sha256 = $stateSha }
    payload = [ordered]@{ sha256 = $payloadSha }
}) -Path $checkpointPath
# The Store's checkpoint_manifest_sha256 is the SHA-256 of these exact bytes,
# so it can only be computed after the file is written.
$checkpointSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $checkpointPath).Hash.ToLowerInvariant()

function Write-BindingOriginRecord {
    param([Parameter(Mandatory = $true)][string]$ManifestSha256)
    Write-SyntheticJson -Value ([ordered]@{
        schema = 'mtg-kernel-cycle4-arm-origin/v1'
        arm_kind = 'treatment-rb'
        run_sha256 = $traineeRunSha256
        base_seed = $traineeBaseSeed
        init_generation = 2048
        genesis_checkpoint_manifest_sha256 = $ManifestSha256
        genesis_checkpoint_payload_sha256 = $payloadSha
        genesis_model_parameter_sha256 = $modelSha
        genesis_train_state_sha256 = $stateSha
    }) -Path (Join-Path $bindingChain 'arm-origin.record.json')
}

function Write-BindingManifest {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$ManifestSha256
    )
    New-SyntheticManifest -Path $Path -RefreshIndex ([uint64]0)
    $document = Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json
    $document.slots[5].checkpoint_manifest_sha256 = $ManifestSha256
    $document.slots[5].checkpoint_payload_sha256 = $payloadSha
    $document.slots[5].model_parameter_sha256 = $modelSha
    $document.slots[5].train_state_sha256 = $stateSha
    Write-SyntheticJson -Value $document -Path $Path
}

$bindingManifestPath = Join-Path $bindingRoot 'refresh-00.manifest.json'
Write-BindingOriginRecord -ManifestSha256 $checkpointSha
Write-BindingManifest -Path $bindingManifestPath -ManifestSha256 $checkpointSha

$binding = Assert-Cycle4GenesisManifestBinding `
    -Manifest (Read-Cycle4Manifest -Path $bindingManifestPath) `
    -Origin (Read-Cycle4ArmOriginRecord -ChainDir $bindingChain) `
    -ArmStoreRoot $bindingStore
Assert-That -Condition ($binding.genesis_checkpoint_manifest_sha256 -ceq $checkpointSha) `
    -Message 'a genesis manifest that binds the Store checkpoint passes'
Assert-That -Condition ($binding.trainee_local_generation -eq 896) `
    -Message 'the own-run slot is checked at trainee-local 896'

# A manifest whose own-run slot names some other checkpoint is refused, even
# though the origin record and the Store still agree with each other.
$staleManifest = Join-Path $bindingRoot 'refresh-00.stale.json'
Write-BindingManifest -Path $staleManifest -ManifestSha256 (New-SyntheticHash -Tag 0xcc)
Assert-Throws -Action {
    Assert-Cycle4GenesisManifestBinding `
        -Manifest (Read-Cycle4Manifest -Path $staleManifest) `
        -Origin (Read-Cycle4ArmOriginRecord -ChainDir $bindingChain) `
        -ArmStoreRoot $bindingStore
} -ExpectedSubstring 'does not equal the arm-origin record' `
  -Message "a genesis manifest binding the wrong checkpoint is refused"

# And an origin record that disagrees with the Store is refused too, so the
# check cannot be satisfied by two agreeing copies of one wrong value.
Write-BindingOriginRecord -ManifestSha256 (New-SyntheticHash -Tag 0xcc)
Write-BindingManifest -Path $staleManifest -ManifestSha256 (New-SyntheticHash -Tag 0xcc)
Assert-Throws -Action {
    Assert-Cycle4GenesisManifestBinding `
        -Manifest (Read-Cycle4Manifest -Path $staleManifest) `
        -Origin (Read-Cycle4ArmOriginRecord -ChainDir $bindingChain) `
        -ArmStoreRoot $bindingStore
} -ExpectedSubstring "does not equal the Store's own generation-0 checkpoint" `
  -Message 'an origin record that disagrees with the Store is refused'

# The slot-identities staging the genesis build feeds the builder derives that
# same identity from the Store, which is why the three ever agree.
Write-BindingOriginRecord -ManifestSha256 $checkpointSha
$stagedGenesis = Join-Path $bindingRoot 'refresh-00.slot-identities.json'
New-Cycle4SlotIdentitiesFile `
    -RosterPath (Join-Path $rosterDir 'refresh-00.slot-identities.json') `
    -OutputPath $stagedGenesis `
    -RefreshIndex ([uint64]0) `
    -ArmStoreRoot $bindingStore `
    -ArmRunSha256 $traineeRunSha256 `
    -ArmBaseSeed $traineeBaseSeed | Out-Null
$stagedDocument = Get-Content -Raw -LiteralPath $stagedGenesis | ConvertFrom-Json
Assert-That -Condition ([string]$stagedDocument.slots[5].checkpoint_manifest_sha256 -ceq $checkpointSha) `
    -Message "the genesis roster staging reads the own-run slot from the Store's generation-0 checkpoint"
Assert-That -Condition ([uint64]$stagedDocument.slots[5].source_generation -eq 896) `
    -Message 'the staged own-run slot is labelled trainee-local 896'
Assert-That -Condition ([string]$stagedDocument.slots[5].source_run_sha256 -ceq $traineeRunSha256) `
    -Message "the staged own-run slot carries the arm's run identity"
Assert-That -Condition ([string]$stagedDocument.slots[0].checkpoint_manifest_sha256 -ceq (New-SyntheticHash -Tag 0x20)) `
    -Message 'every pinned slot passes through the staging unchanged'

# ---------------------------------------------------------------------------
# 7. Round E: the derived run record, the historical-1 rotation, the shared
#    store root, the genesis-parent cross-check, and dry-run quietness.
# ---------------------------------------------------------------------------

# (a) The run record is DERIVED unless the operator explicitly overrides it.
$existingRecord = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (Join-Path $WorkRoot 'evidence-existing-record')
$existingRecord.Remove('RunRecordExecutable') | Out-Null
$existingRecord['UseExistingRunRecord'] = $true
& $wrapper @existingRecord *>&1 | Out-Null
$existingAttempt = Get-LatestAttemptRoot -EvidenceRoot $existingRecord['EvidenceRoot'] -GateName 'cycle4-control-r-formal'
Assert-That -Condition (-not (Test-Path -LiteralPath (Join-Path $existingAttempt 'RUN_FAILED'))) `
    -Message '-UseExistingRunRecord runs the campaign from the record as given'
Assert-That -Condition (@(Get-CommandRecords -AttemptRoot $existingAttempt | Where-Object { $_.label -ceq 'run-record' }).Count -eq 0) `
    -Message '-UseExistingRunRecord derives nothing'

# (b) Neither flag is a launch: the record has to come from somewhere.
$noBuilder = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'no-run-record-builder')
$noBuilder.Remove('RunRecordExecutable') | Out-Null
Assert-Throws -Action { & $wrapper @noBuilder *>&1 | Out-Null } `
    -ExpectedSubstring 'requires -RunRecordExecutable' `
    -Message 'a launch that can neither derive nor be given a run record is refused'

# (c) Both at once is a contradiction, not a preference.
$bothRecordFlags = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'both-run-record-flags')
$bothRecordFlags['UseExistingRunRecord'] = $true
Assert-Throws -Action { & $wrapper @bothRecordFlags *>&1 | Out-Null } `
    -ExpectedSubstring 'mutually exclusive' `
    -Message 'deriving and supplying the run record are mutually exclusive'

# (d) A dry run over a campaign whose run record does not exist yet prints the
# derivation and stops there, publishing a plan rather than a completion.
$absentRecordRoot = Join-Path $WorkRoot 'absent-run-record'
New-Item -ItemType Directory -Force -Path $absentRecordRoot | Out-Null
$absentRecord = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (Join-Path $WorkRoot 'evidence-absent-record')
$absentRecord['RunRecord'] = (Join-Path $absentRecordRoot 'run.json')
& $wrapper @absentRecord *>&1 | Out-Null
$absentAttempt = Get-LatestAttemptRoot -EvidenceRoot $absentRecord['EvidenceRoot'] -GateName 'cycle4-control-r-formal'
$absentResult = Get-Content -Raw -LiteralPath (Join-Path $absentAttempt 'result.json') | ConvertFrom-Json
Assert-That -Condition ([string]$absentResult.dry_run_stopped_after -ceq 'run-record') `
    -Message 'a dry run with no run record on disk stops after planning its derivation'
Assert-That -Condition ([string]$absentResult.status -ceq 'DRY_RUN_PLANNED') `
    -Message 'that dry run publishes a plan, not a completion'
Assert-That -Condition (-not (Test-Path -LiteralPath (Join-Path $absentAttempt 'TRAINING_COMPLETE'))) `
    -Message 'and it leaves no completion marker behind'
Assert-That -Condition (-not (Test-Path -LiteralPath $absentRecord['RunRecord'])) `
    -Message 'a dry run writes no run record of its own'

# (e) -GenesisParentGeneration is cross-checked against the run record's pinned
# origin, so the README's old 2048 can no longer bind the wrong parent.
$wrongGeneration = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'wrong-parent-generation')
$wrongGeneration['GenesisParentGeneration'] = [uint64]2048
Assert-Throws -Action { & $wrapper @wrongGeneration *>&1 | Out-Null } `
    -ExpectedSubstring 'genesis authority: parent artifact is missing' `
    -Message 'a parent generation with no artifacts under it is refused'

$mismatchedRecordRoot = Join-Path $WorkRoot 'mismatched-origin'
New-Item -ItemType Directory -Force -Path $mismatchedRecordRoot | Out-Null
$mismatchedRecord = Join-Path $mismatchedRecordRoot 'run.json'
Write-SyntheticRunRecord -Path $mismatchedRecord -Generation ([uint64]1024)
$wrongPin = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'wrong-origin-generation')
$wrongPin['RunRecord'] = $mismatchedRecord
$wrongPin.Remove('RunRecordExecutable') | Out-Null
$wrongPin['UseExistingRunRecord'] = $true
Assert-Throws -Action { & $wrapper @wrongPin *>&1 | Out-Null } `
    -ExpectedSubstring "disagrees with the run record's pinned origin generation" `
    -Message 'a -GenesisParentGeneration the run record does not pin is refused'

$wrongHashRecord = Join-Path $mismatchedRecordRoot 'run-wrong-hash.json'
Write-SyntheticRunRecord -Path $wrongHashRecord -Generation $parentGeneration -CheckpointSha256 (New-SyntheticHash -Tag 0xe1)
$wrongHash = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'wrong-origin-hash')
$wrongHash['RunRecord'] = $wrongHashRecord
$wrongHash.Remove('RunRecordExecutable') | Out-Null
$wrongHash['UseExistingRunRecord'] = $true
Assert-Throws -Action { & $wrapper @wrongHash *>&1 | Out-Null } `
    -ExpectedSubstring 'does not reproduce the run record' `
    -Message 'a parent store whose artifacts do not hash to the pinned origin is refused'

$noContractsRecord = Join-Path $mismatchedRecordRoot 'run-no-contracts.json'
Write-SyntheticJson -Value ([ordered]@{
    schema = 'mtg-kernel-native-train-run/v2'
    schedule = [ordered]@{ base_seed = $traineeBaseSeed; checkpoint_segment_updates = 4; batch_episodes = 64 }
}) -Path $noContractsRecord
$noContracts = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'no-contracts-record')
$noContracts['RunRecord'] = $noContractsRecord
$noContracts.Remove('RunRecordExecutable') | Out-Null
$noContracts['UseExistingRunRecord'] = $true
Assert-Throws -Action { & $wrapper @noContracts *>&1 | Out-Null } `
    -ExpectedSubstring 'declares no contracts section' `
    -Message 'a run record with no contracts section is refused with a readable message, not a strict-mode error'

# (f) historical-1 rotates: slot 3 names a different Store at every phase, and
# every one of them is proven against that boundary's manifest identity.
function Get-ArmLocatorSlotRoot {
    param(
        [Parameter(Mandatory = $true)][string]$AttemptRoot,
        [Parameter(Mandatory = $true)][int]$Interval,
        [Parameter(Mandatory = $true)][int]$Slot
    )
    $path = Join-Path $AttemptRoot ('interval-{0:d2}\arm-slot-locator.json' -f $Interval)
    $locator = Get-Content -Raw -LiteralPath $path | ConvertFrom-Json
    return [string]$locator.stores[$Slot].store_root
}

$rotates = $true
foreach ($interval in 0..5) {
    $expected = [string]$historicalOneRoots[$interval % 3]
    if ((Get-ArmLocatorSlotRoot -AttemptRoot $attempt -Interval $interval -Slot 3) -cne $expected) { $rotates = $false }
}
Assert-That -Condition $rotates `
    -Message 'slot 3 takes the rotation root for refresh_index mod 3 at every boundary'
Assert-That -Condition ((Get-ArmLocatorSlotRoot -AttemptRoot $attempt -Interval 0 -Slot 3) -cne (Get-ArmLocatorSlotRoot -AttemptRoot $attempt -Interval 1 -Slot 3)) `
    -Message 'consecutive refreshes really do name different historical-1 Stores'

# (g) One physical Store may occupy two slots at DIFFERENT pinned generations:
# anchor-1 at 1536 and historical-1's middle phase at 1024 are the real
# roster's shape, and the locator writer must accept it.
Assert-That -Condition ((Get-ArmLocatorSlotRoot -AttemptRoot $attempt -Interval 1 -Slot 1) -ceq (Get-ArmLocatorSlotRoot -AttemptRoot $attempt -Interval 1 -Slot 3)) `
    -Message 'anchor-1 and the middle rotation phase share one Store root at refresh 1'
Assert-That -Condition (Test-Path -LiteralPath (Join-Path $attempt 'interval-01\panel-slot-locator.json')) `
    -Message 'and the interval that shares a root still writes both locators'

# (h) A mis-ordered rotation triple names the wrong Store, and the slot-3
# content-hash check is what catches it.
$wrongRotation = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'wrong-rotation-order')
$wrongRotation['HistoricalOneStoreRoots'] = @($historicalOneRoots[1], $historicalOneRoots[0], $historicalOneRoots[2])
Assert-Throws -Action { & $wrapper @wrongRotation *>&1 | Out-Null } `
    -ExpectedSubstring 'does not match the manifest identity' `
    -Message 'a rotation triple in the wrong order is refused at the first boundary it is wrong for'

# (i) Omitting the triple entirely leaves slot 3 pinned to one Store, which is
# right at refresh 0 and wrong from refresh 1 -- and fails closed there.
$noRotation = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'no-rotation-triple')
$noRotation.Remove('HistoricalOneStoreRoots') | Out-Null
Assert-Throws -Action { & $wrapper @noRotation *>&1 | Out-Null } `
    -ExpectedSubstring 'does not match the manifest identity' `
    -Message 'a single fixed slot-3 root cannot express the rotation and is caught, not silently trained against'

$shortRotation = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'short-rotation-triple')
$shortRotation['HistoricalOneStoreRoots'] = @($historicalOneRoots[0], $historicalOneRoots[1])
Assert-Throws -Action { & $wrapper @shortRotation *>&1 | Out-Null } `
    -ExpectedSubstring '-HistoricalOneStoreRoots must name exactly 3 store roots' `
    -Message 'a rotation triple that is not a triple is refused'

# (j) historical-0's four content hashes are proven against the cycle-3 Store
# before refresh 4, which is the only place they are checked at all.
$badSlotTwoChain = Join-Path $WorkRoot 'refresh-chain-bad-slot-2'
New-SyntheticManifest -Path (Join-Path $badSlotTwoChain 'refresh-00.manifest.json') -RefreshIndex ([uint64]0)
$badSlotTwoDocument = Get-Content -Raw -LiteralPath (Join-Path $badSlotTwoChain 'refresh-00.manifest.json') | ConvertFrom-Json
$badSlotTwoDocument.slots[2].train_state_sha256 = New-SyntheticHash -Tag 0xe2
Write-SyntheticJson -Value $badSlotTwoDocument -Path (Join-Path $badSlotTwoChain 'refresh-00.manifest.json')
$badSlotTwo = New-WrapperArguments -Mode 'formal' -Arm 'control-r' -EvidenceRoot (New-RejectionEvidenceRoot -Name 'bad-slot-2-identity')
$badSlotTwo['RefreshChainDir'] = $badSlotTwoChain
Assert-Throws -Action { & $wrapper @badSlotTwo *>&1 | Out-Null } `
    -ExpectedSubstring 'train_state_sha256' `
    -Message "historical-0's content hashes are proven against the cycle-3 Store, not taken on trust"

# (k) A fresh static-rb campaign plans quietly: manifestDone is a constant for
# an arm that never builds, and must not be read as "an output is present".
$quietChain = Join-Path $WorkRoot 'refresh-chain-static-quiet'
New-Item -ItemType Directory -Force -Path $quietChain | Out-Null
Copy-Item -LiteralPath (Join-Path $refreshChainDir 'refresh-00.manifest.json') -Destination (Join-Path $quietChain 'refresh-00.manifest.json')
$quietArguments = New-WrapperArguments -Mode 'formal' -Arm 'static-rb' -EvidenceRoot (Join-Path $WorkRoot 'evidence-static-quiet')
$quietArguments['RefreshChainDir'] = $quietChain
$quietOutput = (& $wrapper @quietArguments *>&1 | Out-String)
Assert-That -Condition ($quietOutput -notlike '*outputs are present*') `
    -Message 'a fresh static-rb campaign prints no outputs-present inconsistency for any interval'
Assert-That -Condition ($quietOutput -like '*DRY RUN PLANNED*') `
    -Message 'and it still reports the plan it made'

# ---------------------------------------------------------------------------

Write-Host ''
Write-Host "cycle-4 wrapper dry-run tests: $($script:Checks - $script:Failures)/$($script:Checks) checks passed"
Write-Host "work root: $WorkRoot"
if ($script:Failures -ne 0) {
    Write-Host "FAILURES: $($script:Failures)"
    exit 1
}
exit 0
