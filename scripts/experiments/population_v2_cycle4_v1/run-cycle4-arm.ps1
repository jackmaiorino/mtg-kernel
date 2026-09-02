<#
.SYNOPSIS
Cycle-4 arm launcher wrapper (docs/native_cycle4_arm_launcher_v1.md Section 6).

.DESCRIPTION
Drives one cycle-4 arm end to end, or the CONTROL preflight ladder that must
pass before any arm launches. Nothing here trains: every unit of work is a
child process (the arm bin, the payoff panel runner, the refresh builder bin)
whose exit code this wrapper captures and whose inputs it proves first.

Formal mode:

  0. if the Store holds no genesis, run cycle4_arm_v1 --bootstrap-genesis,
     which seeds it from the pinned parent and trains nothing
  0b. if refresh-00.manifest.json does not exist, build it with
     cycle4_refresh_build_v1 --genesis from the operator's pinned roster with
     the own-run slot filled from the freshly bootstrapped Store, then assert
     it binds the Store's actual genesis checkpoint

then per interval, through refresh 16:

  1. assert the Store's own resume position equals interval * 128
  2. run cycle4_arm_v1 with --stop-generation = position + 128
  3. assert the Store advanced to exactly position + 128
  4. run the payoff panel over the interval's manifest roster
  5. build the next refresh manifest from the chain plus that panel

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
and a plain-text RUN_FAILED naming the failing step on any error.
#>
param(
    [Parameter(Mandatory = $true)][ValidateSet('formal', 'preflight')][string]$Mode,
    [Parameter(Mandatory = $true)][ValidateSet('control-r', 'static-rb', 'treatment-rb')][string]$Arm,
    [Parameter(Mandatory = $true)][string]$EvidenceRoot,
    [Parameter(Mandatory = $true)][string]$RunRecord,
    [Parameter(Mandatory = $true)][string]$RefreshChainDir,
    [Parameter(Mandatory = $true)][string]$SlotIdentitiesRosterDir,
    [Parameter(Mandatory = $true)][string[]]$SlotStoreRoots,
    [Parameter(Mandatory = $true)][string]$GenesisParentStoreRoot,
    [Parameter(Mandatory = $true)][uint64]$GenesisParentGeneration,
    [Parameter(Mandatory = $true)][string]$ArmExecutable,
    [Parameter(Mandatory = $true)][string]$RefreshBuilderExecutable,
    [Parameter(Mandatory = $true)][string]$PanelExecutable,
    [Parameter(Mandatory = $true)][string]$PythonExecutable,
    [Parameter(Mandatory = $true)][uint64]$PanelBaseSeed,
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

$gateName = "cycle4-$Arm-$Mode"
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

    $runRecordDocument = Read-Cycle4Json -Path $RunRecord
    $checkpointSegmentUpdates = [uint64]$runRecordDocument.schedule.checkpoint_segment_updates
    if ($checkpointSegmentUpdates -eq [uint64]0) {
        throw "$RunRecord declares checkpoint_segment_updates = 0"
    }

    $inputRecords = [ordered]@{
        run_record = Get-Cycle4FileRecord -Path $RunRecord
        genesis_slot_identities_roster = Get-Cycle4FileRecord -Path $genesisRosterPath
        arm_executable = Get-Cycle4FileRecord -Path $ArmExecutable
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
    if (-not $SkipHostAssertions) {
        $gitRecord = Get-Cycle4GitRecord -RepoRoot $RepoRoot
        $toolchainRecord = Get-ToolchainRecord
        $gpuRecord = Assert-GpuIdentity -Ordinal $Device
        if ($Device -eq 1) {
            Assert-Gpu1Idle | Out-Null
            Assert-NoForeignGpu1ComputeProcesses
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
        refresh_chain_dir = $RefreshChainDir
        slot_identities_roster_dir = $SlotIdentitiesRosterDir
        store_root = $StoreRoot
        chain_dir = $ChainDir
        inputs = $inputRecords
        git = $gitRecord
        toolchain = $toolchainRecord
        gpu = $gpuRecord
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

    $slotTable = @(
        foreach ($index in 0..($script:Cycle4SlotCount - 1)) {
            [ordered]@{ slot_index = $index; store_root = [string]@($SlotStoreRoots)[$index] }
        }
    )

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
        $result = [ordered]@{
            schema = 'mtg-kernel-cycle4-preflight-ladder-result/v1'
            status = 'PREFLIGHT_COMPLETE'
            completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
            arm = $Arm
            window_updates = $window
            checkpoint_segment_updates = $checkpointSegmentUpdates
            rungs = @($rungs | ForEach-Object { $_.store_root })
            genesis_bindings = @($bindings)
            comparison = $comparison
            nonclaim = 'A passed preflight ladder is launcher determinism evidence only; it is not training and not a playing-strength claim.'
        }
        Write-Cycle4JsonFile -Value $result -Path (Join-Path $root 'result.json')
        Write-Cycle4Marker -Root $root -Name 'PREFLIGHT_COMPLETE' | Out-Null
        Write-Host "CYCLE4 CONTROL PREFLIGHT COMPLETE evidence=$root"
    }
    else {
        # -------------------------------------------------------------------
        # Formal interval loop
        # -------------------------------------------------------------------
        $phase = 'formal-bootstrap'
        $armPrefix = Split-Path -Parent $StoreRoot
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
        $storeGeneration = Get-Cycle4StoreLatestGeneration -StoreRoot $StoreRoot
        if ($null -eq $storeGeneration) { $storeGeneration = [uint64]0 }
        if (($storeGeneration % $script:Cycle4RefreshInterval) -ne [uint64]0) {
            throw "Store $StoreRoot is at generation $storeGeneration, which is not a refresh boundary; a cycle-4 arm only ever resumes on a boundary"
        }
        $startInterval = [uint64]($storeGeneration / $script:Cycle4RefreshInterval)
        if ($startInterval -gt $ThroughRefreshIndex) {
            throw "Store $StoreRoot is already past refresh $ThroughRefreshIndex (generation $storeGeneration)"
        }

        $intervals = @()
        if ($null -eq $dryRunStoppedAfter) {
            for ($interval = $startInterval; $interval -lt $ThroughRefreshIndex; $interval++) {
                $intervals += [uint64]$interval
            }
        }

        foreach ($interval in $intervals) {
            $phase = "interval-$interval"
            $panelIndex = $interval + [uint64]1
            if ($Arm -ceq 'static-rb') { $refreshIndex = [uint64]0 } else { $refreshIndex = [uint64]$interval }
            $manifestPath = Join-Path $RefreshChainDir (Get-Cycle4ChainManifestName -RefreshIndex $refreshIndex)
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
            New-Cycle4SlotLocatorPair `
                -SlotTable $slotTable `
                -Manifest $manifest `
                -ArmLocatorPath $armLocator `
                -PanelLocatorPath $panelLocator `
                -ArmRunSha256 $armRunSha256 `
                -ArmStoreRoot $StoreRoot `
                -GenesisParentStoreRoot $GenesisParentStoreRoot `
                -AllowMissingStores:$DryRun | Out-Null

            $resumeGeneration = $interval * $script:Cycle4RefreshInterval
            $stopGeneration = $resumeGeneration + $script:Cycle4RefreshInterval
            if ($DryRun -and $interval -ne $startInterval) {
                Write-Host "DRY-RUN interval-$interval`: would assert $StoreRoot is at generation $resumeGeneration"
            }
            else {
                Assert-Cycle4ResumePosition -StoreRoot $StoreRoot -ExpectedGeneration $resumeGeneration | Out-Null
            }

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

            # Payoff panel over THIS manifest's roster. Its own output name is
            # derived from the manifest's refresh index plus one, which is the
            # panel the next manifest binds by hash.
            $phase = "interval-$interval-panel"
            $panelOutputDir = Join-Path $intervalRoot 'panel'
            $panelSeed = $PanelBaseSeed + ($panelIndex * $script:Cycle4PanelSeedStridePerRefresh)
            $panelArguments = @(
                (Join-Path $PSScriptRoot 'run_payoff_panel_v1.py'),
                '--manifest', $manifestPath,
                '--slot-locator', $panelLocator,
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
                # so no manifest can ever be built from it.
                $advanced = Join-Path $RefreshChainDir (Get-Cycle4ChainManifestName -RefreshIndex ([uint64]1))
                if (Test-Path -LiteralPath $advanced -PathType Leaf) {
                    throw "static-rb never advances the manifest past genesis, but $advanced appeared during interval $interval"
                }
                continue
            }

            $phase = "interval-$interval-refresh"
            $chainPanel = Join-Path $RefreshChainDir (Get-Cycle4ChainPanelName -RefreshIndex $panelIndex)
            $nextManifest = Join-Path $RefreshChainDir (Get-Cycle4ChainManifestName -RefreshIndex $panelIndex)
            $stagedIdentities = Join-Path $intervalRoot ('refresh-{0:d2}.slot-identities.json' -f $panelIndex)
            $rosterPath = Join-Path $SlotIdentitiesRosterDir ('refresh-{0:d2}.slot-identities.json' -f $panelIndex)

            if ($DryRun) {
                Write-Host "DRY-RUN interval-$interval`: would publish $producedPanel to $chainPanel"
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
                if (-not (Test-Path -LiteralPath $producedPanel -PathType Leaf)) {
                    throw "the panel runner did not publish $producedPanel"
                }
                Copy-Item -LiteralPath $producedPanel -Destination "$chainPanel.stage-$PID" -Force
                Move-Item -LiteralPath "$chainPanel.stage-$PID" -Destination $chainPanel -Force
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
        }

        $phase = 'formal-publication'
        if (-not $DryRun -and -not $SkipHostAssertions -and $Device -eq 1) {
            Assert-Gpu1Idle | Out-Null
            Assert-NoForeignGpu1ComputeProcesses
        }
        $trainingResult = [ordered]@{
            schema = 'mtg-kernel-cycle4-arm-training-result/v1'
            status = 'TRAINING_COMPLETE'
            completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
            arm = $Arm
            dry_run = [bool]$DryRun
            start_interval_index = $startInterval
            through_refresh_index = $ThroughRefreshIndex
            intervals_run = @($intervals)
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
        Write-Cycle4Marker -Root $root -Name 'TRAINING_COMPLETE' | Out-Null
        Write-Host "CYCLE4 ARM TRAINING COMPLETE arm=$Arm evidence=$root"
    }
}
catch {
    Write-Cycle4RunFailed -Root $root -Phase $phase -Message $_.Exception.Message | Out-Null
    throw
}
