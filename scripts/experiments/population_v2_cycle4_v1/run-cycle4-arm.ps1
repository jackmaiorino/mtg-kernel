<#
.SYNOPSIS
Cycle-4 arm launcher wrapper (docs/native_cycle4_arm_launcher_v1.md Section 6).

.DESCRIPTION
Drives one cycle-4 arm end to end, or the CONTROL preflight ladder that must
pass before any arm launches. Nothing here trains: every unit of work is a
child process (the arm bin, the payoff panel runner, the refresh builder bin)
whose exit code this wrapper captures and whose inputs it proves first.

The arm's run record is DERIVED, not supplied: cycle4_run_record_v1 builds it
from the arm kind, the pinned parent Store, -ArmExecutable's own build
provenance, and the compiled cycle-4 literals,
and refuses to replace a DIFFERENT record already at -RunRecord. Running it on
every launch therefore both produces the record the first time and re-proves an
existing one on every later attempt. -UseExistingRunRecord is the explicit
override that takes -RunRecord exactly as given and derives nothing.

historical-1 (slot 3) rotates over three Stores by refresh_index mod 3, so the
locator table is rebuilt per boundary from -HistoricalOneStoreRoots and the
chosen root's four content hashes are proven against that boundary's manifest.

Formal mode:

  0a. derive (or re-prove) the run record with cycle4_run_record_v1
  0. if the Store holds no genesis, run cycle4_arm_v1 --bootstrap-genesis,
     which seeds it from the pinned parent and trains nothing
  0b. if refresh-00.manifest.json does not exist, build it with
     cycle4_refresh_build_v1 --genesis from the operator's pinned roster with
     the own-run slot filled from the freshly bootstrapped Store, then assert
     it binds the Store's actual genesis checkpoint

then per interval, through refresh 16:

  1. assert the Store's own position is inside this interval's window
  2. run cycle4_arm_v1 with --stop-generation = interval end
  3. assert the Store advanced to exactly that stop generation
  4. run the payoff panel over the interval's manifest roster
  5. build the next refresh manifest from the chain plus that panel

Each of those transitions is journalled, hash-chained, under the attempt root
before the next begins, so an interrupted attempt can be resumed exactly: a
Store stopped inside an interval resumes that interval's ORIGINAL stop
generation, and an interval whose training finished but whose panel or next
manifest did not gets those finished before anything advances.
TRAINING_COMPLETE is published only once the whole chain through the last
refresh exists and verifies.

STATIC-RB runs steps 1-4 and never step 5: it reuses the genesis manifest at
every interval, and the wrapper asserts before and after every interval that
no manifest past refresh 0 has appeared in the refresh chain directory.

Preflight mode is the CONTROL ladder: two independent throwaway Store prefixes
under the attempt root, each bootstrapped from the same parent and run record
and each given its OWN genesis manifest built from its own Store, then each
advanced by the same short window and compared byte for byte -- every relative
file's size and SHA-256, the whole store tree hash, the endpoint's own
identity fields, and the two genesis manifests themselves, which must be
byte-identical because the two genesis checkpoints must be. It uses the arm
bin's bounded --preflight/--preflight-updates provision, which the bin refuses
to apply to any Store prefix a formal run has trained.

-DryRun validates every input, writes the provenance records and both locator
files, prints the exact command line of every child it would run, and launches
nothing. -SkipHostAssertions additionally skips the git, toolchain, and GPU
assertions and is accepted ONLY together with -DryRun, so a real launch can
never quietly skip them.

Terminal state, following the g896 formal wrapper family: an empty
PREFLIGHT_COMPLETE or TRAINING_COMPLETE marker in the attempt root on success,
and a plain-text RUN_FAILED naming the failing step on any error. A DRY RUN
publishes neither marker: it writes result.json with status DRY_RUN_PLANNED,
because a run that trained and compared nothing may not leave behind the file
an operator reads as "this arm finished".
#>
[CmdletBinding(DefaultParameterSetName = 'Inline')]
param(
    # Round F defect 4: every parameter below can instead be given in one
    # JSON file named by -ParameterFile. `powershell -NoProfile -File` cannot
    # pass an array at all -- it hands the script flat strings, so
    # `-SlotStoreRoots @('a','b')` arrives as the two tokens `@('a',` and
    # `'b')` and the eight-root check refuses a command line that reads
    # correctly in a README. A parameter file is the paste-able launch form;
    # splatting a hashtable from inside a PowerShell session is the other,
    # and both are documented in README.md.
    [Parameter(Mandatory = $true, ParameterSetName = 'ParameterFile')][string]$ParameterFile,
    [Parameter(Mandatory = $true, ParameterSetName = 'Inline')][ValidateSet('formal', 'preflight')][string]$Mode,
    [Parameter(Mandatory = $true, ParameterSetName = 'Inline')][ValidateSet('control-r', 'static-rb', 'treatment-rb')][string]$Arm,
    [Parameter(Mandatory = $true, ParameterSetName = 'Inline')][string]$EvidenceRoot,
    [Parameter(Mandatory = $true, ParameterSetName = 'Inline')][string]$RunRecord,
    [Parameter(Mandatory = $true, ParameterSetName = 'Inline')][string]$RefreshChainDir,
    [Parameter(Mandatory = $true, ParameterSetName = 'Inline')][string]$SlotIdentitiesRosterDir,
    [Parameter(Mandatory = $true, ParameterSetName = 'Inline')][string[]]$SlotStoreRoots,
    # historical-1 (slot 3) rotates over three Stores by refresh_index mod 3.
    # Supply them in rotation order; the entry -SlotStoreRoots carries at index
    # 3 is then only a fallback, and the locator writer proves the chosen root
    # against the manifest's slot-3 identity at every refresh either way.
    [string[]]$HistoricalOneStoreRoots,
    [Parameter(Mandatory = $true, ParameterSetName = 'Inline')][string]$GenesisParentStoreRoot,
    [Parameter(Mandatory = $true, ParameterSetName = 'Inline')][uint64]$GenesisParentGeneration,
    [Parameter(Mandatory = $true, ParameterSetName = 'Inline')][string]$ArmExecutable,
    # The run-record builder. Required unless -UseExistingRunRecord says the
    # operator is supplying a pre-existing record instead.
    [string]$RunRecordExecutable,
    # Take -RunRecord as given and never generate or re-derive it. The explicit
    # override for a record built elsewhere; without it every launch re-derives
    # the record from the pinned parent and fails closed if what is on disk
    # differs.
    [switch]$UseExistingRunRecord,
    [Parameter(Mandatory = $true, ParameterSetName = 'Inline')][string]$RefreshBuilderExecutable,
    [Parameter(Mandatory = $true, ParameterSetName = 'Inline')][string]$PanelExecutable,
    [Parameter(Mandatory = $true, ParameterSetName = 'Inline')][string]$PythonExecutable,
    [Parameter(Mandatory = $true, ParameterSetName = 'Inline')][uint64]$PanelBaseSeed,
    # Formal mode only: the arm's own Store root. Its PARENT directory is the
    # Store prefix the arm bin's mode marker claims.
    [string]$StoreRoot,
    # Formal mode only: the arm's baseline chain directory.
    [string]$ChainDir,
    [string]$RepoRoot,
    [ValidateSet(0, 1)][int]$Device = 1,
    [uint64]$ThroughRefreshIndex = 16,
    # Preflight only. 0 means "derive the smallest admissible window": the arm
    # bin requires a whole number of checkpoint segments, so a two-update
    # ladder on a four-update segment must run four.
    [uint64]$PreflightUpdates = 0,
    [switch]$DryRun,
    [switch]$SkipHostAssertions
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot 'common.ps1')

# The closed list of parameter names a parameter file may set. Derived from
# this script's own parameter block, so it can never drift from it, minus
# -ParameterFile itself and PowerShell's common parameters.
$script:Cycle4WrapperParameterNames = @(
    $MyInvocation.MyCommand.Parameters.Keys |
        Where-Object { $_ -cne 'ParameterFile' -and $_ -notin [System.Management.Automation.Cmdlet]::CommonParameters }
)

if ($PSCmdlet.ParameterSetName -ceq 'ParameterFile') {
    $loadedParameters = Read-Cycle4ParameterFile `
        -Path $ParameterFile `
        -KnownNames $script:Cycle4WrapperParameterNames
    foreach ($name in @($loadedParameters.Keys)) {
        $value = $loadedParameters[$name]
        # PowerShell's own parameter type constraints still apply: the
        # variables were declared in param() above, so an ill-typed value
        # (a string where a uint64 belongs, a scalar where a string[]
        # belongs) is refused here exactly as it would be on the command
        # line.
        if ($MyInvocation.MyCommand.Parameters[$name].ParameterType -eq [switch]) {
            Set-Variable -Name $name -Value ([switch]([bool]$value))
        }
        else {
            Set-Variable -Name $name -Value $value
        }
    }
    Write-Host "inputs: -ParameterFile $ParameterFile supplied $($loadedParameters.Count) parameters"
    # The Inline set's mandatory parameters are not mandatory in this set, so
    # their presence is proven here instead, by name, all at once.
    $missing = @(
        foreach ($name in @('Mode', 'Arm', 'EvidenceRoot', 'RunRecord', 'RefreshChainDir',
                'SlotIdentitiesRosterDir', 'SlotStoreRoots', 'GenesisParentStoreRoot',
                'GenesisParentGeneration', 'ArmExecutable', 'RefreshBuilderExecutable',
                'PanelExecutable', 'PythonExecutable', 'PanelBaseSeed')) {
            if (-not $loadedParameters.Contains($name)) { $name }
        }
    )
    if ($missing.Count -gt 0) {
        throw "$ParameterFile does not name every required wrapper parameter; missing: $($missing -join ', ')"
    }
}

if ($SkipHostAssertions -and -not $DryRun) {
    throw '-SkipHostAssertions is only accepted together with -DryRun; a real launch never skips the git, toolchain, and GPU assertions'
}
if ([string]::IsNullOrWhiteSpace($RepoRoot)) {
    $RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
}
if ($ThroughRefreshIndex -lt [uint64]1 -or $ThroughRefreshIndex -gt $script:Cycle4MaxRefreshIndex) {
    throw "-ThroughRefreshIndex must be 1..$($script:Cycle4MaxRefreshIndex), got $ThroughRefreshIndex"
}
if ($Mode -ceq 'formal') {
    foreach ($pair in @(@('StoreRoot', $StoreRoot), @('ChainDir', $ChainDir))) {
        if ([string]::IsNullOrWhiteSpace($pair[1])) {
            throw "formal mode requires -$($pair[0])"
        }
    }
    if (-not [System.IO.Path]::IsPathRooted($StoreRoot)) {
        throw "-StoreRoot must be an absolute path: $StoreRoot"
    }
}
else {
    if ($Arm -cne 'control-r') {
        throw "the preflight ladder is the CONTROL ladder; -Arm must be control-r, got $Arm"
    }
    if ($PreflightUpdates -gt $script:Cycle4PreflightMaxUpdates) {
        throw "-PreflightUpdates must be 0 (derive) or 1..$($script:Cycle4PreflightMaxUpdates), got $PreflightUpdates"
    }
}
if (@($SlotStoreRoots).Count -ne $script:Cycle4SlotCount) {
    throw "-SlotStoreRoots must name exactly $($script:Cycle4SlotCount) store roots in slot order 0..7, got $(@($SlotStoreRoots).Count)"
}
$historicalOneRootCount = @($HistoricalOneStoreRoots).Count
$requiresHistoricalOneRotation = ($Mode -ceq 'formal' -and $Arm -cne 'static-rb' -and $ThroughRefreshIndex -ge [uint64]1)
if ($requiresHistoricalOneRotation -and $historicalOneRootCount -ne [int]$script:Cycle4HistoricalRotationPeriod) {
    throw "-HistoricalOneStoreRoots is required for formal $Arm through refresh $ThroughRefreshIndex and must name exactly $($script:Cycle4HistoricalRotationPeriod) store roots in rotation order (refresh_index mod $($script:Cycle4HistoricalRotationPeriod)); got $historicalOneRootCount"
}
if (-not $requiresHistoricalOneRotation -and $historicalOneRootCount -ne 0 -and $historicalOneRootCount -ne [int]$script:Cycle4HistoricalRotationPeriod) {
    throw "-HistoricalOneStoreRoots must name exactly $($script:Cycle4HistoricalRotationPeriod) store roots in rotation order (refresh_index mod $($script:Cycle4HistoricalRotationPeriod)), got $historicalOneRootCount"
}
if ($UseExistingRunRecord -and -not [string]::IsNullOrWhiteSpace($RunRecordExecutable)) {
    throw '-UseExistingRunRecord and -RunRecordExecutable are mutually exclusive: either the wrapper derives the run record or the operator supplies it, never both'
}
if (-not $UseExistingRunRecord -and [string]::IsNullOrWhiteSpace($RunRecordExecutable)) {
    throw 'a cycle-4 launch requires -RunRecordExecutable (cycle4_run_record_v1.exe), or -UseExistingRunRecord to take -RunRecord exactly as given'
}

$gateName = "cycle4-$Arm-$Mode"
$gateRoot = Join-Path $EvidenceRoot $gateName
$root = New-Cycle4AttemptRoot -EvidenceRoot $EvidenceRoot -GateName $gateName
$phase = 'inputs'
$commandLog = Join-Path $root 'commands.jsonl'

function Add-Cycle4CommandRecord {
    param([Parameter(Mandatory = $true)]$Result)
    $line = ($Result | ConvertTo-Json -Depth 6 -Compress) + "`n"
    [System.IO.File]::AppendAllText($commandLog, $line, [System.Text.UTF8Encoding]::new($false))
    return $Result
}

try {
    # -----------------------------------------------------------------------
    # Inputs and provenance
    # -----------------------------------------------------------------------
    Assert-Cycle4Arm -Arm $Arm | Out-Null

    # The genesis refresh manifest is NOT an operator input: it binds the arm's
    # own generation-0 checkpoint, which only exists once the Store does, so
    # the wrapper bootstraps the Store first and builds the manifest from it.
    # What the operator does supply is the pinned roster for every boundary,
    # genesis included, whose own-run slot this wrapper always overwrites.
    $genesisManifestPath = Join-Path $RefreshChainDir (Get-Cycle4ChainManifestName -RefreshIndex ([uint64]0))
    $genesisRosterPath = Join-Path $SlotIdentitiesRosterDir 'refresh-00.slot-identities.json'
    if (-not (Test-Path -LiteralPath $genesisRosterPath -PathType Leaf)) {
        throw "the genesis slot-identities roster is missing: $genesisRosterPath"
    }

    # Formal output parents are launcher inputs, not child-process side effects.
    # Ensure the complete campaign layout before the first child process so a
    # fresh campaign cannot bootstrap a Store and then fail because a later
    # output parent is absent. The launch manifest records the exact set.
    $createdDirectories = @()
    if ($Mode -ceq 'formal') {
        # An advanced Store cannot legitimately lack its refresh chain: refresh 0
        # is built right after genesis and every later manifest binds a panel.
        # Creating the chain here would let the genesis path rebuild refresh 0
        # from the existing Store and re-run formal panels, so a Store past
        # generation 0 with no chain (or no genesis manifest) fails closed
        # BEFORE anything is created; only a fresh campaign gets its layout.
        $advancedGeneration = Get-Cycle4StoreLatestGeneration -StoreRoot $StoreRoot
        if (($null -ne $advancedGeneration) -and ([uint64]$advancedGeneration -gt [uint64]0)) {
            if (-not (Test-Path -LiteralPath $RefreshChainDir -PathType Container)) {
                throw "$StoreRoot is at generation $advancedGeneration but its refresh chain directory is missing: $RefreshChainDir; refusing to create an empty chain for an advanced Store"
            }
            if (-not (Test-Path -LiteralPath $genesisManifestPath -PathType Leaf)) {
                throw "$StoreRoot is at generation $advancedGeneration but the genesis refresh manifest is missing: $genesisManifestPath; an advanced Store cannot lack it"
            }
        }
        $storePrefix = Split-Path -Parent $StoreRoot
        $runRecordDirectory = Split-Path -Parent $RunRecord
        $formalOutputDirectories = @(
            $EvidenceRoot,
            $gateRoot,
            $root,
            $runRecordDirectory,
            $storePrefix,
            $StoreRoot,
            $ChainDir,
            $RefreshChainDir
        )
        for ($intervalIndex = [uint64]0; $intervalIndex -lt $ThroughRefreshIndex; $intervalIndex++) {
            $intervalDirectory = Join-Path $root ('interval-{0:d2}' -f $intervalIndex)
            $formalOutputDirectories += $intervalDirectory
            $formalOutputDirectories += (Join-Path $intervalDirectory 'panel')
        }
        foreach ($directoryPath in $formalOutputDirectories) {
            if ([string]::IsNullOrWhiteSpace($directoryPath)) { continue }
            $directory = New-Item -ItemType Directory -Force -Path $directoryPath
            if (-not (Test-Path -LiteralPath $directory.FullName -PathType Container)) {
                throw "failed creating formal output directory: $directoryPath"
            }
            if ($createdDirectories -cnotcontains $directory.FullName) {
                $createdDirectories += $directory.FullName
            }
        }
    }

    # -------------------------------------------------------------------
    # The run record is DERIVED, not supplied. cycle4_run_record_v1 builds
    # it from the arm kind, the pinned parent Store, and the compiled
    # cycle-4 literals, and refuses to replace a different record already at
    # -RunRecord, so running it on every launch both produces the record the
    # first time and re-proves an existing one on every later attempt. The
    # only way past that is -UseExistingRunRecord, which is the explicit
    # operator override for a record built elsewhere.
    # -------------------------------------------------------------------
    $runRecordDirectory = Split-Path -Parent $RunRecord
    if (-not [string]::IsNullOrWhiteSpace($runRecordDirectory)) {
        New-Item -ItemType Directory -Force -Path $runRecordDirectory | Out-Null
    }
    if ($UseExistingRunRecord) {
        if (-not (Test-Path -LiteralPath $RunRecord -PathType Leaf)) {
            throw "-UseExistingRunRecord was given but $RunRecord does not exist"
        }
        Write-Host "inputs: -UseExistingRunRecord; taking $RunRecord as given and deriving nothing"
    }
    else {
        $result = Invoke-Cycle4Process `
            -FilePath $RunRecordExecutable `
            -Arguments @(
                '--arm', $Arm,
                '--parent-store-root', $GenesisParentStoreRoot,
                '--parent-generation', [string]$GenesisParentGeneration,
                # The record declares the provenance of the launcher that
                # will publish the Store, so the builder is handed that exact
                # executable rather than inheriting the parent record's.
                '--arm-executable', $ArmExecutable,
                '--output', $RunRecord
            ) `
            -WorkingDirectory $RepoRoot `
            -StdoutPath (Join-Path $root 'run-record.stdout.log') `
            -StderrPath (Join-Path $root 'run-record.stderr.log') `
            -Label 'run-record' `
            -DryRun:$DryRun
        Add-Cycle4CommandRecord -Result $result | Out-Null
        Assert-Cycle4ProcessSucceeded -Result $result | Out-Null
    }

    if (-not (Test-Path -LiteralPath $RunRecord -PathType Leaf)) {
        if (-not $DryRun) {
            throw "the run record was not produced: $RunRecord"
        }
        # A dry run over a campaign whose run record does not exist yet can
        # plan the derivation above and nothing further: every later step
        # reads the record's own schedule.
        Write-Host "DRY-RUN inputs: the plan needs the run record the command above would have produced; stopping here"
        Write-Cycle4JsonFile -Value ([ordered]@{
            schema = 'mtg-kernel-cycle4-arm-training-result/v1'
            status = 'DRY_RUN_PLANNED'
            completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
            arm = $Arm
            mode = $Mode
            dry_run = $true
            dry_run_stopped_after = 'run-record'
            run_record = $RunRecord
            command_log = $commandLog
            nonclaim = 'A dry run plans work; it never performs it, and it never publishes a completion marker.'
        }) -Path (Join-Path $root 'result.json')
        Write-Host "CYCLE4 DRY RUN PLANNED arm=$Arm stopped_after=run-record evidence=$root"
        return
    }

    $runRecordDocument = Read-Cycle4Json -Path $RunRecord
    $checkpointSegmentUpdates = [uint64]$runRecordDocument.schedule.checkpoint_segment_updates
    if ($checkpointSegmentUpdates -eq [uint64]0) {
        throw "$RunRecord declares checkpoint_segment_updates = 0"
    }

    $inputRecords = [ordered]@{
        run_record = Get-Cycle4FileRecord -Path $RunRecord
        genesis_slot_identities_roster = Get-Cycle4FileRecord -Path $genesisRosterPath
        arm_executable = Get-Cycle4FileRecord -Path $ArmExecutable
        run_record_executable = $(if ($UseExistingRunRecord) { $null } else { Get-Cycle4FileRecord -Path $RunRecordExecutable })
        refresh_builder_executable = Get-Cycle4FileRecord -Path $RefreshBuilderExecutable
        panel_executable = Get-Cycle4FileRecord -Path $PanelExecutable
        python_executable = Get-Cycle4FileRecord -Path $PythonExecutable
        panel_runner = Get-Cycle4FileRecord -Path (Join-Path $PSScriptRoot 'run_payoff_panel_v1.py')
        wrapper = Get-Cycle4FileRecord -Path (Join-Path $PSScriptRoot 'run-cycle4-arm.ps1')
        wrapper_common = Get-Cycle4FileRecord -Path (Join-Path $PSScriptRoot 'common.ps1')
    }

    $phase = 'host-assertions'
    $gitRecord = $null
    $toolchainRecord = $null
    $gpuRecord = $null
    $panelBuildIdentity = $null
    if (-not $SkipHostAssertions) {
        $gitRecord = Get-Cycle4GitRecord -RepoRoot $RepoRoot
        $toolchainRecord = Get-ToolchainRecord
        $gpuRecord = Assert-GpuIdentity -Ordinal $Device
        if ($Device -eq 1) {
            Assert-Gpu1Idle | Out-Null
            Assert-NoForeignGpu1ComputeProcesses
        }
        # Round F defect 6. -PanelExecutable is a cargo TEST binary
        # (`deps\mtg_kernel-<hash>.exe`) whose name says nothing about which
        # commit produced it, and the inputs record only ever hashed it. The
        # first preflight attempt was pointed at one that predated the launch
        # commit. A FORMAL interval publishes campaign evidence, so its panel
        # binary's build identity must match the commit this launch is on --
        # proven from the binary's own embedded identity where it has one and
        # from the build step's receipt beside it otherwise (see README.md,
        # "Build the three bins and the panel test executable"). A preflight
        # still only hashes it: the ladder proves determinism between two
        # rungs of the same binary and publishes no panel.
        if ($Mode -ceq 'formal' -and -not $DryRun) {
            # The launch's own source-tree digest, from the arm executable
            # this launch will publish the Store with. Round F review finding
            # (P1): the panel binary is bound to these exact source bytes,
            # not merely to a commit, because a dirty-tree build at the same
            # commit is a different compiler input.
            $armBuildIdentity = Get-Cycle4ArmBuildIdentity `
                -ArmExecutable $ArmExecutable `
                -StdoutPath (Join-Path $root 'arm-build-identity.stdout.log') `
                -StderrPath (Join-Path $root 'arm-build-identity.stderr.log') `
                -WorkingDirectory $RepoRoot
            Add-Cycle4CommandRecord -Result $armBuildIdentity.command | Out-Null
            if ($armBuildIdentity.source_git_commit -cne $gitRecord.commit) {
                throw "the arm executable was built from commit $($armBuildIdentity.source_git_commit), but this launch is on commit $($gitRecord.commit)"
            }
            $panelBuildIdentity = Assert-Cycle4PanelBuildIdentity `
                -PanelExecutable $PanelExecutable `
                -LaunchCommit $gitRecord.commit `
                -LaunchSourceTreeSha256 $armBuildIdentity.source_tree_sha256
            Write-Host "inputs: panel executable build identity proven from its $($panelBuildIdentity.source) at commit $($panelBuildIdentity.source_git_commit), source tree $($panelBuildIdentity.source_tree_sha256)"
        }
    }

    $phase = 'records'
    $launchManifest = [ordered]@{
        schema = 'mtg-kernel-cycle4-arm-launch-manifest/v1'
        mode = $Mode
        arm = $Arm
        started_utc = [DateTimeOffset]::UtcNow.ToString('O')
        dry_run = [bool]$DryRun
        host_assertions_skipped = [bool]$SkipHostAssertions
        attempt_root = $root
        repo_root = $RepoRoot
        device = $Device
        through_refresh_index = $ThroughRefreshIndex
        refresh_interval = $script:Cycle4RefreshInterval
        panel_games_per_matchup = $script:Cycle4PanelGamesPerMatchup
        checkpoint_segment_updates = $checkpointSegmentUpdates
        panel_base_seed = $PanelBaseSeed
        slot_store_roots = @($SlotStoreRoots)
        historical_one_store_roots = @($HistoricalOneStoreRoots)
        run_record_derived = (-not [bool]$UseExistingRunRecord)
        genesis_parent_store_root = $GenesisParentStoreRoot
        genesis_parent_generation = $GenesisParentGeneration
        refresh_chain_dir = $RefreshChainDir
        slot_identities_roster_dir = $SlotIdentitiesRosterDir
        store_root = $StoreRoot
        chain_dir = $ChainDir
        created_directories = @($createdDirectories)
        inputs = $inputRecords
        git = $gitRecord
        toolchain = $toolchainRecord
        gpu = $gpuRecord
        panel_build_identity = $panelBuildIdentity
        parameter_file = $(if ($PSCmdlet.ParameterSetName -ceq 'ParameterFile') { Get-Cycle4FileRecord -Path $ParameterFile } else { $null })
    }
    Write-Cycle4JsonFile -Value $launchManifest -Path (Join-Path $root 'launch-manifest.json')

    $genesisAuthority = Get-Cycle4GenesisAuthorityRecord `
        -Arm $Arm `
        -ParentStoreRoot $GenesisParentStoreRoot `
        -ParentGeneration $GenesisParentGeneration `
        -RunRecordPath $RunRecord
    # Formal mode publishes it into the arm's baseline chain directory, beside
    # the bin's own arm-origin.record.json: campaign-scoped rather than
    # attempt-scoped, so every later attempt re-verifies the same genesis facts
    # instead of re-asserting a fresh copy of them. A preflight has no formal
    # chain directory, so its copy stays inside the throwaway attempt root.
    # Named per arm because the record declares arm_kind.
    if ($Mode -ceq 'formal') { $genesisAuthorityHome = $ChainDir } else { $genesisAuthorityHome = $root }
    $genesisAuthorityPath = Join-Path $genesisAuthorityHome "cycle4-genesis-authority-$Arm.json"
    $genesisAuthorityRecord = Assert-OrCreateCycle4GenesisAuthority -Path $genesisAuthorityPath -Record $genesisAuthority
    Write-Cycle4JsonFile -Value $genesisAuthorityRecord -Path (Join-Path $root 'genesis-authority-binding.json')

    # WHICH parent this arm is seeded from is the run record's claim, not the
    # command line's. Cross-checking them here means a -GenesisParentGeneration
    # that names the wrong generation (the README's old 2048 rather than the
    # cycle-3 focal run's 896) stops the launch before a Store prefix is
    # claimed, instead of binding the wrong parent.
    $genesisParentBinding = Assert-Cycle4GenesisParentBinding `
        -RunRecordDocument $runRecordDocument `
        -GenesisAuthority $genesisAuthority `
        -ParentGeneration $GenesisParentGeneration `
        -RunRecordPath $RunRecord
    Write-Cycle4JsonFile -Value $genesisParentBinding -Path (Join-Path $root 'genesis-parent-binding.json')

    # The genesis boundary's table. Every later boundary rebuilds it for its
    # own refresh index, because slot 3 rotates.
    $slotTable = Get-Cycle4SlotTableForRefresh `
        -SlotStoreRoots $SlotStoreRoots `
        -HistoricalOneStoreRoots $HistoricalOneStoreRoots `
        -RefreshIndex ([uint64]0)

    # -------------------------------------------------------------------
    # Inputs-phase decode check, BEFORE any bootstrap (round F defect 3)
    # -------------------------------------------------------------------
    # The first CONTROL preflight ladder attempt spent two full five-minute
    # genesis bootstraps before either rung reached opponent-slot resolution
    # and refused there, on a roster record that could not decode. Nothing
    # about that refusal needed a Store to exist: the record was on disk,
    # readable, and undecodable from the first second of the attempt.
    #
    # `cycle4_arm_v1 --check-slot-locator` is a read-only, device-free mode
    # that decodes every slot Store's run.json and the genesis parent's
    # through the same `decode_train_run_v2` entry point the slot resolver
    # uses, and exits 0 or 3. Running it here turns that ten-minute discovery
    # into a one-second one.
    #
    # Round F review finding (P1): ONE refresh-0 slot table is not the
    # campaign's input set. historical-1 (slot 3) rotates over three Stores
    # by `refresh_index mod 3`, so a refresh-0-only check proves rotation
    # root 0 and leaves roots 1 and 2 -- which refreshes 1 and 2 will train
    # against -- entirely unproven until the GPU is already busy. Slot 3 is
    # the only slot that varies with the refresh index, so checking one
    # representative refresh per rotation phase reached through
    # -ThroughRefreshIndex covers every distinct root the campaign needs.
    # The coverage is then asserted against the union computed directly,
    # rather than assumed.
    $phase = 'inputs-slot-decode'
    $consumedRefreshIndices = @([uint64]0)
    if ($Mode -ceq 'formal' -and $Arm -cne 'static-rb') {
        $consumedRefreshIndices = @()
        for ($refresh = [uint64]0; $refresh -le $ThroughRefreshIndex; $refresh++) {
            $consumedRefreshIndices += $refresh
        }
    }
    $requiredRoots = [ordered]@{}
    foreach ($refresh in $consumedRefreshIndices) {
        foreach ($entry in @(Get-Cycle4SlotTableForRefresh `
                    -SlotStoreRoots $SlotStoreRoots `
                    -HistoricalOneStoreRoots $HistoricalOneStoreRoots `
                    -RefreshIndex $refresh)) {
            $requiredRoots[[string]$entry.store_root] = $true
        }
    }
    # An array of pairs, not a hashtable: [ordered] indexes an INTEGER key
    # positionally rather than by key, and the rotation phase is an integer.
    $representatives = @()
    $seenRotations = @()
    foreach ($refresh in $consumedRefreshIndices) {
        $rotation = Get-Cycle4HistoricalOneRotationIndex -RefreshIndex $refresh
        if ($seenRotations -notcontains $rotation) {
            $seenRotations += $rotation
            $representatives += [ordered]@{ rotation_index = $rotation; refresh_index = $refresh }
        }
    }
    $coveredRoots = [ordered]@{}
    $inputsCheckBindings = @()
    foreach ($representative in $representatives) {
        $rotation = [int]$representative.rotation_index
        $refresh = [uint64]$representative.refresh_index
        $checkTable = Get-Cycle4SlotTableForRefresh `
            -SlotStoreRoots $SlotStoreRoots `
            -HistoricalOneStoreRoots $HistoricalOneStoreRoots `
            -RefreshIndex $refresh
        $checkLocatorPath = Join-Path $root ('inputs-check-slot-locator-rotation-{0}.json' -f $rotation)
        $inputsCheckLocator = New-Cycle4InputsCheckLocator `
            -SlotTable $checkTable `
            -RosterPath $genesisRosterPath `
            -Path $checkLocatorPath `
            -GenesisParentStoreRoot $GenesisParentStoreRoot `
            -ArmStoreRoot $StoreRoot
        $inputsCheckLocator['rotation_index'] = $rotation
        $inputsCheckLocator['representative_refresh_index'] = $refresh
        $inputsCheckBindings += $inputsCheckLocator
        foreach ($entry in @($checkTable)) { $coveredRoots[[string]$entry.store_root] = $true }
        $coveredRoots[[string]$inputsCheckLocator.own_run_store_root] = $true
        $result = Invoke-Cycle4Process `
            -FilePath $ArmExecutable `
            -Arguments @('--check-slot-locator', $checkLocatorPath) `
            -WorkingDirectory $RepoRoot `
            -StdoutPath (Join-Path $root ('inputs-slot-decode-rotation-{0}.stdout.log' -f $rotation)) `
            -StderrPath (Join-Path $root ('inputs-slot-decode-rotation-{0}.stderr.log' -f $rotation)) `
            -Label ('inputs-slot-decode-rotation-{0}' -f $rotation) `
            -DryRun:$DryRun
        Add-Cycle4CommandRecord -Result $result | Out-Null
        Assert-Cycle4ProcessSucceeded -Result $result | Out-Null
    }
    # The own-run slot's table entry is a placeholder the wrapper always
    # overrides (README, "Eight slot store roots"), so it is not part of the
    # input set this check must prove.
    $ownRunPlaceholder = [string]@($SlotStoreRoots)[$script:Cycle4ArmOwnedSlotIndex]
    $uncovered = @($requiredRoots.Keys | Where-Object {
            -not $coveredRoots.Contains($_) -and $_ -cne $ownRunPlaceholder
        })
    if ($uncovered.Count -gt 0) {
        throw "the inputs decode check did not cover every store root this campaign needs through refresh $ThroughRefreshIndex; uncovered: $($uncovered -join ', ')"
    }
    Write-Cycle4JsonFile -Value ([ordered]@{
        schema = 'mtg-kernel-cycle4-inputs-check-binding/v1'
        through_refresh_index = $ThroughRefreshIndex
        rotation_phases_checked = @($seenRotations)
        required_store_roots = @($requiredRoots.Keys)
        covered_store_roots = @($coveredRoots.Keys)
        own_run_slot_placeholder = $ownRunPlaceholder
        locators = @($inputsCheckBindings)
    }) -Path (Join-Path $root 'inputs-check-binding.json')

    # ---------------------------------------------------------------------
    # Genesis: seed the Store, then build the manifest that binds it.
    # ---------------------------------------------------------------------
    function Invoke-Cycle4Bootstrap {
        # Runs `--bootstrap-genesis` unless the Store already holds a genesis.
        # Idempotent by construction: the bin itself is exit 3 on a seeded
        # Store, so the wrapper only ever calls it when latest.json is absent.
        param(
            [Parameter(Mandatory = $true)][string]$Prefix,
            [Parameter(Mandatory = $true)][string]$TargetStoreRoot,
            [Parameter(Mandatory = $true)][string]$TargetChainDir,
            [Parameter(Mandatory = $true)][string]$Label
        )
        if ($null -ne (Get-Cycle4StoreLatestGeneration -StoreRoot $TargetStoreRoot)) {
            return $null
        }
        $locator = Join-Path $Prefix 'bootstrap-slot-locator.json'
        New-Cycle4BootstrapLocator `
            -SlotTable $slotTable `
            -RosterPath $genesisRosterPath `
            -ArmStoreRoot $TargetStoreRoot `
            -Path $locator `
            -GenesisParentStoreRoot $GenesisParentStoreRoot | Out-Null
        $result = Invoke-Cycle4Process `
            -FilePath $ArmExecutable `
            -Arguments @(
                '--arm', $Arm,
                '--store-root', $TargetStoreRoot,
                '--run-record', $RunRecord,
                '--chain-dir', $TargetChainDir,
                '--slot-locator', $locator,
                '--device', [string]$Device,
                '--bootstrap-genesis'
            ) `
            -WorkingDirectory $RepoRoot `
            -StdoutPath (Join-Path $Prefix 'bootstrap.stdout.log') `
            -StderrPath (Join-Path $Prefix 'bootstrap.stderr.log') `
            -Label $Label `
            -DryRun:$DryRun
        Add-Cycle4CommandRecord -Result $result | Out-Null
        Assert-Cycle4ProcessSucceeded -Result $result | Out-Null
        if (-not $DryRun) {
            Assert-Cycle4ResumePosition -StoreRoot $TargetStoreRoot -ExpectedGeneration ([uint64]0) | Out-Null
        }
        return $result
    }

    function Invoke-Cycle4GenesisManifestBuild {
        # Builds refresh-00.manifest.json with the builder bin's --genesis mode
        # from the pinned roster plus the arm's own genesis checkpoint, then
        # proves the result binds that checkpoint. Returns $null in a dry run
        # that has nothing to read.
        param(
            [Parameter(Mandatory = $true)][string]$Prefix,
            [Parameter(Mandatory = $true)][string]$TargetStoreRoot,
            [Parameter(Mandatory = $true)][string]$TargetChainDir,
            [Parameter(Mandatory = $true)][string]$OutputManifest,
            [Parameter(Mandatory = $true)][string]$Label
        )
        $staged = Join-Path $Prefix 'refresh-00.slot-identities.json'
        $runSha256 = '<from-arm-origin.record.json-after-bootstrap>'
        $baseSeed = [uint64]$runRecordDocument.schedule.base_seed
        $origin = $null
        if (Test-Path -LiteralPath (Join-Path $TargetChainDir $script:Cycle4ArmOriginRecordFileName) -PathType Leaf) {
            $origin = Read-Cycle4ArmOriginRecord -ChainDir $TargetChainDir
            if ($origin.base_seed -ne $baseSeed) {
                throw "the arm-origin record's base seed ($($origin.base_seed)) does not equal the run record's ($baseSeed)"
            }
            $runSha256 = $origin.run_sha256
            New-Cycle4SlotIdentitiesFile `
                -RosterPath $genesisRosterPath `
                -OutputPath $staged `
                -RefreshIndex ([uint64]0) `
                -ArmStoreRoot $TargetStoreRoot `
                -ArmRunSha256 $runSha256 `
                -ArmBaseSeed $baseSeed | Out-Null
        }
        elseif ($DryRun) {
            Write-Host "DRY-RUN $Label`: would stage $genesisRosterPath to $staged with the own-run slot read from $TargetStoreRoot generation 0, and take the arm run identity from the arm-origin record the bootstrap above publishes"
        }
        else {
            throw "the bootstrap published no arm-origin record in $TargetChainDir"
        }
        $outputDirectory = Split-Path -Parent $OutputManifest
        if ([string]::IsNullOrWhiteSpace($outputDirectory) -or
            -not (Test-Path -LiteralPath $outputDirectory -PathType Container)) {
            throw "the genesis manifest output parent does not exist before the builder command: $outputDirectory"
        }
        $result = Invoke-Cycle4Process `
            -FilePath $RefreshBuilderExecutable `
            -Arguments @(
                '--genesis',
                '--trainee-run-sha256', $runSha256,
                '--trainee-base-seed', [string]$baseSeed,
                '--slot-identities', $staged,
                '--output', $OutputManifest
            ) `
            -WorkingDirectory $RepoRoot `
            -StdoutPath (Join-Path $Prefix 'genesis-build.stdout.log') `
            -StderrPath (Join-Path $Prefix 'genesis-build.stderr.log') `
            -Label $Label `
            -DryRun:$DryRun
        Add-Cycle4CommandRecord -Result $result | Out-Null
        Assert-Cycle4ProcessSucceeded -Result $result | Out-Null
        if ($DryRun -and -not (Test-Path -LiteralPath $OutputManifest -PathType Leaf)) {
            return $null
        }
        $manifest = Read-Cycle4Manifest -Path $OutputManifest
        if ($null -eq $origin) { throw "cannot verify $OutputManifest without an arm-origin record" }
        return Assert-Cycle4GenesisManifestBinding -Manifest $manifest -Origin $origin -ArmStoreRoot $TargetStoreRoot
    }

    if ($Mode -ceq 'preflight') {
        # -------------------------------------------------------------------
        # CONTROL preflight ladder
        # -------------------------------------------------------------------
        $phase = 'preflight-plan'
        $window = $PreflightUpdates
        if ($window -eq [uint64]0) {
            # Smallest whole number of checkpoint segments that is at least the
            # two short updates the ladder calls for.
            $window = $checkpointSegmentUpdates
            while ($window -lt [uint64]2) { $window = $window + $checkpointSegmentUpdates }
        }
        if ($window -lt [uint64]1 -or $window -gt $script:Cycle4PreflightMaxUpdates) {
            throw "no admissible preflight window: checkpoint_segment_updates=$checkpointSegmentUpdates needs a window of $window, but the arm bin bounds --preflight-updates to 1..$($script:Cycle4PreflightMaxUpdates)"
        }
        if (($window % $checkpointSegmentUpdates) -ne [uint64]0) {
            throw "-PreflightUpdates $window is not a whole number of checkpoint segments ($checkpointSegmentUpdates)"
        }

        $ladderRoot = Join-Path $root 'ladder'
        $rungs = @()
        foreach ($name in @('a', 'b')) {
            $prefix = Join-Path $ladderRoot $name
            $rungs += [ordered]@{
                name = $name
                prefix = $prefix
                store_root = (Join-Path $prefix 'store')
                chain_dir = (Join-Path $prefix 'chain')
                manifest = (Join-Path $prefix 'refresh-00.manifest.json')
                locator = (Join-Path $prefix 'arm-slot-locator.json')
                panel_locator = (Join-Path $prefix 'panel-slot-locator.json')
            }
        }

        # Each rung is a whole independent campaign genesis: its own Store, its
        # own chain directory, its own genesis manifest built from its own
        # Store. Nothing is shared, so neither rung can read the other's
        # artifacts as an opponent -- and the two manifests must still come out
        # byte-identical, which is itself part of what the ladder proves.
        $phase = 'preflight-bootstrap'
        $bindings = @()
        foreach ($rung in $rungs) {
            New-Item -ItemType Directory -Force -Path $rung.prefix | Out-Null
            Invoke-Cycle4Bootstrap `
                -Prefix $rung.prefix `
                -TargetStoreRoot $rung.store_root `
                -TargetChainDir $rung.chain_dir `
                -Label "preflight-bootstrap-$($rung.name)" | Out-Null
            $binding = Invoke-Cycle4GenesisManifestBuild `
                -Prefix $rung.prefix `
                -TargetStoreRoot $rung.store_root `
                -TargetChainDir $rung.chain_dir `
                -OutputManifest $rung.manifest `
                -Label "preflight-genesis-build-$($rung.name)"
            if ($null -ne $binding) { $bindings += $binding }
        }

        $phase = 'preflight-locators'
        foreach ($rung in $rungs) {
            # Each rung's own chain directory, since each rung is an
            # independent campaign genesis. Empty for CONTROL-R, which has no
            # baseline chain; the ladder is the CONTROL ladder, so in practice
            # this is always empty today, and stays correct if the ladder is
            # ever pointed at a v4 arm.
            $armBaselineChainDirForPanel = ''
            if (Test-Cycle4ArmUsesBaselineChain -Arm $Arm) {
                $armBaselineChainDirForPanel = [string]$rung.chain_dir
            }
            if ($DryRun -and -not (Test-Path -LiteralPath $rung.manifest -PathType Leaf)) {
                Write-Host "DRY-RUN preflight-locators: would write $($rung.locator) and $($rung.panel_locator) from $($rung.manifest)"
                continue
            }
            $rungManifest = Read-Cycle4Manifest -Path $rung.manifest
            New-Cycle4SlotLocatorPair `
                -SlotTable $slotTable `
                -Manifest $rungManifest `
                -ArmLocatorPath $rung.locator `
                -PanelLocatorPath $rung.panel_locator `
                -ArmRunSha256 $rungManifest.trainee_run_sha256 `
                -ArmStoreRoot $rung.store_root `
                -ArmBaselineChainDir $armBaselineChainDirForPanel `
                -GenesisParentStoreRoot $GenesisParentStoreRoot `
                -AllowMissingStores:$DryRun | Out-Null
        }

        $phase = 'preflight-training'
        foreach ($rung in $rungs) {
            $arguments = @(
                '--arm', $Arm,
                '--store-root', $rung.store_root,
                '--run-record', $RunRecord,
                '--chain-dir', $rung.chain_dir,
                '--refresh-manifest', $rung.manifest,
                '--slot-locator', $rung.locator,
                '--stop-generation', [string]$window,
                '--device', [string]$Device,
                '--preflight',
                '--preflight-updates', [string]$window
            )
            $result = Invoke-Cycle4Process `
                -FilePath $ArmExecutable `
                -Arguments $arguments `
                -WorkingDirectory $RepoRoot `
                -StdoutPath (Join-Path $rung.prefix 'arm.stdout.log') `
                -StderrPath (Join-Path $rung.prefix 'arm.stderr.log') `
                -Label "preflight-rung-$($rung.name)" `
                -DryRun:$DryRun
            Add-Cycle4CommandRecord -Result $result | Out-Null
            Assert-Cycle4ProcessSucceeded -Result $result | Out-Null
        }

        $phase = 'preflight-comparison'
        if ($DryRun) {
            Write-Host "DRY-RUN preflight-comparison: would compare every relative file hash and the endpoint fields of $($rungs[0].store_root) and $($rungs[1].store_root) at generation $window"
            $comparison = [ordered]@{ dry_run = $true; window_updates = $window }
        }
        else {
            $manifestA = Get-Cycle4Sha256 -Path $rungs[0].manifest
            $manifestB = Get-Cycle4Sha256 -Path $rungs[1].manifest
            if ($manifestA -cne $manifestB) {
                throw "preflight ladder genesis manifests differ: $manifestA and $manifestB; the two rungs did not seed the same genesis checkpoint"
            }
            $inventoryA = @(Get-StoreFileInventory -Path $rungs[0].store_root)
            $inventoryB = @(Get-StoreFileInventory -Path $rungs[1].store_root)
            if ($inventoryA.Count -ne $inventoryB.Count) {
                throw "preflight ladder rungs hold different file counts: $($inventoryA.Count) and $($inventoryB.Count)"
            }
            for ($index = 0; $index -lt $inventoryA.Count; $index++) {
                $left = $inventoryA[$index]
                $right = $inventoryB[$index]
                if ($left.path -cne $right.path) {
                    throw "preflight ladder rungs differ in file layout: '$($left.path)' and '$($right.path)'"
                }
                if ([uint64]$left.bytes -ne [uint64]$right.bytes -or $left.sha256 -cne $right.sha256) {
                    throw "preflight ladder rungs differ at $($left.path): $($left.bytes)/$($left.sha256) and $($right.bytes)/$($right.sha256)"
                }
            }
            $endpoints = @()
            foreach ($rung in $rungs) {
                $generation = Get-Cycle4StoreLatestGeneration -StoreRoot $rung.store_root
                if ($generation -ne $window) {
                    throw "preflight rung $($rung.name) stopped at generation $generation, not $window"
                }
                $endpoints += Get-Cycle4CheckpointIdentity -StoreRoot $rung.store_root -StoreGeneration $window
            }
            foreach ($field in @('checkpoint_manifest_sha256', 'checkpoint_payload_sha256', 'model_parameter_sha256', 'train_state_sha256')) {
                if ([string]$endpoints[0].$field -cne [string]$endpoints[1].$field) {
                    throw "preflight ladder endpoint field $field differs: $($endpoints[0].$field) and $($endpoints[1].$field)"
                }
            }
            $treeA = Get-StoreTreeHash -Path $rungs[0].store_root
            $treeB = Get-StoreTreeHash -Path $rungs[1].store_root
            if ($treeA -cne $treeB) {
                throw "preflight ladder store tree hashes differ: $treeA and $treeB"
            }
            $comparison = [ordered]@{
                dry_run = $false
                window_updates = $window
                file_count = $inventoryA.Count
                store_tree_sha256 = @($treeA, $treeB)
                genesis_manifest_sha256 = $manifestA
                endpoint = $endpoints[0]
            }
        }

        $phase = 'preflight-publication'
        # Same rule as the formal branch's terminal marker: a dry run compared
        # nothing, so it claims nothing.
        $result = [ordered]@{
            schema = 'mtg-kernel-cycle4-preflight-ladder-result/v1'
            status = $(if ($DryRun) { 'DRY_RUN_PLANNED' } else { 'PREFLIGHT_COMPLETE' })
            completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
            dry_run = [bool]$DryRun
            arm = $Arm
            window_updates = $window
            checkpoint_segment_updates = $checkpointSegmentUpdates
            rungs = @($rungs | ForEach-Object { $_.store_root })
            genesis_bindings = @($bindings)
            comparison = $comparison
            nonclaim = 'A passed preflight ladder is launcher determinism evidence only; it is not training and not a playing-strength claim.'
        }
        Write-Cycle4JsonFile -Value $result -Path (Join-Path $root 'result.json')
        if ($DryRun) {
            Write-Host "CYCLE4 CONTROL PREFLIGHT DRY RUN PLANNED evidence=$root"
        }
        else {
            Write-Cycle4Marker -Root $root -Name 'PREFLIGHT_COMPLETE' | Out-Null
            Write-Host "CYCLE4 CONTROL PREFLIGHT COMPLETE evidence=$root"
        }
    }
    else {
        # -------------------------------------------------------------------
        # Formal interval loop
        # -------------------------------------------------------------------
        $phase = 'formal-bootstrap'
        $armPrefix = Split-Path -Parent $StoreRoot
        # Written into the panel locator for every slot the manifest binds to
        # this arm's own run: the payoff probe loads a v4 arm's trained own-run
        # checkpoints through the baseline-aware loader, which needs the chain.
        # Empty for CONTROL-R, whose checkpoints load on the frozen v3 path.
        $armBaselineChainDirForPanel = ''
        if (Test-Cycle4ArmUsesBaselineChain -Arm $Arm) {
            $armBaselineChainDirForPanel = [string]$ChainDir
        }
        Invoke-Cycle4Bootstrap `
            -Prefix $root `
            -TargetStoreRoot $StoreRoot `
            -TargetChainDir $ChainDir `
            -Label 'bootstrap-genesis' | Out-Null

        $phase = 'formal-genesis-manifest'
        $genesisBinding = $null
        if (Test-Path -LiteralPath $genesisManifestPath -PathType Leaf) {
            if (Test-Path -LiteralPath (Join-Path $ChainDir $script:Cycle4ArmOriginRecordFileName) -PathType Leaf) {
                $genesisBinding = Assert-Cycle4GenesisManifestBinding `
                    -Manifest (Read-Cycle4Manifest -Path $genesisManifestPath) `
                    -Origin (Read-Cycle4ArmOriginRecord -ChainDir $ChainDir) `
                    -ArmStoreRoot $StoreRoot
            }
            elseif (-not $DryRun) {
                throw "$genesisManifestPath exists but $ChainDir holds no arm-origin record to verify it against"
            }
        }
        else {
            $genesisBinding = Invoke-Cycle4GenesisManifestBuild `
                -Prefix $root `
                -TargetStoreRoot $StoreRoot `
                -TargetChainDir $ChainDir `
                -OutputManifest $genesisManifestPath `
                -Label 'genesis-build'
        }
        if ($null -ne $genesisBinding) {
            Write-Cycle4JsonFile -Value $genesisBinding -Path (Join-Path $root 'genesis-binding.json')
        }

        $phase = 'formal-plan'
        $dryRunStoppedAfter = $null
        if (-not (Test-Path -LiteralPath $genesisManifestPath -PathType Leaf)) {
            if (-not $DryRun) { throw "the genesis manifest was not produced: $genesisManifestPath" }
            # A dry run over a campaign that has not been bootstrapped yet can
            # plan the two genesis steps above and nothing further: every
            # interval's roster, locators and stop generation are read from
            # manifests those steps would have produced.
            Write-Host 'DRY-RUN formal-plan: the interval plan needs the genesis manifest the build above would have produced; stopping here'
            $dryRunStoppedAfter = 'genesis-manifest'
        }
        $armRunSha256 = $null
        $armBaseSeed = [uint64]$runRecordDocument.schedule.base_seed
        if ($null -eq $dryRunStoppedAfter) {
            $genesisManifest = Read-Cycle4Manifest -Path $genesisManifestPath
            if ($genesisManifest.refresh_index -ne [uint64]0) {
                throw "$genesisManifestPath declares refresh index $($genesisManifest.refresh_index), not 0"
            }
            $armRunSha256 = $genesisManifest.trainee_run_sha256
            if ($genesisManifest.trainee_base_seed -ne $armBaseSeed) {
                throw "$genesisManifestPath declares base seed $($genesisManifest.trainee_base_seed), but $RunRecord declares $armBaseSeed"
            }
        }
        $endGeneration = $ThroughRefreshIndex * $script:Cycle4RefreshInterval
        $storeGeneration = Get-Cycle4StoreLatestGeneration -StoreRoot $StoreRoot
        if ($null -eq $storeGeneration) { $storeGeneration = [uint64]0 }
        if ($storeGeneration -gt $endGeneration) {
            throw "Store $StoreRoot is at generation $storeGeneration, past the refresh $ThroughRefreshIndex end ($endGeneration)"
        }
        if (($storeGeneration % $checkpointSegmentUpdates) -ne [uint64]0) {
            throw "Store $StoreRoot is at generation $storeGeneration, which is not a whole number of checkpoint segments ($checkpointSegmentUpdates); a Store can only ever stop on one"
        }

        $journals = Read-Cycle4IntervalJournals -GateRoot $gateRoot

        # Reconstruct what is left to do. An interval is finished only when all
        # three of its outputs exist: its trained generations, its panel, and
        # the next manifest. The Store answers the first. For a refresh-chained
        # arm the chain itself answers the other two, which is the durable
        # answer; for static-rb, whose panel deliberately never enters the
        # chain and which never builds, only the journal can.
        $plan = @()
        $candidateStates = @()
        for ($candidate = [uint64]0; $candidate -lt $ThroughRefreshIndex; $candidate++) {
            $candidateStop = ($candidate + [uint64]1) * $script:Cycle4RefreshInterval
            $recorded = Get-Cycle4IntervalPhase -Journals $journals -IntervalIndex $candidate
            $trainingDone = $storeGeneration -ge $candidateStop
            # Whether a manifest EXISTS for this interval's successor, as
            # opposed to whether one is owed. For static-rb none is ever owed
            # and none may ever exist, so `manifestDone` is a constant true
            # while `manifestPresent` stays false -- without that distinction
            # the campaign-integrity check below reads the constant as "an
            # output is present" and prints an inconsistency warning at every
            # interval of a perfectly fresh static-rb campaign.
            $manifestPresent = $false
            if ($Arm -ceq 'static-rb') {
                $panelDone = $trainingDone -and (@('panel-complete', 'manifest-complete') -contains $recorded)
                $manifestDone = $true
            }
            else {
                $panelDone = Test-Path -LiteralPath (Join-Path $RefreshChainDir (Get-Cycle4ChainPanelName -RefreshIndex ($candidate + [uint64]1))) -PathType Leaf
                $manifestDone = Test-Path -LiteralPath (Join-Path $RefreshChainDir (Get-Cycle4ChainManifestName -RefreshIndex ($candidate + [uint64]1))) -PathType Leaf
                $manifestPresent = $manifestDone
                if ((@('panel-complete', 'manifest-complete') -contains $recorded) -and -not $panelDone) {
                    throw "interval $candidate journalled '$recorded', but refresh $($candidate + 1)'s panel is missing from $RefreshChainDir; refusing to silently re-run a 28-matchup panel over a chain that lost one"
                }
                if ($manifestDone -and -not $panelDone) {
                    throw "$RefreshChainDir holds refresh $($candidate + 1)'s manifest without the panel it binds"
                }
            }
            # A panel or a manifest cannot legitimately exist for an interval
            # the Store has not trained through: each is produced only after
            # that interval's training finished. Finding one means the Store
            # and the refresh chain are not from the same campaign. A dry run
            # says so and plans the interval in full anyway, because its job is
            # to show a plan over whatever directories it was pointed at; a
            # real run stops.
            if (($panelDone -or $manifestPresent) -and -not $trainingDone) {
                $detail = "interval $candidate's outputs are present in $RefreshChainDir but the Store has not trained through generation $candidateStop"
                if (-not $DryRun) {
                    throw "$detail; the Store and the refresh chain are not from the same campaign"
                }
                Write-Host "DRY-RUN formal-plan: $detail; planning the interval in full anyway"
                $panelDone = $false
                $manifestDone = $false
            }
            $candidateStates += [ordered]@{
                interval = [uint64]$candidate
                training_done = [bool]$trainingDone
                work_needed = (-not ($trainingDone -and $panelDone -and $manifestDone))
            }
            if ($trainingDone -and $panelDone -and $manifestDone) { continue }
            $plan += [ordered]@{
                interval = [uint64]$candidate
                refresh_index = $(if ($Arm -ceq 'static-rb') { [uint64]0 } else { [uint64]$candidate })
                stop_generation = $candidateStop
                train = (-not $trainingDone)
                panel = (-not $panelDone)
                manifest = (-not $manifestDone)
                resumed_from_phase = $recorded
            }
        }
        # Campaign work is chronological. Once an interval still needs any
        # work, no later interval can already be trained: that would mean the
        # Store advanced past a hole in the refresh chain. On a fresh campaign
        # every interval needs work and none is trained, which must plan cleanly.
        for ($index = 0; $index -lt $candidateStates.Count; $index++) {
            if (-not $candidateStates[$index].work_needed) { continue }
            $laterTrained = $false
            for ($later = $index + 1; $later -lt $candidateStates.Count; $later++) {
                if ($candidateStates[$later].training_done) { $laterTrained = $true; break }
            }
            if ($laterTrained) {
                $detail = "interval $($candidateStates[$index].interval) still needs training while later intervals are planned after it"
                if (-not $DryRun) {
                    throw "$detail; the Store and $RefreshChainDir disagree"
                }
                Write-Host "DRY-RUN formal-plan: $detail; planning them in order anyway"
                break
            }
        }
        if ($null -ne $dryRunStoppedAfter) { $plan = @() }
        $startInterval = $(if ($plan.Count -eq 0) { $ThroughRefreshIndex } else { [uint64]$plan[0].interval })

        function Write-Cycle4Phase {
            # Journals one transition, or says what it would journal. A dry run
            # never writes: its records would otherwise be read back by the next
            # real attempt as progress that never happened.
            param(
                [Parameter(Mandatory = $true)][uint64]$IntervalIndex,
                [Parameter(Mandatory = $true)][uint64]$RefreshIndex,
                [Parameter(Mandatory = $true)][uint64]$StopGeneration,
                [Parameter(Mandatory = $true)][string]$Transition
            )
            if ($DryRun) {
                Write-Host "DRY-RUN interval-$IntervalIndex`: would journal $Transition"
                return
            }
            Add-Cycle4IntervalPhase `
                -Journals $journals `
                -AttemptRoot $root `
                -Arm $Arm `
                -IntervalIndex $IntervalIndex `
                -RefreshIndex $RefreshIndex `
                -StopGeneration $StopGeneration `
                -Phase $Transition | Out-Null
        }

        foreach ($step in $plan) {
            $interval = [uint64]$step.interval
            $phase = "interval-$interval"
            $panelIndex = $interval + [uint64]1
            $refreshIndex = [uint64]$step.refresh_index
            $stopGeneration = [uint64]$step.stop_generation
            if ($null -ne $step.resumed_from_phase) {
                Write-Host "interval-$interval`: resuming an interrupted interval, last journalled phase '$($step.resumed_from_phase)'"
            }
            $manifestPath = Join-Path $RefreshChainDir (Get-Cycle4ChainManifestName -RefreshIndex $refreshIndex)
            if ($DryRun -and -not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
                Write-Host "DRY-RUN interval-$interval`: the manifest a prior planned build would produce is not present yet; stopping detailed command expansion"
                $dryRunStoppedAfter = "refresh-$refreshIndex-manifest"
                break
            }
            $manifest = Read-Cycle4Manifest -Path $manifestPath
            if ($manifest.refresh_index -ne $refreshIndex) {
                throw "$manifestPath declares refresh index $($manifest.refresh_index), not $refreshIndex"
            }
            if ($manifest.trainee_run_sha256 -cne $armRunSha256 -or $manifest.trainee_base_seed -ne $armBaseSeed) {
                throw "$manifestPath binds a different trainee identity than the genesis manifest"
            }
            if ($Arm -ceq 'static-rb') {
                $advanced = Join-Path $RefreshChainDir (Get-Cycle4ChainManifestName -RefreshIndex ([uint64]1))
                if (Test-Path -LiteralPath $advanced -PathType Leaf) {
                    throw "static-rb never advances the manifest past genesis, but $advanced exists"
                }
            }

            $intervalRoot = Join-Path $root ('interval-{0:d2}' -f $interval)
            New-Item -ItemType Directory -Force -Path $intervalRoot | Out-Null
            $armLocator = Join-Path $intervalRoot 'arm-slot-locator.json'
            $panelLocator = Join-Path $intervalRoot 'panel-slot-locator.json'
            # Rebuilt per boundary: historical-1 rotates by refresh index, so
            # the table this interval writes its locators from is not the one
            # the previous interval used. static-rb always reuses the genesis
            # manifest, so its refresh index -- and its rotation phase -- stay
            # 0 for the whole campaign, which is what a static pool means.
            $intervalSlotTable = Get-Cycle4SlotTableForRefresh `
                -SlotStoreRoots $SlotStoreRoots `
                -HistoricalOneStoreRoots $HistoricalOneStoreRoots `
                -RefreshIndex ([uint64]$manifest.refresh_index)
            New-Cycle4SlotLocatorPair `
                -SlotTable $intervalSlotTable `
                -Manifest $manifest `
                -ArmLocatorPath $armLocator `
                -PanelLocatorPath $panelLocator `
                -ArmRunSha256 $armRunSha256 `
                -ArmStoreRoot $StoreRoot `
                -ArmBaselineChainDir $armBaselineChainDirForPanel `
                -GenesisParentStoreRoot $GenesisParentStoreRoot `
                -AllowMissingStores:$DryRun | Out-Null

            $intervalStart = $interval * $script:Cycle4RefreshInterval
            if (-not $step.train) {
                Write-Host "interval-$interval`: already trained to generation $stopGeneration; finishing its pending outputs before advancing"
            }
            else {
            if ($DryRun -and $interval -ne $startInterval) {
                Write-Host "DRY-RUN interval-$interval`: would assert $StoreRoot is inside [$intervalStart, $stopGeneration)"
            }
            else {
                # An interrupted interval leaves the Store on a checkpoint
                # segment inside its window, not on the refresh boundary. That
                # is a legal resume point for the SAME stop generation this
                # interval was started with, and for no other, which is why the
                # stop below is derived from the interval rather than from
                # wherever the Store happens to be.
                $actual = Get-Cycle4StoreLatestGeneration -StoreRoot $StoreRoot
                if ($null -eq $actual) { $actual = [uint64]0 }
                if ($actual -lt $intervalStart -or $actual -ge $stopGeneration) {
                    throw "resume assertion FAILED: $StoreRoot is at generation $actual, outside interval $interval's window [$intervalStart, $stopGeneration)"
                }
                if ($actual -ne $intervalStart) {
                    Write-Host "interval-$interval`: resuming mid-interval from generation $actual toward its original stop generation $stopGeneration"
                }
            }
            Write-Cycle4Phase -IntervalIndex $interval -RefreshIndex $refreshIndex -StopGeneration $stopGeneration -Transition 'training-started'

            $armArguments = @(
                '--arm', $Arm,
                '--store-root', $StoreRoot,
                '--run-record', $RunRecord,
                '--chain-dir', $ChainDir,
                '--refresh-manifest', $manifestPath
            )
            if ($refreshIndex -ge [uint64]1) {
                $boundPanel = Join-Path $RefreshChainDir (Get-Cycle4ChainPanelName -RefreshIndex $refreshIndex)
                if (-not $DryRun -and -not (Test-Path -LiteralPath $boundPanel -PathType Leaf)) {
                    throw "manifest $manifestPath binds a payoff panel that is missing: $boundPanel"
                }
                $armArguments += @('--payoff-panel', $boundPanel)
            }
            $armArguments += @(
                '--slot-locator', $armLocator,
                '--stop-generation', [string]$stopGeneration,
                '--device', [string]$Device
            )
            $result = Invoke-Cycle4Process `
                -FilePath $ArmExecutable `
                -Arguments $armArguments `
                -WorkingDirectory $RepoRoot `
                -StdoutPath (Join-Path $intervalRoot 'arm.stdout.log') `
                -StderrPath (Join-Path $intervalRoot 'arm.stderr.log') `
                -Label "arm-interval-$interval" `
                -DryRun:$DryRun
            Add-Cycle4CommandRecord -Result $result | Out-Null
            Assert-Cycle4ProcessSucceeded -Result $result | Out-Null
            if (-not $DryRun) {
                Assert-Cycle4ResumePosition -StoreRoot $StoreRoot -ExpectedGeneration $stopGeneration | Out-Null
            }
            Write-Cycle4Phase -IntervalIndex $interval -RefreshIndex $refreshIndex -StopGeneration $stopGeneration -Transition 'training-complete'
            }

            # Payoff panel over THIS manifest's roster. Its own output name is
            # derived from the manifest's refresh index plus one, which is the
            # panel the next manifest binds by hash.
            $phase = "interval-$interval-panel"
            if (-not $step.panel) {
                Write-Host "interval-$interval`: its panel is already published; skipping"
            }
            else {
            $panelOutputDir = Join-Path $intervalRoot 'panel'
            $panelSeed = $PanelBaseSeed + ($panelIndex * $script:Cycle4PanelSeedStridePerRefresh)
            $panelArguments = @(
                (Join-Path $PSScriptRoot 'run_payoff_panel_v1.py'),
                '--manifest', $manifestPath,
                '--slot-locator', $panelLocator,
                # Which slots must carry baseline_chain_dir is a function of
                # the arm kind, and the manifest carries none (the roster is
                # the same for all three arms), so the runner is told.
                '--arm', $Arm,
                '--games-per-matchup', [string]$script:Cycle4PanelGamesPerMatchup,
                '--base-seed', [string]$panelSeed,
                '--output-dir', $panelOutputDir,
                '--executable', $PanelExecutable,
                '--repo-root', $RepoRoot
            )
            $result = Invoke-Cycle4Process `
                -FilePath $PythonExecutable `
                -Arguments $panelArguments `
                -WorkingDirectory $RepoRoot `
                -StdoutPath (Join-Path $intervalRoot 'panel.stdout.log') `
                -StderrPath (Join-Path $intervalRoot 'panel.stderr.log') `
                -Label "panel-interval-$interval" `
                -Environment @{ CUDA_DEVICE_ORDER = 'PCI_BUS_ID'; CUDA_VISIBLE_DEVICES = [string]$Device; MTG_KERNEL_PILOT_CUDA_ORDINAL = '0' } `
                -DryRun:$DryRun
            Add-Cycle4CommandRecord -Result $result | Out-Null
            Assert-Cycle4ProcessSucceeded -Result $result | Out-Null
            $producedPanel = Join-Path $panelOutputDir (Get-Cycle4ChainPanelName -RefreshIndex ($manifest.refresh_index + [uint64]1))

            if ($Arm -ceq 'static-rb') {
                # The panel still runs -- STATIC-RB's pool is measured like
                # every other arm's -- but it never enters the refresh chain,
                # so no manifest can ever be built from it. The journal is
                # therefore the only durable record that it ran.
                $advanced = Join-Path $RefreshChainDir (Get-Cycle4ChainManifestName -RefreshIndex ([uint64]1))
                if (Test-Path -LiteralPath $advanced -PathType Leaf) {
                    throw "static-rb never advances the manifest past genesis, but $advanced appeared during interval $interval"
                }
                Write-Cycle4Phase -IntervalIndex $interval -RefreshIndex $refreshIndex -StopGeneration $stopGeneration -Transition 'panel-complete'
                continue
            }

            # The panel counts as complete only once its bytes are in the
            # refresh chain, because that copy is what the builder and every
            # later interval read. Doing the copy here, before the journal
            # entry, is what makes 'panel-complete' and the chain agree by
            # construction, so a resume never has to guess which of the two is
            # authoritative.
            $chainPanel = Join-Path $RefreshChainDir (Get-Cycle4ChainPanelName -RefreshIndex $panelIndex)
            if ($DryRun) {
                Write-Host "DRY-RUN interval-$interval`: would publish $producedPanel to $chainPanel"
            }
            else {
                if (-not (Test-Path -LiteralPath $producedPanel -PathType Leaf)) {
                    throw "the panel runner did not publish $producedPanel"
                }
                Copy-Item -LiteralPath $producedPanel -Destination "$chainPanel.stage-$PID" -Force
                Move-Item -LiteralPath "$chainPanel.stage-$PID" -Destination $chainPanel -Force
            }
            Write-Cycle4Phase -IntervalIndex $interval -RefreshIndex $refreshIndex -StopGeneration $stopGeneration -Transition 'panel-complete'
            }

            if ($Arm -ceq 'static-rb') { continue }

            $phase = "interval-$interval-refresh"
            if (-not $step.manifest) {
                Write-Host "interval-$interval`: refresh $panelIndex's manifest already exists; skipping"
                continue
            }
            $chainPanel = Join-Path $RefreshChainDir (Get-Cycle4ChainPanelName -RefreshIndex $panelIndex)
            $nextManifest = Join-Path $RefreshChainDir (Get-Cycle4ChainManifestName -RefreshIndex $panelIndex)
            $stagedIdentities = Join-Path $intervalRoot ('refresh-{0:d2}.slot-identities.json' -f $panelIndex)
            $rosterPath = Join-Path $SlotIdentitiesRosterDir ('refresh-{0:d2}.slot-identities.json' -f $panelIndex)

            if ($DryRun) {
                try {
                    New-Cycle4SlotIdentitiesFile `
                        -RosterPath $rosterPath `
                        -OutputPath $stagedIdentities `
                        -RefreshIndex $panelIndex `
                        -ArmStoreRoot $StoreRoot `
                        -ArmRunSha256 $armRunSha256 `
                        -ArmBaseSeed $armBaseSeed | Out-Null
                }
                catch {
                    Write-Host "DRY-RUN interval-$interval`: would stage $rosterPath to $stagedIdentities with the arm's own slots read from $StoreRoot ($($_.Exception.Message))"
                }
            }
            else {
                # Reached either straight from this interval's panel step or on
                # a resume that found the panel already in the chain.
                if (-not (Test-Path -LiteralPath $chainPanel -PathType Leaf)) {
                    throw "refresh $panelIndex's manifest cannot be built: $chainPanel is missing"
                }
                New-Cycle4SlotIdentitiesFile `
                    -RosterPath $rosterPath `
                    -OutputPath $stagedIdentities `
                    -RefreshIndex $panelIndex `
                    -ArmStoreRoot $StoreRoot `
                    -ArmRunSha256 $armRunSha256 `
                    -ArmBaseSeed $armBaseSeed | Out-Null
            }

            $builderArguments = @(
                '--chain-dir', $RefreshChainDir,
                '--panel', $chainPanel,
                '--next-generation', [string]$panelIndex,
                '--trainee-run-sha256', $armRunSha256,
                '--trainee-base-seed', [string]$armBaseSeed,
                '--slot-identities', $stagedIdentities,
                '--output', $nextManifest
            )
            $result = Invoke-Cycle4Process `
                -FilePath $RefreshBuilderExecutable `
                -Arguments $builderArguments `
                -WorkingDirectory $RepoRoot `
                -StdoutPath (Join-Path $intervalRoot 'refresh-build.stdout.log') `
                -StderrPath (Join-Path $intervalRoot 'refresh-build.stderr.log') `
                -Label "refresh-build-$panelIndex" `
                -DryRun:$DryRun
            Add-Cycle4CommandRecord -Result $result | Out-Null
            Assert-Cycle4ProcessSucceeded -Result $result | Out-Null
            if (-not $DryRun) {
                $built = Read-Cycle4Manifest -Path $nextManifest
                if ($built.refresh_index -ne $panelIndex) {
                    throw "$nextManifest declares refresh index $($built.refresh_index), not $panelIndex"
                }
                if ($built.trainee_run_sha256 -cne $armRunSha256 -or $built.trainee_base_seed -ne $armBaseSeed) {
                    throw "$nextManifest binds a different trainee identity than the genesis manifest"
                }
            }
            Write-Cycle4Phase -IntervalIndex $interval -RefreshIndex $refreshIndex -StopGeneration $stopGeneration -Transition 'manifest-complete'
        }

        # ------------------------------------------------------------------
        # Nothing is complete until the whole chain is. An interrupted attempt
        # can leave the Store at the program end with the last interval's panel
        # and manifest missing, which is exactly the state that must NOT
        # publish TRAINING_COMPLETE.
        # ------------------------------------------------------------------
        $phase = 'formal-verification'
        $chainVerification = $null
        if ($DryRun) {
            Write-Host "DRY-RUN formal-verification: would require $StoreRoot at generation $endGeneration and every panel and manifest through refresh $ThroughRefreshIndex"
        }
        elseif ($Arm -ceq 'static-rb') {
            Assert-Cycle4ResumePosition -StoreRoot $StoreRoot -ExpectedGeneration $endGeneration | Out-Null
            $advanced = Join-Path $RefreshChainDir (Get-Cycle4ChainManifestName -RefreshIndex ([uint64]1))
            if (Test-Path -LiteralPath $advanced -PathType Leaf) {
                throw "static-rb never advances the manifest past genesis, but $advanced exists"
            }
            for ($index = [uint64]0; $index -lt $ThroughRefreshIndex; $index++) {
                $recorded = Get-Cycle4IntervalPhase -Journals $journals -IntervalIndex $index
                if (@('panel-complete', 'manifest-complete') -notcontains $recorded) {
                    throw "refusing to publish TRAINING_COMPLETE: static-rb interval $index's panel is not journalled complete (last phase '$recorded')"
                }
            }
            $chainVerification = [ordered]@{
                arm = $Arm
                static_pool = $true
                final_generation = $endGeneration
                intervals_verified = $ThroughRefreshIndex
            }
        }
        else {
            Assert-Cycle4ResumePosition -StoreRoot $StoreRoot -ExpectedGeneration $endGeneration | Out-Null
            $links = @()
            for ($index = [uint64]1; $index -le $ThroughRefreshIndex; $index++) {
                $linkPanel = Join-Path $RefreshChainDir (Get-Cycle4ChainPanelName -RefreshIndex $index)
                $linkManifest = Join-Path $RefreshChainDir (Get-Cycle4ChainManifestName -RefreshIndex $index)
                foreach ($required in @($linkPanel, $linkManifest)) {
                    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
                        throw "refusing to publish TRAINING_COMPLETE: $required is missing"
                    }
                }
                $link = Read-Cycle4Manifest -Path $linkManifest
                if ([uint64]$link.refresh_index -ne [uint64]$index) {
                    throw "$linkManifest declares refresh index $($link.refresh_index), not $index"
                }
                if ($link.trainee_run_sha256 -cne $armRunSha256 -or $link.trainee_base_seed -ne $armBaseSeed) {
                    throw "$linkManifest binds a different trainee identity than the genesis manifest"
                }
                $links += [ordered]@{
                    refresh_index = [uint64]$index
                    manifest_sha256 = $link.sha256
                    panel_sha256 = (Get-Cycle4Sha256 -Path $linkPanel)
                }
            }
            $chainVerification = [ordered]@{
                arm = $Arm
                static_pool = $false
                final_generation = $endGeneration
                links = @($links)
            }
        }

        $phase = 'formal-publication'
        if (-not $DryRun -and -not $SkipHostAssertions -and $Device -eq 1) {
            Assert-Gpu1Idle | Out-Null
            Assert-NoForeignGpu1ComputeProcesses
        }
        # A dry run trained nothing, so it may claim nothing: no
        # TRAINING_COMPLETE status and no TRAINING_COMPLETE marker. The marker
        # in particular is read by operators and by later tooling as "this arm
        # finished"; a dry run that left one behind would make a campaign that
        # never ran look complete.
        $trainingResult = [ordered]@{
            schema = 'mtg-kernel-cycle4-arm-training-result/v1'
            status = $(if ($DryRun) { 'DRY_RUN_PLANNED' } else { 'TRAINING_COMPLETE' })
            completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
            arm = $Arm
            dry_run = [bool]$DryRun
            start_interval_index = $startInterval
            through_refresh_index = $ThroughRefreshIndex
            intervals_planned = @($plan)
            chain_verification = $chainVerification
            dry_run_stopped_after = $dryRunStoppedAfter
            genesis_binding = $genesisBinding
            store_root = $StoreRoot
            store_prefix = $armPrefix
            final_store_generation = $(if ($DryRun) { $null } else { Get-Cycle4StoreLatestGeneration -StoreRoot $StoreRoot })
            arm_origin_record = $(if ($DryRun -or -not (Test-Path -LiteralPath (Join-Path $ChainDir $script:Cycle4ArmOriginRecordFileName) -PathType Leaf)) { $null } else { Get-Cycle4FileRecord -Path (Join-Path $ChainDir $script:Cycle4ArmOriginRecordFileName) })
            store_mode_marker = $(if ($DryRun -or -not (Test-Path -LiteralPath (Join-Path (Split-Path -Parent $StoreRoot) $script:Cycle4ModeMarkerFileName) -PathType Leaf)) { $null } else { Get-Cycle4FileRecord -Path (Join-Path (Split-Path -Parent $StoreRoot) $script:Cycle4ModeMarkerFileName) })
            command_log = $commandLog
            nonclaim = 'Training completion is not playing-strength evidence; the derived metric is the payoff panel and its BT ratings.'
        }
        Write-Cycle4JsonFile -Value $trainingResult -Path (Join-Path $root 'result.json')
        if ($DryRun) {
            Write-Host "CYCLE4 DRY RUN PLANNED arm=$Arm evidence=$root"
        }
        else {
            Write-Cycle4Marker -Root $root -Name 'TRAINING_COMPLETE' | Out-Null
            Write-Host "CYCLE4 ARM TRAINING COMPLETE arm=$Arm evidence=$root"
        }
    }
}
catch {
    # Captured before anything else can run: the inner try/catch below
    # rebinds $_, so a bare `throw` at the end would rethrow the WRONG error.
    $originalError = $_
    $failureMessage = $originalError.Exception.Message
    Write-Cycle4RunFailed -Root $root -Phase $phase -Message $failureMessage | Out-Null
    # Round F defect 5: a failed attempt publishes the same document a
    # successful one does, with status RUN_FAILED, so the file every reader
    # of this evidence tree opens first exists on both paths. Best-effort:
    # the real error is what must reach the operator, so a failure to write
    # the failure document is reported and swallowed, never substituted.
    try {
        Write-Cycle4FailureResult `
            -Root $root `
            -Phase $phase `
            -Message $failureMessage `
            -Arm $Arm `
            -Mode $Mode `
            -DryRun ([bool]$DryRun) `
            -CommandLog $commandLog | Out-Null
    }
    catch {
        Write-Host "WARNING: could not write the RUN_FAILED result document: $($_.Exception.Message)"
    }
    throw $originalError
}
