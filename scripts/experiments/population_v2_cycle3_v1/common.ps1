Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Cycle-3 launch stack (CLAUDE-POPULATION-V2-CYCLE3-SHEET-V1.md, countersigned
# through Amendment 2 + Fixes 1-2, file SHA
# 1efa40979de0d4e8f3105d1c266b676b0c2a57c320994703b355a1989cdd1c0a; also
# CLAUDE-SEARCHER-POOL-AUTHORITY-SHEET-V1.md through its own Amendment 2
# countersign). Dot-sources the existing scaled_selfplay_population_v1 chain
# (which itself dot-sources regularized_continuation_retest_v1) so every
# lower-level helper (Get-StoreTreeHash, Get-ScaledEndpointRecord,
# Assert-GpuIdentity, Write-JsonFile, New-UniqueAttemptRoot, ...) is available
# unchanged. This file only adds cycle-3-specific identities, the
# MULTIRUN_POPULATION_CYCLE3_AUTHORITY environment path (Set-ScaledNativeEnvironment's
# -Mode ValidateSet is NOT widened -- that is shared infrastructure every
# other campaign script also depends on; this is a parallel, cycle-3-owned
# environment setter instead), and the Amendment 2 A2.1 Layer 2 warm-start
# assert (Assert-WarmStartGenZero pattern, reimplemented here for cycle-3's
# single-lineage store layout, not the response-exploiter launcher's
# multi-lane array shape).

$script:Cycle3ScriptRoot = $PSScriptRoot
. (Join-Path $PSScriptRoot '..\scaled_selfplay_population_v1\common.ps1')
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path

# ---------------------------------------------------------------------------
# Sheet-bound identities
# ---------------------------------------------------------------------------
$script:Cycle3SheetPath = 'C:\Users\Jack\IdeaProjects\collab\CLAUDE-POPULATION-V2-CYCLE3-SHEET-V1.md'
# Updated to the current countersigned SHA after Amendment 8 (countersigned
# 96a65dce..., first round, zero discrepancies; plus its post-countersign
# implementation-record entry noting the chain-decode gate's own HEAVY
# constant -- native_population_refresh_manifest_v1.rs:2332 -- as a third,
# compiled notation of the heavy-window schedule, updated
# [20,25,29,34]->[20,25,29,33] at commit fd8540b to implement the repair):
# eca69e0a...; superseded the Amendment-7-v2-era 0b857412... pin (A7.3 gate
# mechanisms correctly and distinctly named, cfg(test) documentation note,
# corrected Task 1 kill-evidence preamble), which superseded the
# Amendment-6-v2-era 394699e7... pin, which superseded the Amendment-5-era
# 005d9500... pin, which superseded the Amendment-4-era 00affe6a... pin,
# which superseded the original Amendment-3-era 1efa4097... pin the launch
# stack was first built against.
$script:Cycle3SheetSha256 = 'eca69e0a410b790efe6a75a88598f83201392337e5e3a562f282259b0ee63c07'
$script:SearcherPoolAuthoritySheetPath = 'C:\Users\Jack\IdeaProjects\collab\CLAUDE-SEARCHER-POOL-AUTHORITY-SHEET-V1.md'
$script:SearcherPoolAuthoritySheetSha256 = '87a3ef08df86e7b0f8b0b3e2674bcd890a8c09953718646d10193b6fd7c4ea34'

# ---------------------------------------------------------------------------
# Cycle-3 eligible-parent identity (sheet Section 2.3; re-verified byte-for-
# byte against the live extracted store at implementation time -- see the
# implementation report). MUST stay byte-identical to the
# POPULATION_CYCLE3_PARENT_*_V1 Rust constants in native_training_store_run_v2.rs
# (checklist rule adopted at the searcher-sheet Amendment 2 countersign:
# shared numeric literals verify at the referencing document's peak usage,
# with a re-verification convention on any change to a shared literal).
# ---------------------------------------------------------------------------
$script:Cycle3ParentStoreRoot = 'E:\mtg-kernel-population-v2-cycle3\parent-import\current-1-seed-975002-store\run-0\store'
$script:Cycle3ParentGeneration = [uint64]2048
$script:Cycle3ParentBaseSeed = [uint64]972002
$script:Cycle3ParentCheckpointSha256 = '5e1ff645091bfacdade2a3e06b47c3cd71c96ed1c9fee4dd9756b343d7c834fd'
$script:Cycle3ParentSidecarSha256 = '81ab98f52c37cb14a8305f48674133148ac0d06df516c1a12510e5350bb62133'
$script:Cycle3ParentStateSha256 = 'e4aa3172bf3962af1498028f19649a85424d0e30f226b5c1f6722160fb24a2d4'
$script:Cycle3ParentModelParameterSha256 = '67c5d0a2c506c0514623f3f4ea0f273b904662cbdae4f6ddc89c44e255b9a70d'
$script:Cycle3ParentRunSha256 = '8d9a8287ef57651d5744d26275d2a8c0dc74cfb69cb7e1b2dd22691b5bd8a504'
$script:Cycle3ParentStoreTreeSha256 = '06d9e67a3bb56fb716d5d0208c7adf8897c6f996851c6883418563f2fa143a79'

# Cycle-3's own lineage store root (Task 3, Physical Genesis) and cargo target.
$script:Cycle3LineageRoot = 'E:\mtg-kernel-population-v2-cycle3\lineage'
$script:Cycle3CargoTargetDir = 'D:\cargo-target-throughput-remeasure-v1'

function Assert-Cycle3SheetIdentity {
    # Mirrors Assert-SheetIdentityV1 (response_exploiter_v2_campaign_v1):
    # pre-launch SHA verification of the governing sheet's live bytes before
    # anything else runs.
    if (-not (Test-Path -LiteralPath $script:Cycle3SheetPath -PathType Leaf)) {
        throw "cycle-3 governing sheet is missing: $script:Cycle3SheetPath"
    }
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $script:Cycle3SheetPath).Hash.ToLowerInvariant()
    if ($actual -cne $script:Cycle3SheetSha256) {
        throw "cycle-3 governing sheet SHA-256 changed: expected $script:Cycle3SheetSha256, got $actual; refusing to launch against a mutated sheet"
    }
    return [ordered]@{ path = $script:Cycle3SheetPath; sha256 = $actual }
}

function Assert-SearcherPoolAuthoritySheetIdentity {
    # Independent-reviewer finding (CLAUDE-REVIEWER-VERDICT-CYCLE3-LAUNCH-STACK-20260826.md,
    # "Additional findings", item 1): $script:SearcherPoolAuthoritySheetPath was
    # set but never verified, despite this file's own header citing that
    # sheet as co-governing. This closes that gap the same way
    # Assert-Cycle3SheetIdentity does for the cycle-3 sheet.
    if (-not (Test-Path -LiteralPath $script:SearcherPoolAuthoritySheetPath -PathType Leaf)) {
        throw "searcher-pool-authority governing sheet is missing: $script:SearcherPoolAuthoritySheetPath"
    }
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $script:SearcherPoolAuthoritySheetPath).Hash.ToLowerInvariant()
    if ($actual -cne $script:SearcherPoolAuthoritySheetSha256) {
        throw "searcher-pool-authority sheet SHA-256 changed: expected $script:SearcherPoolAuthoritySheetSha256, got $actual; refusing to launch against a mutated sheet"
    }
    return [ordered]@{ path = $script:SearcherPoolAuthoritySheetPath; sha256 = $actual }
}

function Get-ReleaseTestExecutableCycle3V1 {
    # Cycle-3's own exe resolution: same cargo-JSON-artifact mechanism as
    # Get-ReleaseTestExecutable, but built with BOTH
    # experimental-burn-net8-packed-cuda-v1 (the CudaBurnDense backend the
    # multirun harness requires) AND native-training-store-v2-production (the
    # real build-provenance/strict-source-tree admission this campaign uses,
    # unlike prior campaigns' plain release build) against the campaign's own
    # CARGO_TARGET_DIR. Requires a clean, committed worktree (the production
    # feature's own build-script admission gate); asserted explicitly here so
    # the failure is legible rather than a bare build-script panic.
    param(
        [Parameter(Mandatory = $true)][string]$EvidenceRoot,
        [Parameter(Mandatory = $true)][ValidatePattern('^[a-z0-9-]+$')][string]$Label
    )
    Push-Location $script:RepoRoot
    try {
        $dirty = (& git status --porcelain) -join "`n"
    }
    finally { Pop-Location }
    if (-not [string]::IsNullOrWhiteSpace($dirty)) {
        throw "worktree is not clean (native-training-store-v2-production requires a committed tree): `n$dirty"
    }

    New-Item -ItemType Directory -Force -Path $EvidenceRoot | Out-Null
    $jsonPath = Join-Path $EvidenceRoot "cargo-release-build-$Label.jsonl"
    $stderrPath = Join-Path $EvidenceRoot "cargo-release-build-$Label.stderr.log"
    $previousTargetDir = $env:CARGO_TARGET_DIR
    $env:CARGO_TARGET_DIR = $script:Cycle3CargoTargetDir
    Push-Location $script:RepoRoot
    try {
        $previous = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try {
            $jsonLines = @(& cargo test -p mtg-kernel --release --features experimental-burn-net8-packed-cuda-v1,native-training-store-v2-production --lib --no-run --message-format=json 2> $stderrPath)
            $cargoExit = $LASTEXITCODE
        }
        finally { $ErrorActionPreference = $previous }
        Assert-LastExitCode $cargoExit "cargo release build (cycle-3); see $stderrPath"
    }
    finally {
        Pop-Location
        $env:CARGO_TARGET_DIR = $previousTargetDir
    }
    $jsonLines | Set-Content -LiteralPath $jsonPath -Encoding utf8
    $executables = foreach ($line in $jsonLines) {
        try {
            $item = $line | ConvertFrom-Json
            if ($item.reason -eq 'compiler-artifact' -and $item.target.name -eq 'mtg_kernel' -and $item.target.kind -contains 'lib' -and $null -ne $item.executable) {
                [string]$item.executable
            }
        }
        catch {
        }
    }
    $executable = $executables | Select-Object -Last 1
    if ([string]::IsNullOrWhiteSpace($executable) -or -not (Test-Path -LiteralPath $executable)) {
        throw 'release mtg_kernel test executable was not resolved from Cargo JSON (cycle-3 build)'
    }
    return (Resolve-Path -LiteralPath $executable).Path
}

function Set-Cycle3NativeEnvironment {
    # Parallel sibling of Set-ScaledNativeEnvironment, scoped to cycle-3: sets
    # MULTIRUN_POPULATION_CYCLE3_AUTHORITY=1 (never MULTIRUN_POPULATION_AUTHORITY,
    # which stays tranche-1-locked per Amendment 2 A2.2) and always points
    # MULTIRUN_LADDER_INIT_STORE/GEN at the fixed cycle-3 parent (Section 2.3),
    # regardless of which of the 16 refreshes this launch is -- the contract's
    # parent_lineage binding never changes across the cycle (only the store
    # being trained, MULTIRUN_STORE_PARENT, grows).
    param(
        [Parameter(Mandatory = $true)][uint64]$Seed,
        [Parameter(Mandatory = $true)][uint64]$Updates,
        [Parameter(Mandatory = $true)][string]$StoreParent,
        [Parameter(Mandatory = $true)][ValidateSet(0, 1)][int]$GpuOrdinal,
        [Nullable[uint64]]$StopAfterGeneration,
        [Nullable[uint64]]$ExpectedResumeGeneration,
        [switch]$PopulationRuntime,
        [string]$RefreshChain = '',
        [string]$SlotRoots = '',
        [switch]$ResumeExistingStore
    )
    $storeExists = Test-Path -LiteralPath $StoreParent
    if ($ResumeExistingStore) {
        if (-not $storeExists) { throw "resume Store parent does not exist: $StoreParent" }
    }
    elseif ($storeExists) {
        throw "refusing to reuse Store parent: $StoreParent"
    }
    else {
        New-Item -ItemType Directory -Force -Path $StoreParent | Out-Null
    }

    $gpu = Assert-GpuIdentity -Ordinal $GpuOrdinal
    $saved = @{}
    foreach ($name in $script:EnvironmentNames) {
        $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
    }
    $saved['MULTIRUN_POPULATION_CYCLE3_AUTHORITY'] = [Environment]::GetEnvironmentVariable('MULTIRUN_POPULATION_CYCLE3_AUTHORITY', 'Process')
    $saved['MULTIRUN_POPULATION_V2_CYCLE3'] = [Environment]::GetEnvironmentVariable('MULTIRUN_POPULATION_V2_CYCLE3', 'Process')
    $values = @{
        MULTIRUN_RUNS = '1'; MULTIRUN_UPDATES = [string]$Updates
        MULTIRUN_WORKERS = '2'; MULTIRUN_SESSIONS = '32'; MULTIRUN_BROKER_TARGET = '16'
        MULTIRUN_RECORD_ONLY = '1'; MULTIRUN_BASE_SEED = [string]$Seed
        MULTIRUN_SEED_OFFSET = '0'; MULTIRUN_STORE_PARENT = $StoreParent
        MULTIRUN_LADDER = '1'
        MULTIRUN_LADDER_INIT_STORE = $script:Cycle3ParentStoreRoot
        MULTIRUN_LADDER_INIT_GEN = [string]$script:Cycle3ParentGeneration
        MULTIRUN_LADDER_POOL_DIR = $script:PoolRoot
        MULTIRUN_ENVIRONMENT_RANDOMIZATION_V2 = '1'; MULTIRUN_POLICY_ANCHOR_BETA = '0.1'
        MULTIRUN_STOP_AFTER_GENERATION = if ($null -eq $StopAfterGeneration) { $null } else { [string]$StopAfterGeneration }
        MULTIRUN_EXPECT_RESUME_GENERATION = if ($null -eq $ExpectedResumeGeneration) { $null } else { [string]$ExpectedResumeGeneration }
        MULTIRUN_POPULATION_AUTHORITY = '0'
        MULTIRUN_POPULATION_RUNTIME = if ($PopulationRuntime) { '1' } else { '0' }
        MULTIRUN_POPULATION_REFRESH_CHAIN = if ($PopulationRuntime) { $RefreshChain } else { $null }
        MULTIRUN_POPULATION_SLOT_ROOTS = if ($PopulationRuntime) { $SlotRoots } else { $null }
        MULTIRUN_RESPONSE_EXPLOITER_RUNTIME = '0'
        MULTIRUN_RESPONSE_EXPLOITER_REFRESH_CHAIN = $null
        MULTIRUN_RESPONSE_EXPLOITER_SLOT_ROOTS = $null
        MULTIRUN_RESPONSE_EXPLOITER_DENOVO = '0'
        MULTIRUN_WIDE = '0'; CUDA_DEVICE_ORDER = 'PCI_BUS_ID'
        CUDA_VISIBLE_DEVICES = $gpu.uuid; MTG_KERNEL_PILOT_CUDA_ORDINAL = '0'
    }
    foreach ($name in $script:EnvironmentNames) {
        [Environment]::SetEnvironmentVariable($name, $values[$name], 'Process')
    }
    [Environment]::SetEnvironmentVariable('MULTIRUN_POPULATION_CYCLE3_AUTHORITY', '1', 'Process')
    # Amendment 4 (countersigned 00affe6a), A4.3: the pool-RESOLUTION
    # dispatch knob (native_science_loop_v1.rs's population_v2_cycle3_active_dispatch),
    # distinct from MULTIRUN_POPULATION_CYCLE3_AUTHORITY above (record
    # CONSTRUCTION). Set only when population resolution is actually
    # requested this launch; cleared (not just '0') otherwise, matching
    # how $PopulationRuntime's own MULTIRUN_POPULATION_RUNTIME/_REFRESH_CHAIN/
    # _SLOT_ROOTS trio already behaves for the false case.
    [Environment]::SetEnvironmentVariable(
        'MULTIRUN_POPULATION_V2_CYCLE3',
        $(if ($PopulationRuntime) { '1' } else { $null }),
        'Process'
    )
    return $saved
}

function Restore-Cycle3NativeEnvironment {
    param([Parameter(Mandatory = $true)][hashtable]$Saved)
    Restore-NativeEnvironment -Saved $Saved
    [Environment]::SetEnvironmentVariable('MULTIRUN_POPULATION_CYCLE3_AUTHORITY', $Saved['MULTIRUN_POPULATION_CYCLE3_AUTHORITY'], 'Process')
    [Environment]::SetEnvironmentVariable('MULTIRUN_POPULATION_V2_CYCLE3', $Saved['MULTIRUN_POPULATION_V2_CYCLE3'], 'Process')
}

function Assert-WarmStartGenZeroCycle3V1 {
    # Amendment 2 A2.1 Layer 2 (launcher, every process start): before
    # starting or restarting the cycle-3 training process, assert the
    # store's own generation-0 model_parameter_sha256 equals the pinned
    # parent value. Hard-stop (kill the process, throw) on mismatch. Single-
    # store/single-process shape (Section 5.2: cycle-3 trains one lineage on
    # one GPU directly), unlike Assert-WarmStartGenZeroV2's $Lanes-array
    # shape for the response-exploiter build's multiple concurrent seeds.
    param(
        [Parameter(Mandatory = $true)][string]$StoreParent,
        [System.Diagnostics.Process]$Process
    )
    $checkpoint = Join-Path $StoreParent 'run-0\store\checkpoints\update-00000000.checkpoint.json'
    $deadline = [DateTimeOffset]::UtcNow.AddSeconds(1800)
    while (-not (Test-Path -LiteralPath $checkpoint)) {
        if ($null -ne $Process -and $Process.HasExited) {
            throw "Warm-start assertion: cycle-3 process exited before writing its generation-0 checkpoint"
        }
        if ([DateTimeOffset]::UtcNow -gt $deadline) {
            throw "Warm-start assertion: generation-0 checkpoint absent after 1800s at $checkpoint"
        }
        Start-Sleep -Seconds 5
    }
    $record = $null
    foreach ($attempt in 1..3) {
        try { $record = Get-Content -LiteralPath $checkpoint -Raw | ConvertFrom-Json; break }
        catch { Start-Sleep -Seconds 3 }
    }
    if ($null -eq $record) { throw "Warm-start assertion: generation-0 checkpoint unreadable at $checkpoint" }
    $actual = [string]$record.train_state.model_parameter_sha256
    if ($actual -cne $script:Cycle3ParentModelParameterSha256) {
        if ($null -ne $Process -and -not $Process.HasExited) {
            & taskkill /PID $Process.Id /T /F | Out-Null
        }
        throw "Warm-start assertion FAILED: generation-0 model_parameter_sha256 $actual does not equal the pinned cycle-3 parent $($script:Cycle3ParentModelParameterSha256); lane stopped before further budget spend"
    }
    return [ordered]@{ store_parent = $StoreParent; checkpoint = $checkpoint; model_parameter_sha256 = $actual }
}

function Assert-ResumePositionMatchesStoreCycle3V1 {
    # Amendment 7 (collab/CLAUDE-POPULATION-V2-CYCLE3-SHEET-V1.md), A7.2
    # item 4: a launcher-side (Layer 2) mirror of the in-code
    # store-position check -- the existing, unmodified resume-generation
    # guard Amendment 7 cites rather than duplicates
    # (native_science_loop_v1.rs:814-843, the Complete/Continue match
    # arms' resume_generation_checked logic). Reads the store's own
    # on-disk latest.json generation_index and hard-stops BEFORE the
    # process even starts if it disagrees with the caller-supplied resume
    # point, avoiding exactly the doomed-launch cost the 2026-08-26
    # refresh-19 re-run already paid once for real (~20 minutes burned on
    # an InputInvalid discovered only after the process was already
    # running; see refresh1-searcher-smoke.ps1's own FIX comment
    # immediately above its Item4Only phase gate). Same "two layers, same
    # facts, independent" discipline as Assert-WarmStartGenZeroCycle3V1
    # above (Amendment 2 A2.1), but checked before dispatch rather than
    # after.
    param(
        [Parameter(Mandatory = $true)][string]$StoreParent,
        [Parameter(Mandatory = $true)][uint64]$ExpectedResumeGeneration
    )
    $latestPath = Join-Path $StoreParent 'run-0\store\latest.json'
    if (-not (Test-Path -LiteralPath $latestPath)) {
        throw "Amendment 7 A7.2 item 4 assertion: store latest.json not found at $latestPath -- this script always resumes an existing store (-ResumeExistingStore); a missing latest.json means the store is not yet in the state this launch expects"
    }
    $latest = $null
    foreach ($attempt in 1..3) {
        try { $latest = Get-Content -LiteralPath $latestPath -Raw | ConvertFrom-Json; break }
        catch { Start-Sleep -Seconds 3 }
    }
    if ($null -eq $latest) { throw "Amendment 7 A7.2 item 4 assertion: latest.json unreadable at $latestPath" }
    $actual = [uint64]$latest.generation_index
    if ($actual -ne $ExpectedResumeGeneration) {
        throw "Amendment 7 A7.2 item 4 assertion FAILED: store's actual generation_index ($actual) at $latestPath does not equal the caller-supplied resume point ($ExpectedResumeGeneration) -- hard-stopping before starting the process"
    }
    return [ordered]@{ store_parent = $StoreParent; latest_path = $latestPath; generation_index = $actual }
}
