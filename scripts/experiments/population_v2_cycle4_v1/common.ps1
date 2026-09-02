Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Cycle-4 launch stack, shared helpers (docs/native_cycle4_arm_launcher_v1.md
# Section 6). Dot-sources the regularized_continuation_retest_v1 chain so
# every lower-level helper the contract names is REUSED unchanged rather than
# reimplemented: Get-ToolchainRecord, Assert-GpuIdentity, Assert-Gpu1Idle,
# Assert-NoForeignGpu1ComputeProcesses, Get-StoreTreeHash,
# Get-StoreFileInventory, New-UniqueAttemptRoot, Write-JsonFile,
# Write-Utf8NoBomJsonFile, Assert-LastExitCode, Get-TextSha256, Stop-ProcessTree.
#
# What this file adds is only what is cycle-4's own: the pre-registered
# constants, manifest/locator/identity documents, the Store-position and
# checkpoint-identity readers, the child-process runner with the
# WaitForExit()+Refresh() double call, and the terminal markers.
#
# Everything here is PowerShell 5.1 compatible: no `&&`, no ternary, no
# null-coalescing, and no reliance on Get-FileHash (see the shadow below).

. (Join-Path $PSScriptRoot '..\regularized_continuation_retest_v1\common.ps1')

# ---------------------------------------------------------------------------
# Self-contained .NET SHA-256 (ported from the g896 formal wrapper family).
#
# A detached PowerShell host does not reliably have the Microsoft.PowerShell
# .Utility cmdlet Get-FileHash available, and the g896 formal CONTROL run lost
# a wrapper verdict to exactly that class of host-provided-cmdlet surprise.
# Defining a function of the same name AFTER the dot-source above makes every
# call in this whole stack -- including the precedent's own Get-StoreTreeHash
# and Get-StoreFileInventory -- resolve to this implementation, which depends
# on nothing but the .NET base class library.
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

# ---------------------------------------------------------------------------
# Pre-registered constants (OX_CYCLE4_PREREG_SKETCH_V2 through the ratified
# defaults). Every one of these is also a compiled constant on the Rust side
# (native_population_refresh_manifest_cycle4_v1.rs); they are restated here
# only so the wrapper can reject a mismatch BEFORE spending GPU time, never as
# an independent source of truth.
# ---------------------------------------------------------------------------
$script:Cycle4Arms = @('control-r', 'static-rb', 'treatment-rb')
$script:Cycle4RefreshInterval = [uint64]128
$script:Cycle4MaxRefreshIndex = [uint64]16
$script:Cycle4SlotCount = 8
$script:Cycle4PanelGamesPerMatchup = [uint64]256
$script:Cycle4TraineeStartLocalGeneration = [uint64]896
$script:Cycle4HistoricalLag = [uint64]512
$script:Cycle4StoreGenerationTotal = [uint64]2048
$script:Cycle4PreflightMaxUpdates = [uint64]8
$script:Cycle4ExpectedRoles = @(
    'anchor-0', 'anchor-1', 'historical-0', 'historical-1',
    'current-0', 'current-1', 'exploiter-0', 'exploiter-1'
)
# Slots the ARM's own Store occupies, and therefore the only slot identities
# this wrapper ever derives rather than reads from the operator's roster.
# Slot 5 (current-1) at EVERY refresh including genesis, where it binds the
# arm's own generation-0 checkpoint; slot 2 (historical-0) from refresh 4,
# before which historical-0 is still the cycle-3 lineage.
$script:Cycle4ArmOwnedSlotIndex = 5
$script:Cycle4HistoricalArmSlotIndex = 2
$script:Cycle4HistoricalArmFirstRefreshIndex = [uint64]4
# historical-1 (slot 3) is not one frozen occupant but a THREE-phase rotation
# over the program-v1 endpoints 970001/970002/970003, all at generation 1024,
# selected by `refresh_index % 3` -- the same arithmetic
# validate_slot_assignment_cycle4_v1 performs against
# CYCLE4_HISTORICAL_1_ROTATION_V1. One fixed slot-root table therefore cannot
# express the campaign: two thirds of the refreshes would name the wrong
# Store. -HistoricalOneStoreRoots supplies the three roots in rotation order
# and Get-Cycle4SlotTableForRefresh picks the phase.
$script:Cycle4HistoricalRotationSlotIndex = 3
$script:Cycle4HistoricalRotationPeriod = [uint64]3
$script:Cycle4ArmOriginRecordSchema = 'mtg-kernel-cycle4-arm-origin/v1'
$script:Cycle4IntervalPhaseSchema = 'mtg-kernel-cycle4-interval-phase/v1'
# The four transitions one interval passes through, in order. Every one of
# them is a point an interrupted attempt can stop at, and each is durable
# before the next begins, so the phase a journal last recorded plus the Store
# and chain contents determine exactly what is left to do.
$script:Cycle4IntervalPhases = @('training-started', 'training-complete', 'panel-complete', 'manifest-complete')
$script:Cycle4PhaseChainGenesisParent = ('0' * 64)

$script:Cycle4ManifestSchema = 'mtg-kernel-population-refresh-manifest-cycle4/v1'
$script:Cycle4ArmLocatorSchema = 'mtg-kernel-cycle4-arm-slot-locator/v1'
$script:Cycle4PanelLocatorSchema = 'mtg-kernel-cycle4-slot-locator/v1'
$script:Cycle4SlotIdentitiesSchema = 'mtg-kernel-cycle4-slot-identities/v1'
$script:Cycle4GenesisAuthoritySchema = 'mtg-kernel-cycle4-genesis-authority/v1'
$script:Cycle4ArmOriginRecordFileName = 'arm-origin.record.json'
$script:Cycle4ModeMarkerFileName = 'cycle4-arm-mode.marker.json'

# The panel runner's own EVAL_SEED_STRIDE is 1,000,000 per matchup and it runs
# 28 matchups, so one panel consumes [base, base + 28,000,000). Striding a
# clean 32,000,000 per refresh keeps every panel's seed window disjoint from
# every other panel's across the whole campaign, which is the contract's "no
# reused pair seeds" requirement at campaign scope rather than panel scope.
$script:Cycle4PanelSeedStridePerRefresh = [uint64]32000000

# ---------------------------------------------------------------------------
# Small file/JSON helpers
# ---------------------------------------------------------------------------

function Get-Cycle4Sha256 {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "required file is missing: $Path"
    }
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Get-Cycle4FileRecord {
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

function Read-Cycle4Json {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "required JSON document is missing: $Path"
    }
    $text = [System.IO.File]::ReadAllText($Path, [System.Text.UTF8Encoding]::new($false))
    # Strip a UTF-8 BOM if the producer wrote one; the Rust side reads bytes.
    if ($text.Length -gt 0 -and [int][char]$text[0] -eq 65279) { $text = $text.Substring(1) }
    return $text | ConvertFrom-Json
}

function Write-Cycle4JsonFile {
    # Atomic publication: a staged sibling written, flushed to disk, then moved
    # into place, so a killed wrapper never leaves a half-written record or
    # locator behind for the next process to read as authoritative.
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

function Test-Cycle4ArmUsesBaselineChain {
    # Mirrors Cycle4ArmKindV1::uses_baseline_v4_v1: TREATMENT-RB and STATIC-RB
    # run terminal_reinforce_value/v4-candidate and therefore carry a baseline
    # chain, and their trained own-run checkpoints only load through the
    # baseline-aware loader. CONTROL-R runs the frozen v3 path and has no
    # chain at all.
    param([Parameter(Mandatory = $true)][string]$Arm)
    return (@('treatment-rb', 'static-rb') -contains $Arm)
}

function Assert-Cycle4Arm {
    param([Parameter(Mandatory = $true)][string]$Arm)
    if ($script:Cycle4Arms -notcontains $Arm) {
        throw "unknown arm: $Arm; expected one of $($script:Cycle4Arms -join ', ')"
    }
    return $Arm
}

function Get-Cycle4ChainManifestName {
    param([Parameter(Mandatory = $true)][uint64]$RefreshIndex)
    return ('refresh-{0:d2}.manifest.json' -f $RefreshIndex)
}

function Get-Cycle4ChainPanelName {
    param([Parameter(Mandatory = $true)][uint64]$RefreshIndex)
    return ('refresh-{0:d2}.panel.json' -f $RefreshIndex)
}

# ---------------------------------------------------------------------------
# Provenance records
# ---------------------------------------------------------------------------

function Get-Cycle4GitRecord {
    # A cycle-4-owned git record rather than the precedent's Get-GitRecord: the
    # latter pins an ancestry check against the regularized-continuation base
    # commit, which is a cycle-1 fact and not a cycle-4 launch condition. What
    # carries over unchanged is the substance -- exact HEAD, a clean-worktree
    # requirement, and hashes of the status and diff so a later reviewer can
    # prove the tree that ran.
    param([Parameter(Mandatory = $true)][string]$RepoRoot)
    $safe = $RepoRoot.Replace('\', '/')
    $status = @(& git -c "safe.directory=$safe" -C $RepoRoot status --porcelain 2>&1)
    Assert-LastExitCode $LASTEXITCODE 'git status'
    $head = (@(& git -c "safe.directory=$safe" -C $RepoRoot rev-parse HEAD 2>&1) -join "`n").Trim()
    Assert-LastExitCode $LASTEXITCODE 'git rev-parse'
    if ($status.Count -ne 0) {
        throw "a cycle-4 launch requires a clean worktree at $RepoRoot"
    }
    $diff = @(& git -c "safe.directory=$safe" -C $RepoRoot diff --binary HEAD 2>&1)
    Assert-LastExitCode $LASTEXITCODE 'git diff'
    return [ordered]@{
        repo_root = $RepoRoot
        commit = $head
        dirty = $false
        status_sha256 = Get-TextSha256 (($status -join "`n"))
        worktree_diff_sha256 = Get-TextSha256 (($diff -join "`n"))
    }
}

function New-Cycle4AttemptRoot {
    param(
        [Parameter(Mandatory = $true)][string]$EvidenceRoot,
        [Parameter(Mandatory = $true)][ValidatePattern('^[a-z0-9-]+$')][string]$GateName
    )
    New-Item -ItemType Directory -Force -Path $EvidenceRoot | Out-Null
    return New-UniqueAttemptRoot -EvidenceRoot $EvidenceRoot -GateName $GateName
}

# ---------------------------------------------------------------------------
# Store readers (native training Store V2 on-disk layout:
# native_training_store_layout_v2.rs owns these names)
# ---------------------------------------------------------------------------

function Get-Cycle4StoreLatestGeneration {
    # $null when the Store has no latest.json yet, which is exactly the
    # "genesis has not been authored" state the launcher bootstraps from.
    param([Parameter(Mandatory = $true)][string]$StoreRoot)
    $latestPath = Join-Path $StoreRoot 'latest.json'
    if (-not (Test-Path -LiteralPath $latestPath -PathType Leaf)) { return $null }
    $latest = Read-Cycle4Json -Path $latestPath
    return [uint64]$latest.generation_index
}

function Assert-Cycle4ResumePosition {
    # Launcher-side mirror of the in-library resume check: read the Store's own
    # latest.json and hard-stop BEFORE dispatching a process that would only
    # discover the disagreement after paying for a CUDA context.
    param(
        [Parameter(Mandatory = $true)][string]$StoreRoot,
        [Parameter(Mandatory = $true)][uint64]$ExpectedGeneration
    )
    $actual = Get-Cycle4StoreLatestGeneration -StoreRoot $StoreRoot
    if ($null -eq $actual) {
        if ($ExpectedGeneration -ne [uint64]0) {
            throw "resume assertion FAILED: $StoreRoot has no latest.json, so it can only resume at generation 0, not $ExpectedGeneration"
        }
        return [ordered]@{ store_root = $StoreRoot; generation_index = $null; genesis_pending = $true }
    }
    if ($actual -ne $ExpectedGeneration) {
        throw "resume assertion FAILED: $StoreRoot is at generation $actual, not the expected $ExpectedGeneration"
    }
    return [ordered]@{ store_root = $StoreRoot; generation_index = $actual; genesis_pending = $false }
}

function Get-Cycle4CheckpointIdentity {
    # The five-hash occupant identity of one Store generation, in the exact
    # shape mtg-kernel-cycle4-slot-identities/v1 wants.
    #
    # checkpoint_manifest_sha256 is the SHA-256 of the checkpoint record's own
    # bytes. That is exact, not an approximation: the decoder computes it as
    # sha256(canonical manifest bytes) (native_training_store_checkpoint_v3.rs)
    # and the Store writes exactly those bytes to this file. The other three
    # are the record's own declared bindings, each of which the decoder
    # re-derives from the payload and rejects on mismatch. The genesis
    # boundary cross-checks this whole derivation against the bin's own
    # arm-origin record, which reports the same four values from memory.
    param(
        [Parameter(Mandatory = $true)][string]$StoreRoot,
        [Parameter(Mandatory = $true)][uint64]$StoreGeneration
    )
    $leaf = ('update-{0:d8}.checkpoint.json' -f $StoreGeneration)
    $path = Join-Path (Join-Path $StoreRoot 'checkpoints') $leaf
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Store $StoreRoot has no checkpoint record for generation $StoreGeneration ($path)"
    }
    $record = Read-Cycle4Json -Path $path
    if ([uint64]$record.generation_index -ne $StoreGeneration) {
        throw "checkpoint record at $path declares generation $($record.generation_index), not $StoreGeneration"
    }
    return [ordered]@{
        store_generation = $StoreGeneration
        checkpoint_manifest_sha256 = Get-Cycle4Sha256 -Path $path
        checkpoint_payload_sha256 = [string]$record.payload.sha256
        model_parameter_sha256 = [string]$record.train_state.model_parameter_sha256
        train_state_sha256 = [string]$record.train_state.state_sha256
    }
}

# ---------------------------------------------------------------------------
# Refresh manifest
# ---------------------------------------------------------------------------

function Read-Cycle4Manifest {
    # Structural read only: schema tag, exactly eight slots in index order with
    # the pre-registered roles, and the identity fields the locators key on.
    # The manifest's full semantic contract is the Rust builder's and the arm
    # launcher's job, and is deliberately not re-derived here.
    param([Parameter(Mandatory = $true)][string]$Path)
    $document = Read-Cycle4Json -Path $Path
    if ([string]$document.schema -cne $script:Cycle4ManifestSchema) {
        throw "unexpected manifest schema at $Path`: $($document.schema)"
    }
    $slots = @($document.slots)
    if ($slots.Count -ne $script:Cycle4SlotCount) {
        throw "manifest at $Path must carry exactly $($script:Cycle4SlotCount) slots, found $($slots.Count)"
    }
    $ordered = New-Object object[] $script:Cycle4SlotCount
    foreach ($slot in $slots) {
        $index = [int]$slot.slot_index
        if ($index -lt 0 -or $index -ge $script:Cycle4SlotCount) {
            throw "manifest at $Path has an out-of-range slot_index: $index"
        }
        if ($null -ne $ordered[$index]) {
            throw "manifest at $Path has a duplicate slot_index: $index"
        }
        if ([string]$slot.role -cne $script:Cycle4ExpectedRoles[$index]) {
            throw "manifest at $Path slot $index role is $($slot.role), expected $($script:Cycle4ExpectedRoles[$index])"
        }
        $ordered[$index] = $slot
    }
    foreach ($index in 0..($script:Cycle4SlotCount - 1)) {
        if ($null -eq $ordered[$index]) { throw "manifest at $Path is missing slot $index" }
    }
    return [ordered]@{
        path = (Resolve-Path -LiteralPath $Path).Path
        sha256 = Get-Cycle4Sha256 -Path $Path
        refresh_index = [uint64]$document.refresh_index
        trainee_local_generation = [uint64]$document.trainee_local_generation
        trainee_run_sha256 = [string]$document.trainee_run_sha256
        trainee_base_seed = [uint64]$document.trainee_base_seed
        slots = $ordered
    }
}

# ---------------------------------------------------------------------------
# The machine-local slot table, per refresh boundary
# ---------------------------------------------------------------------------

function Get-Cycle4HistoricalOneRotationIndex {
    # The rotation phase historical-1 occupies at one refresh. Mirrors
    # `usize::try_from(wire.refresh_index % 3)` in
    # validate_slot_assignment_cycle4_v1; restated here only so the wrapper can
    # pick the right Store BEFORE the manifest validator would reject it.
    param([Parameter(Mandatory = $true)][uint64]$RefreshIndex)
    return [int]($RefreshIndex % $script:Cycle4HistoricalRotationPeriod)
}

function Get-Cycle4SlotTableForRefresh {
    # ONE boundary's eight machine-local store roots, in slot order 0..7.
    #
    # Seven of them are the operator's fixed -SlotStoreRoots entries. Slot 3
    # (historical-1) is not fixed: it rotates over three Stores by
    # `refresh_index % 3`, so it comes from the rotation triple instead.
    #
    # An operator who supplies no triple gets the fixed table's slot-3 entry at
    # every phase. That is deliberately NOT silently accepted as correct: the
    # locator writer verifies the chosen root's four content hashes against the
    # manifest's slot-3 identity, so a single-Store table fails closed at the
    # first refresh whose rotation phase names a different Store rather than
    # training an interval against the wrong opponent.
    param(
        [Parameter(Mandatory = $true)][string[]]$SlotStoreRoots,
        [Parameter(Mandatory = $true)][uint64]$RefreshIndex,
        [string[]]$HistoricalOneStoreRoots
    )
    if (@($SlotStoreRoots).Count -ne $script:Cycle4SlotCount) {
        throw "the slot store root table must name exactly $($script:Cycle4SlotCount) store roots in slot order 0..7, got $(@($SlotStoreRoots).Count)"
    }
    $roots = @($SlotStoreRoots)
    if ($null -ne $HistoricalOneStoreRoots -and @($HistoricalOneStoreRoots).Count -ne 0) {
        if (@($HistoricalOneStoreRoots).Count -ne [int]$script:Cycle4HistoricalRotationPeriod) {
            throw "-HistoricalOneStoreRoots must name exactly $($script:Cycle4HistoricalRotationPeriod) store roots in rotation order (refresh_index mod $($script:Cycle4HistoricalRotationPeriod)), got $(@($HistoricalOneStoreRoots).Count)"
        }
        $rotation = Get-Cycle4HistoricalOneRotationIndex -RefreshIndex $RefreshIndex
        $roots[$script:Cycle4HistoricalRotationSlotIndex] = [string]@($HistoricalOneStoreRoots)[$rotation]
    }
    return @(
        foreach ($index in 0..($script:Cycle4SlotCount - 1)) {
            [ordered]@{ slot_index = $index; store_root = [string]$roots[$index] }
        }
    )
}

function Assert-Cycle4FrozenSlotContentHashes {
    # Proves one FOREIGN slot's Store really holds the checkpoint the manifest
    # pins it to, by recomputing all four content hashes at the slot's own
    # pinned generation.
    #
    # This is a genuinely independent check for two slots the manifest
    # validator does not fully pin against a Store:
    #
    #  * slot 3 (historical-1) -- the validator pins the rotation identity by
    #    refresh index, but nothing on the launcher side proves the ROOT the
    #    operator handed it is the Store that identity belongs to; without this
    #    a mis-ordered rotation triple would only surface as a wrong opponent.
    #  * slot 2 (historical-0) before refresh 4 -- the validator pins only the
    #    cycle-3 lineage's run sha, base seed and lagged generation there, NOT
    #    the four content hashes (they are read from that Store's roster entry),
    #    so this is the only place they are proven against the Store.
    #
    # Only ever called for slots whose pinned `source_generation` is a STORE
    # generation. The arm's own slots are excluded by the caller: their
    # manifest generation is trainee-local (store generation plus 896).
    param(
        [Parameter(Mandatory = $true)]$Manifest,
        [Parameter(Mandatory = $true)][int]$SlotIndex,
        [Parameter(Mandatory = $true)][string]$StoreRoot,
        [switch]$AllowMissingStore
    )
    $slot = $Manifest.slots[$SlotIndex]
    $role = $script:Cycle4ExpectedRoles[$SlotIndex]
    $generation = [uint64]$slot.source_generation
    $checkpointPath = Join-Path (Join-Path $StoreRoot 'checkpoints') ('update-{0:d8}.checkpoint.json' -f $generation)
    if ($AllowMissingStore -and -not (Test-Path -LiteralPath $checkpointPath -PathType Leaf)) {
        Write-Host "DRY-RUN slot-$SlotIndex ($role): would verify $checkpointPath against the manifest's four content hashes"
        return $null
    }
    $identity = Get-Cycle4CheckpointIdentity -StoreRoot $StoreRoot -StoreGeneration $generation
    foreach ($field in @('checkpoint_manifest_sha256', 'checkpoint_payload_sha256', 'model_parameter_sha256', 'train_state_sha256')) {
        if ([string]$slot.$field -cne [string]$identity.$field) {
            throw "slot $SlotIndex ($role) at $StoreRoot generation $generation does not match the manifest identity at refresh $($Manifest.refresh_index): $field is $($identity.$field), the manifest declares $([string]$slot.$field)"
        }
    }
    return $identity
}

# ---------------------------------------------------------------------------
# The two locator files, written from ONE machine-local slot table
# ---------------------------------------------------------------------------

function New-Cycle4SlotLocatorPair {
    # The arm launcher keys its locator by occupant IDENTITY (a wrong store
    # cannot occupy a right slot) and the payoff panel runner keys its own by
    # slot INDEX. Both are machine-local, neither ever enters a hashed
    # artifact, and both are written here from a single slot table so the two
    # files can never disagree about which store is in which slot.
    #
    # The table is cross-checked against the manifest roster first: eight
    # entries, indexes 0..7 exactly once, absolute paths, no duplicate store
    # root, and eight DISTINCT roster identities (a repeated identity would
    # make the identity-keyed file ambiguous, and the Rust decoder rejects it).
    #
    # One slot root is never the operator's to supply: whichever slots the
    # manifest binds to the ARM's OWN run are the arm's own Store, at
    # different generations. The manifest itself says which those are
    # (source_run_sha256 == the arm's run), so they are substituted here
    # rather than trusted from the table -- which is also why two slots
    # sharing the arm's Store root is admissible while any other repeated
    # root is a typo.
    param(
        [Parameter(Mandatory = $true)]$SlotTable,
        [Parameter(Mandatory = $true)]$Manifest,
        [Parameter(Mandatory = $true)][string]$ArmLocatorPath,
        [Parameter(Mandatory = $true)][string]$PanelLocatorPath,
        [Parameter(Mandatory = $true)][string]$ArmRunSha256,
        [Parameter(Mandatory = $true)][string]$ArmStoreRoot,
        [string]$ArmBaselineChainDir,
        [string]$GenesisParentStoreRoot,
        [switch]$AllowMissingStores
    )
    if (-not [System.IO.Path]::IsPathRooted($ArmStoreRoot)) {
        throw "the arm store root must be an absolute path: $ArmStoreRoot"
    }
    $baselineChainDir = $null
    if (-not [string]::IsNullOrWhiteSpace($ArmBaselineChainDir)) {
        if (-not [System.IO.Path]::IsPathRooted($ArmBaselineChainDir)) {
            throw "the arm baseline chain directory must be an absolute path: $ArmBaselineChainDir"
        }
        # Normalized rather than resolved: the chain directory legitimately
        # does not exist yet the first time a locator is written.
        $baselineChainDir = [System.IO.Path]::GetFullPath($ArmBaselineChainDir)
    }
    $entries = @($SlotTable)
    if ($entries.Count -ne $script:Cycle4SlotCount) {
        throw "the slot table must carry exactly $($script:Cycle4SlotCount) entries, found $($entries.Count)"
    }
    $byIndex = New-Object object[] $script:Cycle4SlotCount
    foreach ($entry in $entries) {
        $index = [int]$entry.slot_index
        if ($index -lt 0 -or $index -ge $script:Cycle4SlotCount) {
            throw "slot table has an out-of-range slot_index: $index"
        }
        if ($null -ne $byIndex[$index]) { throw "slot table has a duplicate slot_index: $index" }
        $root = [string]$entry.store_root
        if ([string]::IsNullOrWhiteSpace($root) -or -not [System.IO.Path]::IsPathRooted($root)) {
            throw "slot $index store root must be a non-empty absolute path, got '$root'"
        }
        $byIndex[$index] = [ordered]@{ slot_index = $index; store_root = $root }
    }
    foreach ($index in 0..($script:Cycle4SlotCount - 1)) {
        if ($null -eq $byIndex[$index]) { throw "slot table is missing slot $index" }
    }

    $armSlots = @()
    foreach ($index in 0..($script:Cycle4SlotCount - 1)) {
        if ([string]$Manifest.slots[$index].source_run_sha256 -ceq $ArmRunSha256) {
            $byIndex[$index].store_root = $ArmStoreRoot
            $armSlots += $index
        }
    }
    # Two slots may legitimately name the SAME Store at different pinned
    # generations -- anchor-1 is 970002 at 1536 and historical-1's middle
    # rotation phase is 970002 at 1024, one physical Store occupying two slots
    # as two different frozen occupants. What must be distinct is the OCCUPANT
    # IDENTITY, not the path, and the roster identity check below enforces
    # exactly that. So the duplicate rule keys on (root, pinned generation):
    # the same Store at the same generation in two slots is still a typo,
    # because it would be one occupant claiming two slots.
    $foreignRoots = @(
        foreach ($index in 0..($script:Cycle4SlotCount - 1)) {
            if ($armSlots -notcontains $index) { $byIndex[$index].store_root.ToLowerInvariant() }
        }
    )
    $foreignKeys = @(
        foreach ($index in 0..($script:Cycle4SlotCount - 1)) {
            if ($armSlots -notcontains $index) {
                '{0}@{1}' -f $byIndex[$index].store_root.ToLowerInvariant(), [uint64]$Manifest.slots[$index].source_generation
            }
        }
    )
    if (@($foreignKeys | Sort-Object -Unique).Count -ne $foreignKeys.Count) {
        throw 'the slot table maps two slots to the same store root at the same pinned generation'
    }
    if ($foreignRoots -contains $ArmStoreRoot.ToLowerInvariant()) {
        throw "a slot the manifest does not bind to the arm's own run names the arm's Store root: $ArmStoreRoot"
    }
    if (-not $AllowMissingStores) {
        foreach ($index in 0..($script:Cycle4SlotCount - 1)) {
            $root = $byIndex[$index].store_root
            if (-not (Test-Path -LiteralPath $root -PathType Container)) {
                throw "slot $index store root does not exist: $root"
            }
        }
    }

    # The two foreign slots whose ROOT the manifest cannot vouch for: the
    # rotating historical-1, and historical-0 while it is still the cycle-3
    # lineage. Both are checked against the Store itself.
    $verifiedSlots = @($script:Cycle4HistoricalRotationSlotIndex)
    if ($armSlots -notcontains $script:Cycle4HistoricalArmSlotIndex) {
        $verifiedSlots += $script:Cycle4HistoricalArmSlotIndex
    }
    $slotIdentityChecks = [ordered]@{}
    foreach ($index in @($verifiedSlots | Sort-Object)) {
        $checked = Assert-Cycle4FrozenSlotContentHashes `
            -Manifest $Manifest `
            -SlotIndex $index `
            -StoreRoot $byIndex[$index].store_root `
            -AllowMissingStore:$AllowMissingStores
        $slotIdentityChecks["$index"] = $(if ($null -eq $checked) { 'deferred-dry-run' } else { $checked.checkpoint_manifest_sha256 })
    }

    $identities = @($Manifest.slots | ForEach-Object { [string]$_.checkpoint_manifest_sha256 })
    $distinctIdentities = @($identities | Sort-Object -Unique)
    if ($distinctIdentities.Count -ne $script:Cycle4SlotCount) {
        throw 'the manifest roster repeats a checkpoint_manifest_sha256; an identity-keyed locator cannot be built from it'
    }
    foreach ($identity in $identities) {
        if ($identity -notmatch '^[0-9a-f]{64}$') {
            throw "manifest roster identity is not a lowercase SHA-256: $identity"
        }
    }

    $armLocator = [ordered]@{
        schema = $script:Cycle4ArmLocatorSchema
        stores = @(
            foreach ($index in 0..($script:Cycle4SlotCount - 1)) {
                [ordered]@{
                    checkpoint_manifest_sha256 = $identities[$index]
                    store_root = $byIndex[$index].store_root
                }
            }
        )
    }
    if (-not [string]::IsNullOrWhiteSpace($GenesisParentStoreRoot)) {
        if (-not [System.IO.Path]::IsPathRooted($GenesisParentStoreRoot)) {
            throw "genesis parent store root must be absolute: $GenesisParentStoreRoot"
        }
        $armLocator['genesis_parent_store_root'] = $GenesisParentStoreRoot
    }

    # The panel runner's slot entry is a bare store-root string, and stays one
    # for every slot that needs nothing more. A slot bound to the ARM's own run
    # on a v4 arm needs one thing more: its trained checkpoints only load
    # through the baseline-aware loader, which needs the arm's chain directory.
    # Those slots carry an object instead, `store_root` plus the optional
    # `baseline_chain_dir`. The addition is deliberately additive, so a
    # CONTROL-R locator is byte-identical to what this wrote before the field
    # existed and a reader that only understands strings still reads every
    # slot a v3 arm ever produces.
    $panelStores = [ordered]@{}
    foreach ($index in 0..($script:Cycle4SlotCount - 1)) {
        $root = $byIndex[$index].store_root
        if (($armSlots -contains $index) -and $null -ne $baselineChainDir) {
            $panelStores["$index"] = [ordered]@{
                store_root = $root
                baseline_chain_dir = $baselineChainDir
            }
        }
        else {
            $panelStores["$index"] = $root
        }
    }
    $panelLocator = [ordered]@{
        schema = $script:Cycle4PanelLocatorSchema
        stores = $panelStores
    }

    Write-Cycle4JsonFile -Value $armLocator -Path $ArmLocatorPath
    Write-Cycle4JsonFile -Value $panelLocator -Path $PanelLocatorPath
    return [ordered]@{
        arm_locator = Get-Cycle4FileRecord -Path $ArmLocatorPath
        panel_locator = Get-Cycle4FileRecord -Path $PanelLocatorPath
        manifest_sha256 = $Manifest.sha256
        manifest_refresh_index = $Manifest.refresh_index
        arm_owned_slot_indexes = @($armSlots)
        arm_baseline_chain_dir = $baselineChainDir
        verified_slot_identities = $slotIdentityChecks
    }
}

function New-Cycle4BootstrapLocator {
    # The locator a `--bootstrap-genesis` invocation takes. Only its
    # `genesis_parent_store_root` is used by that mode -- there is no manifest
    # yet, so no roster to match identities against -- but the bin still
    # decodes and structurally validates the eight entries, so they are filled
    # from the operator's pinned roster (with the arm's own Store substituted
    # for the own-run slot) rather than invented. Written into the attempt
    # root, never reused as a training locator.
    param(
        [Parameter(Mandatory = $true)]$SlotTable,
        [Parameter(Mandatory = $true)][string]$RosterPath,
        [Parameter(Mandatory = $true)][string]$ArmStoreRoot,
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$GenesisParentStoreRoot
    )
    $roster = Read-Cycle4Json -Path $RosterPath
    if ([string]$roster.schema -cne $script:Cycle4SlotIdentitiesSchema) {
        throw "unexpected slot-identities schema at $RosterPath`: $($roster.schema)"
    }
    $identities = New-Object object[] $script:Cycle4SlotCount
    foreach ($slot in @($roster.slots)) {
        $index = [int]$slot.slot_index
        if ($index -lt 0 -or $index -ge $script:Cycle4SlotCount) {
            throw "slot-identities roster has an out-of-range slot_index: $index"
        }
        $identities[$index] = [string]$slot.checkpoint_manifest_sha256
    }
    $roots = New-Object object[] $script:Cycle4SlotCount
    foreach ($entry in @($SlotTable)) {
        $roots[[int]$entry.slot_index] = [string]$entry.store_root
    }
    $roots[$script:Cycle4ArmOwnedSlotIndex] = $ArmStoreRoot
    $stores = @(
        foreach ($index in 0..($script:Cycle4SlotCount - 1)) {
            if ($null -eq $identities[$index] -or $identities[$index] -notmatch '^[0-9a-f]{64}$') {
                throw "slot-identities roster slot $index carries no lowercase SHA-256 identity"
            }
            [ordered]@{
                checkpoint_manifest_sha256 = $identities[$index]
                store_root = $roots[$index]
            }
        }
    )
    if (-not [System.IO.Path]::IsPathRooted($GenesisParentStoreRoot)) {
        throw "genesis parent store root must be absolute: $GenesisParentStoreRoot"
    }
    Write-Cycle4JsonFile -Value ([ordered]@{
        schema = $script:Cycle4ArmLocatorSchema
        stores = $stores
        genesis_parent_store_root = $GenesisParentStoreRoot
    }) -Path $Path
    return Get-Cycle4FileRecord -Path $Path
}

function Read-Cycle4ArmOriginRecord {
    # The record the bin publishes at `--bootstrap-genesis`: the arm's run
    # identity and base seed, the parent it was seeded from, and the four
    # hashes of the genesis checkpoint the Store actually published. It is the
    # authoritative source for all of those -- nothing else on disk carries the
    # arm's own generation-0 identity before the genesis manifest exists.
    param([Parameter(Mandatory = $true)][string]$ChainDir)
    $path = Join-Path $ChainDir $script:Cycle4ArmOriginRecordFileName
    $record = Read-Cycle4Json -Path $path
    if ([string]$record.schema -cne $script:Cycle4ArmOriginRecordSchema) {
        throw "unexpected arm-origin schema at $path`: $($record.schema)"
    }
    return [ordered]@{
        path = (Resolve-Path -LiteralPath $path).Path
        sha256 = Get-Cycle4Sha256 -Path $path
        arm_kind = [string]$record.arm_kind
        run_sha256 = [string]$record.run_sha256
        base_seed = [uint64]$record.base_seed
        init_generation = [uint64]$record.init_generation
        genesis = [ordered]@{
            store_generation = [uint64]0
            checkpoint_manifest_sha256 = [string]$record.genesis_checkpoint_manifest_sha256
            checkpoint_payload_sha256 = [string]$record.genesis_checkpoint_payload_sha256
            model_parameter_sha256 = [string]$record.genesis_model_parameter_sha256
            train_state_sha256 = [string]$record.genesis_train_state_sha256
        }
    }
}

function Assert-Cycle4GenesisManifestBinding {
    # The genesis manifest is only trustworthy if its own-run slot binds the
    # checkpoint the Store actually published. Three independent derivations
    # of the same four hashes must agree: the manifest the builder just wrote,
    # the bin's own arm-origin record (reported from memory at publication),
    # and this wrapper's own read of the Store's generation-0 checkpoint file.
    # Agreement also proves the wrapper's file-hash derivation is the same one
    # the Rust decoder performs, which is what licenses using it unchecked at
    # every later boundary.
    param(
        [Parameter(Mandatory = $true)]$Manifest,
        [Parameter(Mandatory = $true)]$Origin,
        [Parameter(Mandatory = $true)][string]$ArmStoreRoot
    )
    if ($Manifest.refresh_index -ne [uint64]0) {
        throw "the genesis binding check wants refresh 0, got $($Manifest.refresh_index)"
    }
    if ($Manifest.trainee_run_sha256 -cne $Origin.run_sha256 -or $Manifest.trainee_base_seed -ne $Origin.base_seed) {
        throw 'the genesis manifest binds a different trainee identity than the arm-origin record'
    }
    $slot = $Manifest.slots[$script:Cycle4ArmOwnedSlotIndex]
    $expectedGeneration = $script:Cycle4TraineeStartLocalGeneration
    if ([uint64]$slot.source_generation -ne $expectedGeneration) {
        throw "the genesis manifest own-run slot declares generation $($slot.source_generation), not the trainee-local $expectedGeneration"
    }
    if ([string]$slot.source_run_sha256 -cne $Origin.run_sha256) {
        throw 'the genesis manifest own-run slot is not bound to the arm run'
    }
    $fromStore = Get-Cycle4CheckpointIdentity -StoreRoot $ArmStoreRoot -StoreGeneration ([uint64]0)
    foreach ($field in @('checkpoint_manifest_sha256', 'checkpoint_payload_sha256', 'model_parameter_sha256', 'train_state_sha256')) {
        $manifestValue = [string]$slot.$field
        if ($manifestValue -cne [string]$Origin.genesis.$field) {
            throw "the genesis manifest own-run slot $field ($manifestValue) does not equal the arm-origin record's ($($Origin.genesis.$field))"
        }
        if ($manifestValue -cne [string]$fromStore.$field) {
            throw "the genesis manifest own-run slot $field ($manifestValue) does not equal the Store's own generation-0 checkpoint ($($fromStore.$field))"
        }
    }
    return [ordered]@{
        manifest_sha256 = $Manifest.sha256
        own_run_slot_index = $script:Cycle4ArmOwnedSlotIndex
        trainee_local_generation = $expectedGeneration
        arm_origin_record_sha256 = $Origin.sha256
        genesis_checkpoint_manifest_sha256 = [string]$slot.checkpoint_manifest_sha256
        genesis_checkpoint_payload_sha256 = [string]$slot.checkpoint_payload_sha256
        genesis_model_parameter_sha256 = [string]$slot.model_parameter_sha256
        genesis_train_state_sha256 = [string]$slot.train_state_sha256
    }
}

# ---------------------------------------------------------------------------
# Genesis authority (the cycle-4 sibling of the precedent's denovo/policy
# anchor authority records; a launcher-level companion to the arm bin's own
# arm-origin.record.json, never a substitute for it)
# ---------------------------------------------------------------------------

function Get-Cycle4GenesisAuthorityRecord {
    #
    # It binds the SEEDING facts only. The genesis manifest is no longer an
    # input to the campaign (the wrapper builds it from the bootstrapped
    # Store), so binding its hash here would only record something this same
    # wrapper produced a moment later; Assert-Cycle4GenesisManifestBinding
    # checks that relationship directly instead.
    param(
        [Parameter(Mandatory = $true)][string]$Arm,
        [Parameter(Mandatory = $true)][string]$ParentStoreRoot,
        [Parameter(Mandatory = $true)][uint64]$ParentGeneration,
        [Parameter(Mandatory = $true)][string]$RunRecordPath
    )
    $checkpoints = Join-Path $ParentStoreRoot 'checkpoints'
    $checkpoint = Join-Path $checkpoints ('update-{0:d8}.checkpoint.json' -f $ParentGeneration)
    $sidecar = Join-Path $checkpoints ('update-{0:d8}.sidecar.json' -f $ParentGeneration)
    $state = Join-Path $checkpoints ('update-{0:d8}.state.f32le' -f $ParentGeneration)
    foreach ($path in @($checkpoint, $sidecar, $state)) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "genesis authority: parent artifact is missing: $path"
        }
    }
    $parentRun = Join-Path $ParentStoreRoot 'run.json'
    return [ordered]@{
        schema = $script:Cycle4GenesisAuthoritySchema
        arm_kind = $Arm
        parent_store_root = $ParentStoreRoot
        parent_generation = $ParentGeneration
        parent_run_sha256 = Get-Cycle4Sha256 -Path $parentRun
        parent_checkpoint_sha256 = Get-Cycle4Sha256 -Path $checkpoint
        parent_sidecar_sha256 = Get-Cycle4Sha256 -Path $sidecar
        parent_state_sha256 = Get-Cycle4Sha256 -Path $state
        arm_run_record_sha256 = Get-Cycle4Sha256 -Path $RunRecordPath
    }
}

function Assert-Cycle4GenesisParentBinding {
    # -GenesisParentStoreRoot and -GenesisParentGeneration are machine-local
    # operator inputs, but WHICH parent an arm is seeded from is not: the run
    # record pins it in `contracts.opponent_ladder_initialization`, and the arm
    # bin refuses to author genesis unless the parent Store reproduces that
    # pin exactly. Checking the same equality here means a wrapper pointed at
    # the wrong parent, or at the right parent with the wrong generation,
    # fails at phase=inputs instead of after a Store prefix has been claimed.
    #
    # The five fields compared are the ones a launcher can recompute from
    # plain files. `derived_model_parameter_sha256` is deliberately not among
    # them: deriving it needs the genesis weights-only payload surgery, which
    # is the bin's, and the bin does check it.
    param(
        [Parameter(Mandatory = $true)]$RunRecordDocument,
        [Parameter(Mandatory = $true)]$GenesisAuthority,
        [Parameter(Mandatory = $true)][uint64]$ParentGeneration,
        [Parameter(Mandatory = $true)][string]$RunRecordPath
    )
    # Property existence is tested rather than dereferenced: Set-StrictMode
    # turns a missing property into an opaque PropertyNotFound error, and this
    # is a case an operator has to be able to read.
    if ($RunRecordDocument.PSObject.Properties.Name -notcontains 'contracts') {
        throw "$RunRecordPath declares no contracts section; it is not a cycle-4 arm run record"
    }
    $contracts = $RunRecordDocument.contracts
    if ($null -eq $contracts -or ($contracts.PSObject.Properties.Name -notcontains 'opponent_ladder_initialization')) {
        throw "$RunRecordPath declares no contracts.opponent_ladder_initialization; a cycle-4 arm's genesis parent is pinned there, not on the command line"
    }
    $declared = $contracts.opponent_ladder_initialization
    if ([uint64]$declared.generation -ne $ParentGeneration) {
        throw "-GenesisParentGeneration $ParentGeneration disagrees with the run record's pinned origin generation $($declared.generation) ($RunRecordPath); the correct cycle-4 value is the cycle-3 focal run's store generation $($script:Cycle4TraineeStartLocalGeneration)"
    }
    $pairs = @(
        @('source_run_sha256', 'parent_run_sha256'),
        @('checkpoint_sha256', 'parent_checkpoint_sha256'),
        @('sidecar_sha256', 'parent_sidecar_sha256'),
        @('state_sha256', 'parent_state_sha256')
    )
    foreach ($pair in $pairs) {
        $expected = [string]$declared.($pair[0])
        $actual = [string]$GenesisAuthority.($pair[1])
        if ($expected -cne $actual) {
            throw "the genesis parent store does not reproduce the run record's pinned origin: $($pair[0]) is $expected in $RunRecordPath but the parent store hashes to $actual"
        }
    }
    return [ordered]@{
        parent_generation = $ParentGeneration
        parent_run_sha256 = [string]$declared.source_run_sha256
        parent_checkpoint_sha256 = [string]$declared.checkpoint_sha256
        parent_sidecar_sha256 = [string]$declared.sidecar_sha256
        parent_state_sha256 = [string]$declared.state_sha256
        derived_model_parameter_sha256 = [string]$declared.derived_model_parameter_sha256
    }
}

function Assert-OrCreateCycle4GenesisAuthority {
    # Written once, then re-verified field by field on every later launch: a
    # parent checkpoint, run record, or genesis manifest that changed under a
    # running campaign stops the campaign here rather than surfacing later as
    # an uninterpretable training anomaly.
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)]$Record
    )
    if (Test-Path -LiteralPath $Path -PathType Leaf) {
        $actual = Read-Cycle4Json -Path $Path
        foreach ($field in $Record.Keys) {
            if ([string]$actual.$field -cne [string]$Record[$field]) {
                throw "genesis authority mismatch for '$field' at $Path`: recorded '$($actual.$field)', current '$($Record[$field])'"
            }
        }
    }
    else {
        Write-Cycle4JsonFile -Value $Record -Path $Path
    }
    return Get-Cycle4FileRecord -Path $Path
}

# ---------------------------------------------------------------------------
# Slot identities for the next boundary
# ---------------------------------------------------------------------------

function New-Cycle4SlotIdentitiesFile {
    # The builder bin's --slot-identities input for ONE boundary. The frozen
    # occupants (and, before refresh 4, the cycle-3 historical-0) come from the
    # operator's pinned roster, because their identities are compiled Rust
    # constants this wrapper has no independent claim on. The slots the ARM
    # itself occupies are DERIVED here from the arm's own Store head, which is
    # the only place their identity exists -- this is what the builder module
    # means by "as produced by the wrapper from the Store heads".
    #
    # Refresh 0 is derived like every other boundary now: `--bootstrap-genesis`
    # publishes the arm's Store before any manifest exists, so slot 5's
    # genesis identity is read from that Store's generation-0 checkpoint.
    param(
        [Parameter(Mandatory = $true)][string]$RosterPath,
        [Parameter(Mandatory = $true)][string]$OutputPath,
        [Parameter(Mandatory = $true)][uint64]$RefreshIndex,
        [Parameter(Mandatory = $true)][string]$ArmStoreRoot,
        [Parameter(Mandatory = $true)][string]$ArmRunSha256,
        [Parameter(Mandatory = $true)][uint64]$ArmBaseSeed
    )
    $roster = Read-Cycle4Json -Path $RosterPath
    if ([string]$roster.schema -cne $script:Cycle4SlotIdentitiesSchema) {
        throw "unexpected slot-identities schema at $RosterPath`: $($roster.schema)"
    }
    $slots = @($roster.slots)
    if ($slots.Count -ne $script:Cycle4SlotCount) {
        throw "slot-identities roster at $RosterPath must carry exactly $($script:Cycle4SlotCount) slots"
    }
    $ordered = New-Object object[] $script:Cycle4SlotCount
    foreach ($slot in $slots) {
        $index = [int]$slot.slot_index
        if ($index -lt 0 -or $index -ge $script:Cycle4SlotCount) {
            throw "slot-identities roster has an out-of-range slot_index: $index"
        }
        if ($null -ne $ordered[$index]) {
            throw "slot-identities roster has a duplicate slot_index: $index"
        }
        $ordered[$index] = [ordered]@{
            slot_index = [uint64]$index
            source_base_seed = [uint64]$slot.source_base_seed
            source_run_sha256 = [string]$slot.source_run_sha256
            source_generation = [uint64]$slot.source_generation
            checkpoint_manifest_sha256 = [string]$slot.checkpoint_manifest_sha256
            checkpoint_payload_sha256 = [string]$slot.checkpoint_payload_sha256
            model_parameter_sha256 = [string]$slot.model_parameter_sha256
            train_state_sha256 = [string]$slot.train_state_sha256
        }
    }
    foreach ($index in 0..($script:Cycle4SlotCount - 1)) {
        if ($null -eq $ordered[$index]) { throw "slot-identities roster is missing slot $index" }
    }

    $traineeLocal = $script:Cycle4TraineeStartLocalGeneration + ($RefreshIndex * $script:Cycle4RefreshInterval)
    $derived = @()
    $derived += [ordered]@{
        slot_index = $script:Cycle4ArmOwnedSlotIndex
        trainee_local_generation = $traineeLocal
    }
    if ($RefreshIndex -ge $script:Cycle4HistoricalArmFirstRefreshIndex) {
        $derived += [ordered]@{
            slot_index = $script:Cycle4HistoricalArmSlotIndex
            trainee_local_generation = ($traineeLocal - $script:Cycle4HistoricalLag)
        }
    }
    foreach ($target in $derived) {
        $storeGeneration = $target.trainee_local_generation - $script:Cycle4TraineeStartLocalGeneration
        $identity = Get-Cycle4CheckpointIdentity -StoreRoot $ArmStoreRoot -StoreGeneration $storeGeneration
        $ordered[$target.slot_index] = [ordered]@{
            slot_index = [uint64]$target.slot_index
            source_base_seed = $ArmBaseSeed
            source_run_sha256 = $ArmRunSha256
            # The manifest validator pins slots 2 and 5 to TRAINEE-LOCAL
            # generations, so that is what is written here, while the hashes
            # above were read at the corresponding STORE generation
            # (trainee-local minus 896). See the README's known-issue note.
            source_generation = [uint64]$target.trainee_local_generation
            checkpoint_manifest_sha256 = $identity.checkpoint_manifest_sha256
            checkpoint_payload_sha256 = $identity.checkpoint_payload_sha256
            model_parameter_sha256 = $identity.model_parameter_sha256
            train_state_sha256 = $identity.train_state_sha256
        }
    }

    $document = [ordered]@{
        schema = $script:Cycle4SlotIdentitiesSchema
        slots = @($ordered)
    }
    Write-Cycle4JsonFile -Value $document -Path $OutputPath
    return [ordered]@{
        record = Get-Cycle4FileRecord -Path $OutputPath
        derived_slot_indexes = @($derived | ForEach-Object { $_.slot_index })
    }
}

# ---------------------------------------------------------------------------
# Interval phase journal
#
# An attempt can be interrupted anywhere, and the Store alone cannot say where:
# a latest.json at a refresh boundary means either "this interval finished and
# its panel and next manifest are done" or "training finished and neither is",
# and a latest.json inside an interval means training is mid-flight at a stop
# generation only the launching attempt knew. So each interval keeps a small
# hash-chained journal, one file per interval under the attempt root, written
# atomically at every transition. Resume reads the journals of every attempt
# under the gate root (newest wins per interval, dry runs ignored), verifies
# the chain, and reconstructs the pending work from it plus the Store and the
# refresh chain.
# ---------------------------------------------------------------------------

function Get-Cycle4IntervalPhaseFileName {
    param([Parameter(Mandatory = $true)][uint64]$IntervalIndex)
    return ('interval-{0:d2}.phase.json' -f $IntervalIndex)
}

function Get-Cycle4PhaseRecordSha256 {
    # Deterministic framing over every field of one record plus its parent
    # hash. LF-joined and never reordered, so two readers of the same journal
    # compute the same digest.
    param(
        [Parameter(Mandatory = $true)]$Record,
        [Parameter(Mandatory = $true)][uint64]$IntervalIndex,
        [Parameter(Mandatory = $true)][uint64]$RefreshIndex,
        [Parameter(Mandatory = $true)][uint64]$StopGeneration
    )
    $frame = @(
        [string]$Record.phase
        [string]$Record.utc
        [string]$Record.attempt_root
        [string]$IntervalIndex
        [string]$RefreshIndex
        [string]$StopGeneration
        [string]$Record.parent_sha256
    ) -join "`n"
    return Get-TextSha256 $frame
}

function Assert-Cycle4IntervalJournal {
    # Recomputes every record's digest and every link, including the link from
    # the previous interval's terminal record. A truncated, reordered or edited
    # journal fails here rather than producing a plausible-looking plan.
    param(
        [Parameter(Mandatory = $true)]$Journal,
        [string]$PreviousIntervalTipSha256
    )
    $records = @($Journal.records)
    if ($records.Count -eq 0) {
        throw "interval $($Journal.interval_index) journal carries no records"
    }
    $expectedParent = $script:Cycle4PhaseChainGenesisParent
    if (-not [string]::IsNullOrWhiteSpace($PreviousIntervalTipSha256)) {
        $expectedParent = $PreviousIntervalTipSha256
    }
    $seen = 0
    foreach ($record in $records) {
        if ([string]$record.parent_sha256 -cne $expectedParent) {
            throw "interval $($Journal.interval_index) journal record '$($record.phase)' is not chained to its predecessor"
        }
        $computed = Get-Cycle4PhaseRecordSha256 `
            -Record $record `
            -IntervalIndex ([uint64]$Journal.interval_index) `
            -RefreshIndex ([uint64]$Journal.refresh_index) `
            -StopGeneration ([uint64]$Journal.stop_generation)
        if ($computed -cne [string]$record.record_sha256) {
            throw "interval $($Journal.interval_index) journal record '$($record.phase)' does not match its own digest"
        }
        $position = [array]::IndexOf($script:Cycle4IntervalPhases, [string]$record.phase)
        if ($position -lt 0) {
            throw "interval $($Journal.interval_index) journal carries an unknown phase: $($record.phase)"
        }
        if ($position -ne $seen) {
            throw "interval $($Journal.interval_index) journal reaches '$($record.phase)' out of order"
        }
        $seen++
        $expectedParent = [string]$record.record_sha256
    }
    return $expectedParent
}

function Read-Cycle4IntervalJournals {
    # Every attempt under the gate root, oldest to newest; the newest copy of
    # each interval's journal wins, because an attempt that touched an interval
    # carries that interval's whole history forward. Dry-run attempts are
    # skipped outright: they plan work rather than performing it, and their
    # records must never be mistaken for progress.
    param([Parameter(Mandatory = $true)][string]$GateRoot)
    $journals = @{}
    if (-not (Test-Path -LiteralPath $GateRoot -PathType Container)) { return $journals }
    foreach ($attempt in @(Get-ChildItem -LiteralPath $GateRoot -Directory | Sort-Object Name)) {
        $launch = Join-Path $attempt.FullName 'launch-manifest.json'
        if (Test-Path -LiteralPath $launch -PathType Leaf) {
            $manifest = Read-Cycle4Json -Path $launch
            # Property-existence checked rather than dereferenced: a launch
            # manifest that predates a field must not crash a resume.
            if (($manifest.PSObject.Properties.Name -contains 'dry_run') -and [bool]$manifest.dry_run) {
                continue
            }
        }
        foreach ($file in @(Get-ChildItem -LiteralPath $attempt.FullName -Filter 'interval-*.phase.json' -File)) {
            $journal = Read-Cycle4Json -Path $file.FullName
            if ([string]$journal.schema -cne $script:Cycle4IntervalPhaseSchema) {
                throw "unexpected interval-phase schema at $($file.FullName): $($journal.schema)"
            }
            $journals[[uint64]$journal.interval_index] = $journal
        }
    }
    # Verify the whole chain in interval order once every file is in hand.
    $tip = $null
    foreach ($index in @($journals.Keys | Sort-Object)) {
        $tip = Assert-Cycle4IntervalJournal -Journal $journals[$index] -PreviousIntervalTipSha256 $tip
    }
    return $journals
}

function Get-Cycle4IntervalPhase {
    # The last phase recorded for one interval, or $null when the interval has
    # no journal at all (a campaign that predates the journal, or an interval
    # that has never been started).
    param(
        [Parameter(Mandatory = $true)]$Journals,
        [Parameter(Mandatory = $true)][uint64]$IntervalIndex
    )
    if (-not $Journals.ContainsKey($IntervalIndex)) { return $null }
    $records = @($Journals[$IntervalIndex].records)
    return [string]$records[$records.Count - 1].phase
}

function Add-Cycle4IntervalPhase {
    # Appends one transition and rewrites that interval's journal atomically
    # into the CURRENT attempt root, carrying every earlier record forward so
    # the newest copy is always the complete history. Returns the updated
    # journal.
    param(
        [Parameter(Mandatory = $true)]$Journals,
        [Parameter(Mandatory = $true)][string]$AttemptRoot,
        [Parameter(Mandatory = $true)][string]$Arm,
        [Parameter(Mandatory = $true)][uint64]$IntervalIndex,
        [Parameter(Mandatory = $true)][uint64]$RefreshIndex,
        [Parameter(Mandatory = $true)][uint64]$StopGeneration,
        [Parameter(Mandatory = $true)][ValidateSet('training-started', 'training-complete', 'panel-complete', 'manifest-complete')][string]$Phase
    )
    if ($Journals.ContainsKey($IntervalIndex)) {
        $journal = $Journals[$IntervalIndex]
        if ([uint64]$journal.stop_generation -ne $StopGeneration -or [uint64]$journal.refresh_index -ne $RefreshIndex) {
            throw "interval $IntervalIndex was journalled with stop generation $($journal.stop_generation) at refresh $($journal.refresh_index), but this attempt is using $StopGeneration at refresh $RefreshIndex"
        }
        $records = @($journal.records)
        $parent = [string]$records[$records.Count - 1].record_sha256
    }
    else {
        $records = @()
        $parent = $script:Cycle4PhaseChainGenesisParent
        if ($IntervalIndex -gt [uint64]0 -and $Journals.ContainsKey($IntervalIndex - [uint64]1)) {
            $previous = @($Journals[$IntervalIndex - [uint64]1].records)
            $parent = [string]$previous[$previous.Count - 1].record_sha256
        }
    }
    $record = [ordered]@{
        phase = $Phase
        utc = [DateTimeOffset]::UtcNow.ToString('O')
        attempt_root = $AttemptRoot
        parent_sha256 = $parent
    }
    $record['record_sha256'] = Get-Cycle4PhaseRecordSha256 `
        -Record $record `
        -IntervalIndex $IntervalIndex `
        -RefreshIndex $RefreshIndex `
        -StopGeneration $StopGeneration
    $updated = [ordered]@{
        schema = $script:Cycle4IntervalPhaseSchema
        arm = $Arm
        interval_index = $IntervalIndex
        refresh_index = $RefreshIndex
        stop_generation = $StopGeneration
        records = @($records + $record)
    }
    Write-Cycle4JsonFile -Value $updated -Path (Join-Path $AttemptRoot (Get-Cycle4IntervalPhaseFileName -IntervalIndex $IntervalIndex))
    $Journals[$IntervalIndex] = $updated
    return $updated
}

# ---------------------------------------------------------------------------
# Child processes
# ---------------------------------------------------------------------------

function Format-Cycle4CommandLine {
    # The exact command line, quoted the way Start-Process receives it, so a
    # dry run prints something an operator can paste and a reviewer can diff.
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )
    $parts = @('"' + $FilePath + '"')
    foreach ($argument in $Arguments) {
        $parts += '"' + ([string]$argument).Replace('"', '\"') + '"'
    }
    return ($parts -join ' ')
}

function Invoke-Cycle4Process {
    # One child process, its exit code captured authoritatively.
    #
    # Round F defect 2. The first CONTROL preflight ladder attempt recorded
    # exit_code 0 for both rungs while the arm bin had in fact taken its
    # contract-rejection path (empty stdout, the refusal on stderr, and
    # `exit_code_v1()` maps Contract to 3). The cause is a Windows-specific
    # Start-Process property, not the bin: under PowerShell 5.1 the
    # Process object -PassThru returns may hold NO cached native handle, and
    # once the child has exited and Windows has reaped it there is nothing
    # left to read a code from, so `.ExitCode` answers $null forever no
    # matter how many times WaitForExit() and Refresh() are called. The old
    # body then wrote `[int]$exitCode`, and `[int]$null` is 0 in PowerShell:
    # a silent, total inversion of the fail-closed contract, turning every
    # child refusal into a recorded success.
    #
    # Two changes, both required:
    #
    #   1. `.Handle` is read IMMEDIATELY after the start, before the child
    #      can exit. Touching that property makes System.Diagnostics.Process
    #      duplicate and cache the native handle for the lifetime of the
    #      object, which is what keeps the exit code readable after the
    #      child is gone. This is the fix the S1 launcher family uses.
    #   2. A $null ExitCode is a HARD FAILURE, never a cast. If the handle
    #      trick somehow still leaves the code unreadable, the launcher must
    #      say so and stop, not invent a zero.
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$WorkingDirectory,
        [Parameter(Mandatory = $true)][string]$StdoutPath,
        [Parameter(Mandatory = $true)][string]$StderrPath,
        [Parameter(Mandatory = $true)][string]$Label,
        [hashtable]$Environment,
        [switch]$DryRun
    )
    $commandLine = Format-Cycle4CommandLine -FilePath $FilePath -Arguments $Arguments
    if ($DryRun) {
        Write-Host "DRY-RUN $Label`: $commandLine"
        if ($null -ne $Environment) {
            foreach ($name in @($Environment.Keys | Sort-Object)) {
                Write-Host "DRY-RUN $Label env: $name=$($Environment[$name])"
            }
        }
        return [ordered]@{
            label = $Label
            command_line = $commandLine
            dry_run = $true
            exit_code = 0
        }
    }
    if (-not (Test-Path -LiteralPath $FilePath -PathType Leaf)) {
        throw "$Label executable is missing: $FilePath"
    }
    foreach ($path in @($StdoutPath, $StderrPath)) {
        $directory = Split-Path -Parent $path
        if (-not [string]::IsNullOrWhiteSpace($directory)) {
            New-Item -ItemType Directory -Force -Path $directory | Out-Null
        }
    }
    $saved = @{}
    if ($null -ne $Environment) {
        foreach ($name in $Environment.Keys) {
            $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
            [Environment]::SetEnvironmentVariable($name, [string]$Environment[$name], 'Process')
        }
    }
    # PowerShell 5.1's -ArgumentList does not quote array elements on its own,
    # so a path with a space would silently split into two arguments. Pass the
    # already-quoted text instead (the precedent's own Start-NativeLane shape).
    $argumentText = (@($Arguments | ForEach-Object { '"' + ([string]$_).Replace('"', '\"') + '"' }) -join ' ')
    $started = [DateTimeOffset]::UtcNow
    try {
        $process = Start-Process -FilePath $FilePath -ArgumentList $argumentText `
            -WorkingDirectory $WorkingDirectory -WindowStyle Hidden -PassThru `
            -RedirectStandardOutput $StdoutPath -RedirectStandardError $StderrPath
        # Cache the native handle NOW, while the child is certainly still
        # alive. See this function's header: without this the exit code of a
        # short-lived child is unrecoverable. Best-effort on purpose -- if
        # the handle cannot be taken this is not itself the failure; the
        # unreadable exit code below is, and it is checked unconditionally.
        try { $null = $process.Handle } catch { }
        try { $processId = [int]$process.Id } catch { $processId = -1 }
        $process.WaitForExit()
        $process.Refresh()
        $exitCode = $process.ExitCode
    }
    finally {
        foreach ($name in $saved.Keys) {
            [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process')
        }
    }
    if ($null -eq $exitCode) {
        # Never [int]$null (which is 0). An unreadable exit code is an
        # unknown outcome, and an unknown outcome is a failure.
        throw "$Label exit code could not be read from the child process (pid $processId); refusing to record an outcome. stdout=$StdoutPath stderr=$StderrPath"
    }
    $finished = [DateTimeOffset]::UtcNow
    return [ordered]@{
        label = $Label
        command_line = $commandLine
        dry_run = $false
        exit_code = [int]$exitCode
        process_id = $processId
        started_utc = $started.ToString('O')
        completed_utc = $finished.ToString('O')
        wall_seconds = ($finished - $started).TotalSeconds
        stdout = $StdoutPath
        stderr = $StderrPath
    }
}

function Assert-Cycle4ProcessSucceeded {
    param([Parameter(Mandatory = $true)]$Result)
    if ($null -eq $Result.exit_code) {
        # The second line of defence for round F defect 2. Invoke-Cycle4Process
        # already refuses an unreadable exit code at the source; this makes the
        # same refusal true of any result document that reaches this gate,
        # however it was produced.
        throw "$($Result.label) exited with an unreadable exit code; an unknown outcome is never a success"
    }
    if ($Result.exit_code -ne 0) {
        $detail = "$($Result.label) exited $($Result.exit_code)"
        if (-not $Result.dry_run) { $detail = "$detail; see $($Result.stderr)" }
        throw $detail
    }
    return $Result
}

# ---------------------------------------------------------------------------
# Terminal markers (the g896 family's shape: gate-specific empty markers plus
# one plain-text RUN_FAILED naming the failing step)
# ---------------------------------------------------------------------------

function Write-Cycle4Marker {
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][ValidateSet('PREFLIGHT_COMPLETE', 'TRAINING_COMPLETE')][string]$Name
    )
    $path = Join-Path $Root $Name
    if (-not (Test-Path -LiteralPath $path)) {
        New-Item -ItemType File -Path $path | Out-Null
    }
    return $path
}

function Write-Cycle4RunFailed {
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][string]$Phase,
        [Parameter(Mandatory = $true)][string]$Message
    )
    $path = Join-Path $Root 'RUN_FAILED'
    $line = "$([DateTimeOffset]::UtcNow.ToString('O')) phase=$Phase error=$Message"
    [System.IO.File]::WriteAllText($path, $line, [System.Text.UTF8Encoding]::new($false))
    return $path
}
