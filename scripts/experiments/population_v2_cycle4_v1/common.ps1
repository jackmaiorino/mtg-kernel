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
# Slot 5 (current-1) from refresh 1; slot 2 (historical-0) from refresh 4,
# before which historical-0 is still the cycle-3 lineage.
$script:Cycle4ArmOwnedSlotIndex = 5
$script:Cycle4HistoricalArmSlotIndex = 2
$script:Cycle4HistoricalArmFirstRefreshIndex = [uint64]4

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
    # bytes (the Store publishes canonical bytes, and the Rust decoder derives
    # this same value from the bytes it read); the other three are the record's
    # own declared bindings.
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
    param(
        [Parameter(Mandatory = $true)]$SlotTable,
        [Parameter(Mandatory = $true)]$Manifest,
        [Parameter(Mandatory = $true)][string]$ArmLocatorPath,
        [Parameter(Mandatory = $true)][string]$PanelLocatorPath,
        [string]$GenesisParentStoreRoot,
        [switch]$AllowMissingStores
    )
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
        if (-not $AllowMissingStores -and -not (Test-Path -LiteralPath $root -PathType Container)) {
            throw "slot $index store root does not exist: $root"
        }
        $byIndex[$index] = [ordered]@{ slot_index = $index; store_root = $root }
    }
    foreach ($index in 0..($script:Cycle4SlotCount - 1)) {
        if ($null -eq $byIndex[$index]) { throw "slot table is missing slot $index" }
    }
    $distinctRoots = @($byIndex | ForEach-Object { $_.store_root.ToLowerInvariant() } | Sort-Object -Unique)
    if ($distinctRoots.Count -ne $script:Cycle4SlotCount) {
        throw 'the slot table maps two slots to the same store root'
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

    $panelStores = [ordered]@{}
    foreach ($index in 0..($script:Cycle4SlotCount - 1)) {
        $panelStores["$index"] = $byIndex[$index].store_root
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
    }
}

# ---------------------------------------------------------------------------
# Genesis authority (the cycle-4 sibling of the precedent's denovo/policy
# anchor authority records; a launcher-level companion to the arm bin's own
# arm-origin.record.json, never a substitute for it)
# ---------------------------------------------------------------------------

function Get-Cycle4GenesisAuthorityRecord {
    param(
        [Parameter(Mandatory = $true)][string]$Arm,
        [Parameter(Mandatory = $true)][string]$ParentStoreRoot,
        [Parameter(Mandatory = $true)][uint64]$ParentGeneration,
        [Parameter(Mandatory = $true)][string]$RunRecordPath,
        [Parameter(Mandatory = $true)][string]$GenesisManifestPath
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
        genesis_refresh_manifest_sha256 = Get-Cycle4Sha256 -Path $GenesisManifestPath
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
    # Refresh 0 is the one boundary where nothing is derived: the arm's Store
    # does not exist until the genesis manifest already does, so slot 5's
    # genesis identity is necessarily an operator input (see the README).
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
    if ($RefreshIndex -ge [uint64]1) {
        $derived += [ordered]@{
            slot_index = $script:Cycle4ArmOwnedSlotIndex
            trainee_local_generation = $traineeLocal
        }
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
    # One child process, its exit code captured with the WaitForExit() then
    # Refresh() double call.
    #
    # Start-Process can leave ExitCode unset when the child exits before the
    # caller observes HasExited; the g896 formal CONTROL run published a
    # RUN_FAILED marker over exactly that unset property while every artifact
    # verified. A parameterless WaitForExit() refreshes the native handle and a
    # following Refresh() materializes the real code, so the exit code read
    # below is always the process's own.
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
        $process.WaitForExit()
        $process.Refresh()
        $exitCode = $process.ExitCode
    }
    finally {
        foreach ($name in $saved.Keys) {
            [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process')
        }
    }
    $finished = [DateTimeOffset]::UtcNow
    return [ordered]@{
        label = $Label
        command_line = $commandLine
        dry_run = $false
        exit_code = [int]$exitCode
        process_id = [int]$process.Id
        started_utc = $started.ToString('O')
        completed_utc = $finished.ToString('O')
        wall_seconds = ($finished - $started).TotalSeconds
        stdout = $StdoutPath
        stderr = $StderrPath
    }
}

function Assert-Cycle4ProcessSucceeded {
    param([Parameter(Mandatory = $true)]$Result)
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
