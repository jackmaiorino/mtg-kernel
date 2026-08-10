# Multirun operating-point launcher v1: shared validation, rendering, and
# launch machinery. Contract: THROUGHPUT-MULTIRUN-OPERATING-POINT-DESIGN-V1.md
# (collab). Zero Rust changes; consumes the pilot at main f986378 as-is.
# Render-only mode is the cold-gate surface and must stay side-effect free.

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:SchemaId = 'multirun-operating-point-launch/v1'

# Allocation-failure signature, carried EXACTLY from the certified fair
# campaign's SHA-pinned runner copy ($AllocationFailurePattern,
# 00-runner-starting-copy.ps1:109-114). A cold gate re-extracts the pattern
# from the pinned copy and asserts equality, so drift is impossible.
$script:AllocationFailurePatternV1 =
    '(?i)(can''t allocate|cannot allocate|failed to allocate|' +
    'allocation (?:has )?failed|memory allocation.*fail|' +
    'out of (?:device )?memory|\bOOM\b|CUDA_ERROR_OUT_OF_MEMORY|' +
    'CUDA error.*(?:alloc|memory)|CUDNN_STATUS_ALLOC_FAILED|' +
    'CUBLAS_STATUS_ALLOC_FAILED|cuMemAlloc|insufficient (?:device )?memory)'
$script:RunnerTest = 'native_science_loop_v1::windows_science_loop_tests::multirun_pilot_v1'
$script:RunnerArgs = @($script:RunnerTest, '--ignored', '--exact', '--nocapture', '--test-threads=1')

# The eight config-value-carrying environment names, in render order.
$script:ValueNames = @(
    'MULTIRUN_RUNS', 'MULTIRUN_UPDATES', 'MULTIRUN_WORKERS',
    'MULTIRUN_SESSIONS', 'MULTIRUN_BROKER_TARGET', 'MULTIRUN_BASE_SEED',
    'MULTIRUN_SEED_OFFSET', 'MULTIRUN_STORE_PARENT'
)

# Every remaining MULTIRUN_* name the pilot reads at f986378, plus
# MULTIRUN_EXECUTION_MODE (no pilot caller yet; cleared = sync by contract).
# All are ALWAYS cleared: v1 cannot express these modes.
$script:ClearedNames = @(
    'MULTIRUN_RECORD_ONLY', 'MULTIRUN_STOP_AFTER_GENERATION',
    'MULTIRUN_EXPECT_RESUME_GENERATION', 'MULTIRUN_POLICY_ANCHOR_BETA',
    'MULTIRUN_ENVIRONMENT_RANDOMIZATION_V2', 'MULTIRUN_WIDE',
    'MULTIRUN_LADDER', 'MULTIRUN_LADDER_INIT_STORE',
    'MULTIRUN_LADDER_INIT_GEN', 'MULTIRUN_LADDER_POOL_DIR',
    'MULTIRUN_POPULATION_AUTHORITY', 'MULTIRUN_POPULATION_RUNTIME',
    'MULTIRUN_POPULATION_REFRESH_CHAIN', 'MULTIRUN_POPULATION_SLOT_ROOTS',
    'MULTIRUN_RESPONSE_EXPLOITER_RUNTIME', 'MULTIRUN_RESPONSE_EXPLOITER_DENOVO',
    'MULTIRUN_RESPONSE_EXPLOITER_REFRESH_CHAIN',
    'MULTIRUN_RESPONSE_EXPLOITER_SLOT_ROOTS', 'MULTIRUN_EXECUTION_MODE'
)

# Placement names: set per process. Ordinal is ALWAYS 0: CUDA_VISIBLE_DEVICES
# masking makes the pinned UUID the only visible device, and the bridge's
# ordinal parse is lenient (bridge.rs:418-425), so placement never rides on it.
$script:PlacementNames = @('CUDA_DEVICE_ORDER', 'CUDA_VISIBLE_DEVICES', 'MTG_KERNEL_PILOT_CUDA_ORDINAL')

function Get-FullWhitelist {
    return @($script:ValueNames + $script:ClearedNames + $script:PlacementNames)
}

function Assert-ExactKeySet {
    param(
        [Parameter(Mandatory = $true)]$Object,
        [Parameter(Mandatory = $true)][string[]]$Expected,
        [Parameter(Mandatory = $true)][string]$Where
    )
    # Case-sensitive both directions: PS default matching is case-insensitive
    # and would let 'Schema' or 'PROCESSES' slip the deny-unknown net.
    $actual = @($Object.PSObject.Properties.Name)
    foreach ($key in $actual) {
        if ($Expected -cnotcontains $key) { throw "unknown key '$key' in $Where (deny-unknown)" }
    }
    foreach ($key in $Expected) {
        if ($actual -cnotcontains $key) { throw "missing key '$key' in $Where (deny-missing)" }
    }
}

function Assert-PositiveInteger {
    param([Parameter(Mandatory = $true)]$Value, [Parameter(Mandatory = $true)][string]$Name)
    if ($Value -isnot [long] -and $Value -isnot [int]) { throw "$Name must be a JSON integer, got '$Value'" }
    if ($Value -lt 1) { throw "$Name must be >= 1, got $Value" }
}

function Read-LaunchConfig {
    param([Parameter(Mandatory = $true)][string]$ConfigPath)
    if (-not (Test-Path -LiteralPath $ConfigPath)) { throw "config not found: $ConfigPath" }
    $raw = Get-Content -LiteralPath $ConfigPath -Raw
    $config = $raw | ConvertFrom-Json

    Assert-ExactKeySet -Object $config -Where 'config root' -Expected @(
        'schema', 'label', 'expected_commit', 'base_seed', 'updates', 'workers',
        'sessions', 'broker_target', 'store_parent_root', 'monitor', 'processes'
    )
    if ($config.schema -cne $script:SchemaId) { throw "schema must be '$($script:SchemaId)', got '$($config.schema)'" }
    if ($config.label -cnotmatch '^[a-z0-9][a-z0-9-]{2,63}$') { throw "label must be lowercase kebab-case 3-64 chars, got '$($config.label)'" }
    if ($config.expected_commit -cnotmatch '^[0-9a-f]{40}$') { throw 'expected_commit must be a full lowercase 40-hex sha' }
    Assert-PositiveInteger -Value $config.base_seed -Name 'base_seed'
    Assert-PositiveInteger -Value $config.updates -Name 'updates'
    Assert-PositiveInteger -Value $config.workers -Name 'workers'
    Assert-PositiveInteger -Value $config.sessions -Name 'sessions'
    Assert-PositiveInteger -Value $config.broker_target -Name 'broker_target'
    # Bounds mirror validate_topology_v2 (native_training_store_run_v2.rs:2791-2806).
    if ($config.workers -gt 16) { throw "workers must be <= 16, got $($config.workers)" }
    if ($config.sessions -gt 64) { throw "sessions must be <= 64, got $($config.sessions)" }
    $actors = $config.workers * $config.sessions
    if ($config.broker_target -gt $actors) { throw "broker_target must be <= workers*sessions ($actors), got $($config.broker_target)" }
    # v1 contract constraint: the pilot's printed episode accounting hardcodes
    # 64 episodes per generation (native_science_loop_v1.rs:2007-2008); outside
    # actor count 64 the printed totals are silently wrong. All certified
    # operating points are 2/32 at K=64.
    if ($actors -ne 64) { throw "v1 requires workers*sessions == 64 (pilot episode accounting contract), got $actors" }
    if ([string]::IsNullOrWhiteSpace($config.store_parent_root)) { throw 'store_parent_root must be non-empty' }
    # IsPathRooted accepts drive-relative 'D:foo'; require a full drive path.
    if ($config.store_parent_root -cnotmatch '^[A-Za-z]:\\') { throw "store_parent_root must be a full drive-absolute path, got '$($config.store_parent_root)'" }

    $processes = @($config.processes)
    if ($processes.Count -lt 1) { throw 'processes must have at least one entry' }
    $seenUuids = @{}
    $totalRuns = [long]0
    foreach ($process in $processes) {
        Assert-ExactKeySet -Object $process -Where 'process entry' -Expected @('device_uuid', 'runs')
        if ($process.device_uuid -cnotmatch '^GPU-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$') { throw "device_uuid must be a full lowercase GPU-<uuid>, got '$($process.device_uuid)'" }
        if ($seenUuids.ContainsKey($process.device_uuid)) { throw "duplicate device_uuid '$($process.device_uuid)'" }
        $seenUuids[$process.device_uuid] = $true
        Assert-PositiveInteger -Value $process.runs -Name 'process runs'
        $totalRuns += [long]$process.runs
    }
    # Seed arithmetic stays exact everywhere it travels: run seed = base +
    # offset + ordinal. Bound base_seed to 2^53 so the value survives every
    # JSON round-trip (PS 5.1 may surface large numbers as doubles) and the
    # sum can never approach Int64/u64 edges.
    if ([long]$config.base_seed -gt 9007199254740992) { throw 'base_seed must be <= 2^53 (JSON-exact, overflow-safe band)' }

    # Monitor block: census cadence, hard wall-clock deadline, and one memory
    # rail per configured device. Rails are pinned IN the config so the leg
    # files really do carry their abort criteria.
    Assert-ExactKeySet -Object $config.monitor -Where 'monitor' -Expected @(
        'census_seconds', 'wall_clock_timeout_seconds', 'device_rails'
    )
    Assert-PositiveInteger -Value $config.monitor.census_seconds -Name 'monitor.census_seconds'
    if ($config.monitor.census_seconds -gt 60) { throw 'monitor.census_seconds must be <= 60' }
    Assert-PositiveInteger -Value $config.monitor.wall_clock_timeout_seconds -Name 'monitor.wall_clock_timeout_seconds'
    if ($config.monitor.wall_clock_timeout_seconds -lt 60 -or $config.monitor.wall_clock_timeout_seconds -gt 86400) { throw 'monitor.wall_clock_timeout_seconds must be in 60..86400' }
    $railUuids = @{}
    foreach ($rail in @($config.monitor.device_rails)) {
        Assert-ExactKeySet -Object $rail -Where 'device rail' -Expected @('device_uuid', 'rail_mib')
        if ($rail.device_uuid -cnotmatch '^GPU-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$') { throw "rail device_uuid must be a full lowercase GPU-<uuid>, got '$($rail.device_uuid)'" }
        Assert-PositiveInteger -Value $rail.rail_mib -Name 'rail_mib'
        if ($railUuids.ContainsKey($rail.device_uuid)) { throw "duplicate rail for '$($rail.device_uuid)'" }
        $railUuids[$rail.device_uuid] = $rail.rail_mib
    }
    foreach ($process in $processes) {
        if (-not $railUuids.ContainsKey($process.device_uuid)) { throw "no memory rail configured for device '$($process.device_uuid)'" }
    }
    foreach ($uuid in $railUuids.Keys) {
        if (-not $seenUuids.ContainsKey($uuid)) { throw "rail configured for unused device '$uuid'" }
    }
    return $config
}

function Get-RenderedLaunch {
    # Pure function of the validated config: derived seed offsets (cumulative
    # runs, the scheme S1 ran in production shape), derived store parents,
    # full-whitelist env maps with every name explicitly set or null (null =
    # the launcher explicitly clears it in the child environment).
    param([Parameter(Mandatory = $true)]$Config)
    $rendered = [ordered]@{
        schema_id  = $script:SchemaId
        label      = $Config.label
        expected_commit = $Config.expected_commit
        runner_test = $script:RunnerTest
        runner_args = @($script:RunnerArgs)
        whitelist  = @(Get-FullWhitelist)
        monitor    = [pscustomobject][ordered]@{
            census_seconds = $Config.monitor.census_seconds
            wall_clock_timeout_seconds = $Config.monitor.wall_clock_timeout_seconds
            device_rails = @($Config.monitor.device_rails | ForEach-Object {
                [pscustomobject][ordered]@{ device_uuid = $_.device_uuid; rail_mib = $_.rail_mib }
            })
        }
        processes  = @()
    }
    $offset = [long]0
    $storeParents = @{}
    $index = 0
    foreach ($process in @($Config.processes)) {
        $storeParent = Join-Path $Config.store_parent_root ("proc-{0}" -f $index)
        if ($storeParents.ContainsKey($storeParent)) { throw "derived store parent collision: $storeParent" }
        $storeParents[$storeParent] = $true
        $env = [ordered]@{}
        foreach ($name in Get-FullWhitelist) { $env[$name] = $null }
        $env['MULTIRUN_RUNS'] = [string]$process.runs
        $env['MULTIRUN_UPDATES'] = [string]$Config.updates
        $env['MULTIRUN_WORKERS'] = [string]$Config.workers
        $env['MULTIRUN_SESSIONS'] = [string]$Config.sessions
        $env['MULTIRUN_BROKER_TARGET'] = [string]$Config.broker_target
        $env['MULTIRUN_BASE_SEED'] = [string]$Config.base_seed
        $env['MULTIRUN_SEED_OFFSET'] = [string]$offset
        $env['MULTIRUN_STORE_PARENT'] = $storeParent
        $env['CUDA_DEVICE_ORDER'] = 'PCI_BUS_ID'
        $env['CUDA_VISIBLE_DEVICES'] = $process.device_uuid
        $env['MTG_KERNEL_PILOT_CUDA_ORDINAL'] = '0'
        $rendered.processes += [pscustomobject][ordered]@{
            process_index = $index
            device_uuid   = $process.device_uuid
            runs          = $process.runs
            seed_offset   = $offset
            store_parent  = $storeParent
            env           = [pscustomobject]$env
        }
        $offset += [long]$process.runs
        $index += 1
    }
    # Global run-seed collision check: seeds are base + offset + ordinal;
    # cumulative offsets make them disjoint by construction, asserted anyway.
    $allSeeds = @{}
    foreach ($process in $rendered.processes) {
        for ($ordinal = 0; $ordinal -lt $process.runs; $ordinal++) {
            $seed = [long]$Config.base_seed + $process.seed_offset + $ordinal
            if ($allSeeds.ContainsKey($seed)) { throw "run seed collision at $seed" }
            $allSeeds[$seed] = $true
        }
    }
    return [pscustomobject]$rendered
}

function Get-Sha256Hex {
    param([Parameter(Mandatory = $true)][string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Resolve-PilotExecutable {
    # House rule: executables come from cargo JSON artifacts only, never a
    # guessed filename. Builds (or reuses) the release lib-test binary.
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$StderrPath,
        [int]$Jobs = 0
    )
    Push-Location (Join-Path $RepoRoot 'mtg-kernel')
    try {
        $previous = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try {
            $jobArgs = @()
            if ($Jobs -gt 0) { $jobArgs = @('--jobs', [string]$Jobs) }
            $jsonLines = @(& cargo test -p mtg-kernel --release --features experimental-burn-net8-packed-cuda-v1 --lib --no-run --message-format=json @jobArgs 2> $StderrPath)
            $cargoExit = $LASTEXITCODE
        }
        finally { $ErrorActionPreference = $previous }
        if ($cargoExit -ne 0) { throw "cargo build failed (exit $cargoExit); see $StderrPath" }
    }
    finally { Pop-Location }
    $executables = foreach ($line in $jsonLines) {
        try { $item = $line | ConvertFrom-Json } catch { continue }
        if ($null -eq $item) { continue }
        $names = @($item.PSObject.Properties.Name)
        if ($names -cnotcontains 'executable' -or $names -cnotcontains 'target' -or $names -cnotcontains 'profile') { continue }
        # target.kind filtering per the stale-binary house rule: accept only
        # the crate's own lib target compiled as a test harness.
        if (-not $item.executable) { continue }
        # Cargo lib targets carry the CRATE name: mtg_kernel, underscore,
        # not the package's hyphenated name (verified against live artifact
        # JSON; the hyphenated filter matched nothing).
        if ($item.target.name -cne 'mtg_kernel') { continue }
        if (@($item.target.kind) -cnotcontains 'lib') { continue }
        if (-not $item.profile.test) { continue }
        $item.executable
    }
    $executables = @($executables | Select-Object -Unique)
    if ($executables.Count -ne 1) { throw "expected exactly one lib-test executable from cargo JSON, got $($executables.Count)" }
    $executable = $executables[0]
    # Positive execution marker: the resolved binary must itself report that
    # it carries the pilot test. --list never runs tests and touches no GPU.
    # EAP guard: a redirected native stderr line is a terminating exception
    # under Stop in PS 5.1 (same class as the taskkill fix).
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $listing = & $executable $script:RunnerTest --list --exact 2>&1 | Out-String
        $listExit = $LASTEXITCODE
    }
    finally { $ErrorActionPreference = $previous }
    if ($listExit -ne 0) { throw "execution-marker listing failed (exit $listExit)" }
    if ($listing -cnotmatch [regex]::Escape($script:RunnerTest)) { throw "resolved executable does not carry $($script:RunnerTest)" }
    return $executable
}

function Invoke-BoundedNvidiaSmi {
    # Bounded nvidia-smi invocation: a hung driver query must never disable
    # the deadline or the rail abort. Returns output lines, or $null on any
    # failure or timeout (the caller counts those fail-closed).
    param([Parameter(Mandatory = $true)][string]$ArgumentString, [int]$TimeoutMs = 10000)
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = 'nvidia-smi'
    $psi.Arguments = $ArgumentString
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    try {
        $query = [System.Diagnostics.Process]::Start($psi)
        $stdoutTask = $query.StandardOutput.ReadToEndAsync()
        [void]$query.StandardError.ReadToEndAsync()
        if (-not $query.WaitForExit($TimeoutMs)) {
            try { $query.Kill() } catch {}
            return $null
        }
        if ($query.ExitCode -ne 0) { return $null }
        return @($stdoutTask.Result -split "`r?`n" | Where-Object { $_ -ne '' })
    }
    catch { return $null }
}

function Assert-DeviceInventory {
    # Launch-mode only: every configured UUID must exist on the host.
    param([Parameter(Mandatory = $true)]$Rendered)
    $inventory = Invoke-BoundedNvidiaSmi -ArgumentString '--query-gpu=uuid,name,memory.total --format=csv,noheader'
    if ($null -eq $inventory) { throw 'nvidia-smi inventory query failed or timed out' }
    $uuids = @($inventory | ForEach-Object { ($_ -split ',')[0].Trim() })
    foreach ($process in $Rendered.processes) {
        if ($uuids -cnotcontains $process.device_uuid) { throw "device_uuid '$($process.device_uuid)' not present in live inventory" }
    }
    return $inventory
}

function Start-RenderedLaunch {
    # Spawns one child process per entry with an explicitly constructed
    # environment: whitelist names set from the render map or REMOVED when
    # null, everything else inherited. Writes exactly one launch record into
    # a fresh root, executes the hashed exe copy (TOCTOU closure).
    param(
        [Parameter(Mandatory = $true)]$Rendered,
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$LaunchRoot,
        [Parameter(Mandatory = $true)][string]$ConfigPath
    )
    if (Test-Path -LiteralPath $LaunchRoot) { throw "launch root must be fresh, exists: $LaunchRoot" }
    foreach ($process in $Rendered.processes) {
        if (Test-Path -LiteralPath $process.store_parent) { throw "store parent must be fresh, exists: $($process.store_parent)" }
    }
    $head = (& git -C $RepoRoot rev-parse HEAD).Trim()
    if ($head -cne $Rendered.expected_commit) { throw "repo HEAD $head does not match expected_commit $($Rendered.expected_commit)" }
    $dirty = (& git -C $RepoRoot status --porcelain) | Where-Object { $_ -cnotmatch '^\?\? scripts/orchestration/' }
    if ($dirty) { throw 'repo tree is not clean (tracked changes present)' }

    # Everything above this line mutates nothing on disk; a throw there
    # leaves a clean slate for retry. From root creation onward, EVERY exit
    # path writes a record into the root: pre-flight failures write a
    # FAILED-PREFLIGHT record, everything later goes through the main
    # try/catch/finally below.
    New-Item -ItemType Directory -Force -Path $LaunchRoot | Out-Null
    $recordPath = Join-Path $LaunchRoot 'launch-record.json'
    $startedUtc = [DateTime]::UtcNow.ToString('o')
    $ambientMib = [ordered]@{}
    try {
        $inventory = Assert-DeviceInventory -Rendered $Rendered
        # Ambient-conditions rule (orchestrator directive, lane log
        # 2026-08-10 ~14:05): census both devices BEFORE spawning children,
        # record the readings, and hold the leg if ambient exceeds the
        # certified basis plus allowance. Certified ambient basis per the
        # Amendment 5 rail derivation: dev0 1,861 MiB, dev1 74 MiB;
        # allowances 500 / 200 MiB per the directive.
        $ambientLimits = @{
            'GPU-3502709e-6aef-8ed7-4abe-562838793e3d' = [long]2361
            'GPU-0642d3ca-e3d4-ba16-96ab-c561c6da90e3' = [long]274
        }
        $ambientSample = Invoke-BoundedNvidiaSmi -ArgumentString '--query-gpu=uuid,memory.used --format=csv,noheader,nounits'
        if ($null -eq $ambientSample) { throw 'ambient census failed or timed out' }
        foreach ($row in $ambientSample) {
            $parts = $row -split ','
            if ($parts.Count -lt 2) { continue }
            $uuid = $parts[0].Trim()
            if (@($Rendered.processes | Where-Object { $_.device_uuid -ceq $uuid }).Count -eq 0) { continue }
            $ambientMib[$uuid] = [long]($parts[1].Trim())
        }
        foreach ($process in $Rendered.processes) {
            if (-not $ambientMib.Contains($process.device_uuid)) { throw "ambient census returned no reading for $($process.device_uuid)" }
            if ($ambientLimits.ContainsKey($process.device_uuid) -and $ambientMib[$process.device_uuid] -gt $ambientLimits[$process.device_uuid]) {
                throw "HELD-AMBIENT: $($process.device_uuid) ambient $($ambientMib[$process.device_uuid]) MiB exceeds the ruled limit $($ambientLimits[$process.device_uuid]) MiB; leg held for orchestrator call"
            }
        }
        $sourceExe = Resolve-PilotExecutable -RepoRoot $RepoRoot -StderrPath (Join-Path $LaunchRoot 'cargo-build.stderr.log')
        # Post-build re-verify: the build takes minutes; the tree must not
        # have moved under it, or the recorded source_commit would be a lie.
        $headAfterBuild = (& git -C $RepoRoot rev-parse HEAD).Trim()
        if ($headAfterBuild -cne $head) { throw "repo HEAD moved during build ($head -> $headAfterBuild)" }
        $exeCopy = Join-Path $LaunchRoot ([System.IO.Path]::GetFileName($sourceExe))
        Copy-Item -LiteralPath $sourceExe -Destination $exeCopy
        $exeSha = Get-Sha256Hex -Path $exeCopy
    }
    catch {
        try {
            $preflightDisposition = if ($_.Exception.Message -clike 'HELD-AMBIENT*') { 'HELD-AMBIENT' } else { 'FAILED-PREFLIGHT' }
            $preflightRecord = [ordered]@{
                schema        = 'multirun-operating-point-launch-record/v1'
                label         = $Rendered.label
                config_path   = (Resolve-Path -LiteralPath $ConfigPath).Path
                config_sha256 = Get-Sha256Hex -Path $ConfigPath
                source_commit = $head
                disposition   = $preflightDisposition
                preflight_error = $_.Exception.Message
                ambient_mib   = $ambientMib
                started_utc   = $startedUtc
                ended_utc     = [DateTime]::UtcNow.ToString('o')
                processes     = @()
            }
            ($preflightRecord | ConvertTo-Json -Depth 6) | Out-File -LiteralPath $recordPath -Encoding utf8
        }
        catch {
            try { Set-Content -LiteralPath (Join-Path $LaunchRoot 'launch-record-write-failure.txt') -Encoding utf8 -Value $_.Exception.Message } catch {}
        }
        throw
    }

    $record = [ordered]@{
        schema         = 'multirun-operating-point-launch-record/v1'
        label          = $Rendered.label
        config_path    = (Resolve-Path -LiteralPath $ConfigPath).Path
        config_sha256  = Get-Sha256Hex -Path $ConfigPath
        config_bytes_base64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes($ConfigPath))
        source_commit  = $head
        executable     = $exeCopy
        executable_sha256 = $exeSha
        device_inventory = @($inventory)
        ambient_mib    = $ambientMib
        monitor        = $Rendered.monitor
        disposition    = 'RUNNING'
        rail_breaches  = @()
        started_utc    = $startedUtc
        processes      = @()
    }

    $children = @()
    $launchDisposition = 'RUNNING'
    $censusPath = Join-Path $LaunchRoot 'census.csv'
    $maxByUuid = @{}
    $censusCount = 0

    # Kill with PID-reuse protection: only a process whose StartTime ticks
    # still match the child we spawned is ours to kill (house watchdog rule).
    # Per-child try/catch: one refusing child must never shield the others
    # from the kill (reviewer-reproduced: a redirected native stderr line is
    # a terminating RemoteException under EAP=Stop in PS 5.1). taskkill exit
    # is inspected, failure escalates to Stop-Process -Force, and the
    # post-kill wait is bounded; the outcome is recorded per child.
    function Stop-OwnedChildren {
        param($Children, [string]$Disposition)
        function Add-KillOutcome {
            param($Child, [string]$Entry)
            # Append-only: a later failure must never erase an earlier
            # recorded outcome.
            if ([string]::IsNullOrEmpty($Child.kill_outcome)) { $Child.kill_outcome = $Entry }
            else { $Child.kill_outcome = "$($Child.kill_outcome); $Entry" }
        }
        foreach ($child in @($Children)) {
            try {
                if ($child.process.HasExited) {
                    # A self-exited child is collected, never skipped: on an
                    # outer-catch invocation the collection loop does not
                    # run, and the record must not read RUNNING/null for a
                    # process that already finished.
                    try {
                        if ($null -eq $child.exit_code) { $child.exit_code = $child.process.ExitCode }
                        if ($null -eq $child.ended) { $child.ended = [DateTime]::UtcNow.ToString('o') }
                        if ($child.disposition -ceq 'RUNNING') {
                            $child.disposition = if ($child.exit_code -eq 0) { 'COMPLETED' } else { 'FAILED' }
                        }
                    }
                    catch {}
                    continue
                }
                $live = Get-Process -Id $child.process.Id -ErrorAction SilentlyContinue
                # Identity read guarded: StartTime can throw on a racing
                # exit; an unverifiable identity is never killed (PID-reuse
                # rule) but the reason is recorded.
                $liveTicks = $null
                try { if ($null -ne $live) { $liveTicks = $live.StartTime.ToUniversalTime().Ticks } } catch {}
                if ($null -eq $liveTicks -or $liveTicks -ne $child.start_ticks) {
                    if ($null -eq $live) { continue }
                    Add-KillOutcome $child 'identity-unverifiable; not killed'
                    continue
                }
                # Disposition is set the moment this child is selected for a
                # kill, before any step that can throw, so a later failure
                # can never leave it reading RUNNING.
                $child.disposition = $Disposition
                $previous = $ErrorActionPreference
                $ErrorActionPreference = 'Continue'
                try { & taskkill /PID $child.process.Id /T /F 2>&1 | Out-Null; $killExit = $LASTEXITCODE }
                finally { $ErrorActionPreference = $previous }
                if ($killExit -ne 0) {
                    try { Stop-Process -Id $child.process.Id -Force -ErrorAction Stop; Add-KillOutcome $child 'stop-process-escalation' }
                    catch { Add-KillOutcome $child "kill-failed: taskkill exit $killExit; Stop-Process: $($_.Exception.Message)" }
                }
                else { Add-KillOutcome $child 'taskkill' }
                if (-not $child.process.WaitForExit(15000)) { Add-KillOutcome $child 'wait-timeout' }
            }
            catch { try { Add-KillOutcome $child "kill-error: $($_.Exception.Message)" } catch {} }
        }
    }

    try {
        foreach ($process in $Rendered.processes) {
            $logPath = Join-Path $LaunchRoot ("proc-{0}.log" -f $process.process_index)
            $psi = New-Object System.Diagnostics.ProcessStartInfo
            $psi.FileName = $exeCopy
            # Windows PowerShell 5.1 / .NET Framework: no ArgumentList; every
            # runner arg is space-free so a plain join is unambiguous.
            $psi.Arguments = ($Rendered.runner_args -join ' ')
            $psi.UseShellExecute = $false
            $psi.RedirectStandardOutput = $true
            $psi.RedirectStandardError = $true
            foreach ($name in $Rendered.whitelist) {
                $value = $process.env.$name
                if ($psi.EnvironmentVariables.ContainsKey($name)) { $psi.EnvironmentVariables.Remove($name) }
                if ($null -ne $value) { $psi.EnvironmentVariables[$name] = $value }
            }
            $child = [System.Diagnostics.Process]::Start($psi)
            $children += [pscustomobject]@{
                process = $child; log = $logPath
                stdout = $child.StandardOutput.ReadToEndAsync()
                stderr = $child.StandardError.ReadToEndAsync()
                index = $process.process_index
                device_uuid = $process.device_uuid
                seed_offset = $process.seed_offset
                store_parent = $process.store_parent
                env_map = $process.env
                started = [DateTime]::UtcNow.ToString('o')
                start_ticks = $child.StartTime.ToUniversalTime().Ticks
                disposition = 'RUNNING'
                kill_outcome = $null
                exit_code = $null
                oom_signature_seen = $null
                ended = $null
                collect_error = $null
            }
        }

        # Monitor loop: bounded census of both rails at the configured
        # cadence, wall-clock deadline, fail-closed lost-census counter.
        Set-Content -LiteralPath $censusPath -Encoding utf8 -Value 'utc,device_uuid,memory_used_mib'
        $deadline = [DateTime]::UtcNow.AddSeconds($Rendered.monitor.wall_clock_timeout_seconds)
        $railByUuid = @{}
        foreach ($rail in $Rendered.monitor.device_rails) { $railByUuid[$rail.device_uuid] = [long]$rail.rail_mib }
        $lostCensus = 0
        $launchDisposition = 'COMPLETED'
        while (@($children | Where-Object { -not $_.process.HasExited }).Count -gt 0) {
            if ([DateTime]::UtcNow -gt $deadline) {
                Stop-OwnedChildren -Children $children -Disposition 'ABORTED-TIMEOUT'
                $launchDisposition = 'ABORTED-TIMEOUT'
                break
            }
            # Per-iteration try/catch: a single census failure must degrade
            # to the lost-census counter, never abort monitoring silently.
            $sampleParsed = $false
            try {
                $sample = Invoke-BoundedNvidiaSmi -ArgumentString '--query-gpu=uuid,memory.used --format=csv,noheader,nounits'
                $censusCount += 1
                if ($null -ne $sample) {
                    $nowIso = [DateTime]::UtcNow.ToString('o')
                    foreach ($row in $sample) {
                        $parts = $row -split ','
                        if ($parts.Count -lt 2) { continue }
                        $uuid = $parts[0].Trim(); $used = [long]($parts[1].Trim())
                        if (-not $railByUuid.ContainsKey($uuid)) { continue }
                        $sampleParsed = $true
                        Add-Content -LiteralPath $censusPath -Encoding utf8 -Value "$nowIso,$uuid,$used"
                        if (-not $maxByUuid.ContainsKey($uuid) -or $used -gt $maxByUuid[$uuid]) { $maxByUuid[$uuid] = $used }
                        if ($used -gt $railByUuid[$uuid]) {
                            Stop-OwnedChildren -Children $children -Disposition 'ABORTED-RAIL'
                            $launchDisposition = 'ABORTED-RAIL'
                            # A list, not a scalar: concurrent same-sample
                            # breaches on both devices must all be recorded.
                            $record.rail_breaches += [ordered]@{ device_uuid = $uuid; measured_mib = $used; rail_mib = $railByUuid[$uuid]; at_utc = $nowIso }
                        }
                    }
                }
            }
            catch { $sampleParsed = $false }
            if ($sampleParsed) { $lostCensus = 0 } else { $lostCensus += 1 }
            if ($lostCensus -ge 5) {
                # Five consecutive failed/timed-out/malformed samples: the
                # rails are blind, so the launch aborts fail-closed.
                Stop-OwnedChildren -Children $children -Disposition 'ABORTED-CENSUS-LOST'
                $launchDisposition = 'ABORTED-CENSUS-LOST'
                break
            }
            if ($launchDisposition -cne 'COMPLETED') { break }
            Start-Sleep -Seconds $Rendered.monitor.census_seconds
        }

        foreach ($child in $children) {
            try {
                if (-not $child.process.WaitForExit(15000)) {
                    # ended stays null: this child may STILL be running when
                    # the record is written, and the record must say so
                    # rather than stamp a fictitious end time.
                    $child.disposition = 'WAIT-TIMEOUT'
                    if ($launchDisposition -ceq 'COMPLETED') { $launchDisposition = 'FAILED' }
                    continue
                }
                if ($null -eq $child.ended) { $child.ended = [DateTime]::UtcNow.ToString('o') }
                $stdoutText = $child.stdout.Result
                $stderrText = $child.stderr.Result
                Set-Content -LiteralPath $child.log -Encoding utf8 -Value ($stdoutText + "`n--- STDERR ---`n" + $stderrText)
                if ($null -eq $child.exit_code) { $child.exit_code = $child.process.ExitCode }
                if ($child.disposition -ceq 'RUNNING') {
                    $child.disposition = if ($child.exit_code -eq 0) { 'COMPLETED' } else { 'FAILED' }
                }
                # Deliberately case-insensitive: the certified detector
                # carries (?i) and -match honors it; this is a signature
                # scan, not an identity comparison.
                $child.oom_signature_seen = [bool](($stdoutText + $stderrText) -match $script:AllocationFailurePatternV1)
                if ($child.oom_signature_seen -and $launchDisposition -ceq 'COMPLETED') { $launchDisposition = 'FAILED' }
                if ($child.exit_code -ne 0 -and $launchDisposition -ceq 'COMPLETED') { $launchDisposition = 'FAILED' }
            }
            catch {
                # Literal token per the sheet's enumeration; the message
                # travels in its own detail field.
                $child.disposition = 'COLLECT-ERROR'
                $child.collect_error = $_.Exception.Message
                if ($null -eq $child.ended) { $child.ended = [DateTime]::UtcNow.ToString('o') }
                if ($launchDisposition -ceq 'COMPLETED') { $launchDisposition = 'FAILED' }
            }
        }
    }
    catch {
        $record['abort_error'] = $_.Exception.Message
        if ($launchDisposition -ceq 'RUNNING' -or $launchDisposition -ceq 'COMPLETED') { $launchDisposition = 'ABORTED-ERROR' }
        Stop-OwnedChildren -Children $children -Disposition 'ABORTED-ERROR'
        throw
    }
    finally {
        # The launch record is written with best-known state on EVERY exit
        # path; a monitoring failure must never leave a window undocumented.
        try {
            foreach ($child in $children) {
                $record.processes += [ordered]@{
                    process_index = $child.index
                    device_uuid   = $child.device_uuid
                    seed_offset   = $child.seed_offset
                    store_parent  = $child.store_parent
                    env           = $child.env_map
                    pid           = $child.process.Id
                    start_utc_ticks = $child.start_ticks
                    started_utc   = $child.started
                    ended_utc     = $child.ended
                    exit_code     = $child.exit_code
                    disposition   = $child.disposition
                    collect_error = $child.collect_error
                    kill_outcome  = $child.kill_outcome
                    oom_signature_seen = $child.oom_signature_seen
                    log_path      = $child.log
                }
            }
            $record.disposition = $launchDisposition
            $record['census'] = [ordered]@{
                samples = $censusCount
                series_path = $censusPath
                max_memory_used_mib = [ordered]@{}
            }
            foreach ($uuid in $maxByUuid.Keys) { $record.census.max_memory_used_mib[$uuid] = $maxByUuid[$uuid] }
            $record.ended_utc = [DateTime]::UtcNow.ToString('o')
            ($record | ConvertTo-Json -Depth 10) | Out-File -LiteralPath $recordPath -Encoding utf8
        }
        catch {
            try { Set-Content -LiteralPath (Join-Path $LaunchRoot 'launch-record-write-failure.txt') -Encoding utf8 -Value $_.Exception.Message } catch {}
        }
    }
    if ($launchDisposition -cne 'COMPLETED') { throw "launch disposition $launchDisposition; see $recordPath" }
    return $recordPath
}
