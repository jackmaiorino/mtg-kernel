# Multirun operating-point launcher v1: shared validation, rendering, and
# launch machinery. Contract: THROUGHPUT-MULTIRUN-OPERATING-POINT-DESIGN-V1.md
# (collab). Zero Rust changes; consumes the pilot at main f986378 as-is.
# Render-only mode is the cold-gate surface and must stay side-effect free.

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:SchemaId = 'multirun-operating-point-launch/v1'
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
    $actual = @($Object.PSObject.Properties.Name)
    foreach ($key in $actual) {
        if ($Expected -notcontains $key) { throw "unknown key '$key' in $Where (deny-unknown)" }
    }
    foreach ($key in $Expected) {
        if ($actual -notcontains $key) { throw "missing key '$key' in $Where (deny-missing)" }
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
        'sessions', 'broker_target', 'store_parent_root', 'processes'
    )
    if ($config.schema -ne $script:SchemaId) { throw "schema must be '$($script:SchemaId)', got '$($config.schema)'" }
    if ($config.label -notmatch '^[a-z0-9][a-z0-9-]{2,63}$') { throw "label must be kebab-case 3-64 chars, got '$($config.label)'" }
    if ($config.expected_commit -notmatch '^[0-9a-f]{40}$') { throw 'expected_commit must be a full 40-hex sha' }
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
    if (-not [System.IO.Path]::IsPathRooted($config.store_parent_root)) { throw 'store_parent_root must be absolute' }

    $processes = @($config.processes)
    if ($processes.Count -lt 1) { throw 'processes must have at least one entry' }
    $seenUuids = @{}
    foreach ($process in $processes) {
        Assert-ExactKeySet -Object $process -Where 'process entry' -Expected @('device_uuid', 'runs')
        if ($process.device_uuid -notmatch '^GPU-[0-9a-f-]{36}$') { throw "device_uuid must be a full GPU-<uuid>, got '$($process.device_uuid)'" }
        if ($seenUuids.ContainsKey($process.device_uuid)) { throw "duplicate device_uuid '$($process.device_uuid)'" }
        $seenUuids[$process.device_uuid] = $true
        Assert-PositiveInteger -Value $process.runs -Name 'process runs'
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
        $hasExecutable = $item.PSObject.Properties.Name -contains 'executable'
        $hasTarget = $item.PSObject.Properties.Name -contains 'target'
        if ($hasExecutable -and $hasTarget -and $item.executable -and $item.target.name -eq 'mtg-kernel') { $item.executable }
    }
    $executables = @($executables | Select-Object -Unique)
    if ($executables.Count -ne 1) { throw "expected exactly one lib-test executable from cargo JSON, got $($executables.Count)" }
    return $executables[0]
}

function Assert-DeviceInventory {
    # Launch-mode only: every configured UUID must exist on the host.
    param([Parameter(Mandatory = $true)]$Rendered)
    $inventory = @(& nvidia-smi --query-gpu=uuid,name,memory.total --format=csv,noheader)
    if ($LASTEXITCODE -ne 0) { throw 'nvidia-smi inventory query failed' }
    $uuids = @($inventory | ForEach-Object { ($_ -split ',')[0].Trim() })
    foreach ($process in $Rendered.processes) {
        if ($uuids -notcontains $process.device_uuid) { throw "device_uuid '$($process.device_uuid)' not present in live inventory" }
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
    if ($head -ne $Rendered.expected_commit) { throw "repo HEAD $head does not match expected_commit $($Rendered.expected_commit)" }
    $dirty = (& git -C $RepoRoot status --porcelain) | Where-Object { $_ -notmatch '^\?\? scripts/orchestration/' }
    if ($dirty) { throw 'repo tree is not clean (tracked changes present)' }

    New-Item -ItemType Directory -Force -Path $LaunchRoot | Out-Null
    $inventory = Assert-DeviceInventory -Rendered $Rendered
    $sourceExe = Resolve-PilotExecutable -RepoRoot $RepoRoot -StderrPath (Join-Path $LaunchRoot 'cargo-build.stderr.log')
    $exeCopy = Join-Path $LaunchRoot ([System.IO.Path]::GetFileName($sourceExe))
    Copy-Item -LiteralPath $sourceExe -Destination $exeCopy
    $exeSha = Get-Sha256Hex -Path $exeCopy

    $record = [ordered]@{
        schema         = 'multirun-operating-point-launch-record/v1'
        label          = $Rendered.label
        config_path    = (Resolve-Path -LiteralPath $ConfigPath).Path
        config_sha256  = Get-Sha256Hex -Path $ConfigPath
        source_commit  = $head
        executable     = $exeCopy
        executable_sha256 = $exeSha
        device_inventory = @($inventory)
        started_utc    = [DateTime]::UtcNow.ToString('o')
        processes      = @()
    }

    $children = @()
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
        $stdout = $child.StandardOutput.ReadToEndAsync()
        $stderr = $child.StandardError.ReadToEndAsync()
        $children += [pscustomobject]@{
            process = $child; log = $logPath; stdout = $stdout; stderr = $stderr
            index = $process.process_index; started = [DateTime]::UtcNow.ToString('o')
        }
    }
    foreach ($child in $children) {
        $child.process.WaitForExit()
        Set-Content -LiteralPath $child.log -Encoding utf8 -Value ($child.stdout.Result + "`n--- STDERR ---`n" + $child.stderr.Result)
        $record.processes += [ordered]@{
            process_index = $child.index
            pid           = $child.process.Id
            started_utc   = $child.started
            ended_utc     = [DateTime]::UtcNow.ToString('o')
            exit_code     = $child.process.ExitCode
            log_path      = $child.log
        }
    }
    $record.ended_utc = [DateTime]::UtcNow.ToString('o')
    $recordPath = Join-Path $LaunchRoot 'launch-record.json'
    ($record | ConvertTo-Json -Depth 8) | Out-File -LiteralPath $recordPath -Encoding utf8
    $failed = @($record.processes | Where-Object { $_.exit_code -ne 0 })
    if ($failed.Count -gt 0) { throw "launch completed with $($failed.Count) failing process(es); see $recordPath" }
    return $recordPath
}
