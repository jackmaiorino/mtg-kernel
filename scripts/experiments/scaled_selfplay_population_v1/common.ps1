Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:ScaledScriptRoot = $PSScriptRoot
. (Join-Path $PSScriptRoot '..\regularized_continuation_retest_v1\common.ps1')

$script:EnvironmentNames = @($script:EnvironmentNames) + @(
    'MULTIRUN_POPULATION_AUTHORITY', 'MULTIRUN_POPULATION_RUNTIME',
    'MULTIRUN_POPULATION_REFRESH_CHAIN', 'MULTIRUN_POPULATION_SLOT_ROOTS',
    'MULTIRUN_WIDE'
)

function Set-ScaledNativeEnvironment {
    param(
        [Parameter(Mandatory = $true)][uint64]$Seed,
        [Parameter(Mandatory = $true)][uint64]$Updates,
        [Parameter(Mandatory = $true)][string]$StoreParent,
        [Parameter(Mandatory = $true)][ValidateSet(0, 1)][int]$GpuOrdinal,
        [Parameter(Mandatory = $true)][ValidateSet('retest', 'successor', 'population')][string]$Mode,
        [Nullable[uint64]]$StopAfterGeneration,
        [Nullable[uint64]]$ExpectedResumeGeneration,
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
    Assert-OrCreatePolicyAnchorAuthority -StoreParent $StoreParent -PolicyAnchorBeta '0.1' -ResumeExistingStore:$ResumeExistingStore | Out-Null
    $saved = @{}
    foreach ($name in $script:EnvironmentNames) {
        $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
    }
    $populationAuthority = $Mode -ne 'retest'
    $populationRuntime = $Mode -eq 'population'
    $values = @{
        MULTIRUN_RUNS = '1'; MULTIRUN_UPDATES = [string]$Updates
        MULTIRUN_WORKERS = '2'; MULTIRUN_SESSIONS = '32'; MULTIRUN_BROKER_TARGET = '16'
        MULTIRUN_RECORD_ONLY = '1'; MULTIRUN_BASE_SEED = [string]$Seed
        MULTIRUN_SEED_OFFSET = '0'; MULTIRUN_STORE_PARENT = $StoreParent
        MULTIRUN_LADDER = '1'; MULTIRUN_LADDER_INIT_STORE = $script:InitStore
        MULTIRUN_LADDER_INIT_GEN = [string]$script:InitGeneration
        MULTIRUN_LADDER_POOL_DIR = $script:PoolRoot
        MULTIRUN_ENVIRONMENT_RANDOMIZATION_V2 = '1'; MULTIRUN_POLICY_ANCHOR_BETA = '0.1'
        MULTIRUN_STOP_AFTER_GENERATION = if ($null -eq $StopAfterGeneration) { $null } else { [string]$StopAfterGeneration }
        MULTIRUN_EXPECT_RESUME_GENERATION = if ($null -eq $ExpectedResumeGeneration) { $null } else { [string]$ExpectedResumeGeneration }
        MULTIRUN_POPULATION_AUTHORITY = if ($populationAuthority) { '1' } else { '0' }
        MULTIRUN_POPULATION_RUNTIME = if ($populationRuntime) { '1' } else { '0' }
        MULTIRUN_POPULATION_REFRESH_CHAIN = if ($populationRuntime) { $RefreshChain } else { $null }
        MULTIRUN_POPULATION_SLOT_ROOTS = if ($populationRuntime) { $SlotRoots } else { $null }
        MULTIRUN_WIDE = '0'; CUDA_DEVICE_ORDER = 'PCI_BUS_ID'
        CUDA_VISIBLE_DEVICES = $gpu.uuid; MTG_KERNEL_PILOT_CUDA_ORDINAL = '0'
    }
    foreach ($name in $script:EnvironmentNames) {
        [Environment]::SetEnvironmentVariable($name, $values[$name], 'Process')
    }
    return $saved
}

function Invoke-ScaledNativePilot {
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][uint64]$Seed,
        [Parameter(Mandatory = $true)][uint64]$Updates,
        [Parameter(Mandatory = $true)][string]$StoreParent,
        [Parameter(Mandatory = $true)][ValidateSet(0, 1)][int]$GpuOrdinal,
        [Parameter(Mandatory = $true)][ValidateSet('retest', 'successor', 'population')][string]$Mode,
        [Parameter(Mandatory = $true)][string]$LogPath,
        [Nullable[uint64]]$StopAfterGeneration,
        [Nullable[uint64]]$ExpectedResumeGeneration,
        [string]$RefreshChain = '',
        [string]$SlotRoots = '',
        [switch]$ResumeExistingStore
    )
    $saved = Set-ScaledNativeEnvironment -Seed $Seed -Updates $Updates -StoreParent $StoreParent -GpuOrdinal $GpuOrdinal -Mode $Mode -StopAfterGeneration $StopAfterGeneration -ExpectedResumeGeneration $ExpectedResumeGeneration -RefreshChain $RefreshChain -SlotRoots $SlotRoots -ResumeExistingStore:$ResumeExistingStore
    try {
        $previous = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try {
            & $Executable $script:RunnerTest --ignored --exact --nocapture --test-threads=1 2>&1 |
                Tee-Object -FilePath $LogPath | Out-Null
            $nativeExit = $LASTEXITCODE
        }
        finally { $ErrorActionPreference = $previous }
        Assert-LastExitCode $nativeExit 'scaled native pilot'
    }
    finally { Restore-NativeEnvironment -Saved $saved }

    $text = Get-Content -LiteralPath $LogPath -Raw
    $authority = if ($Mode -eq 'retest') { 'false' } else { 'true' }
    $runtime = if ($Mode -eq 'population') { 'true' } else { 'false' }
    if ($text -notmatch "MULTIRUN CONFIG .*envrand_v2=true .*population_authority=$authority .*population_runtime=$runtime") {
        throw 'scaled runner configuration marker is absent'
    }
    if ($text -notmatch 'policy_anchor_beta=0\.1(?:\s|$)') {
        throw 'policy-anchor beta 0.1 marker is absent'
    }
    $start = if ($null -eq $ExpectedResumeGeneration) { [uint64]0 } else { [uint64]$ExpectedResumeGeneration }
    $end = if ($null -eq $StopAfterGeneration) { $Updates } else { [uint64]$StopAfterGeneration }
    $episodes = [uint64]64 * ($end - $start)
    if ($text -notmatch "MULTIRUN AGGREGATE runs=1 episodes=$episodes") {
        throw 'scaled native aggregate completion marker is absent'
    }
}

function Start-ScaledNativeLane {
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][uint64]$Seed,
        [Parameter(Mandatory = $true)][uint64]$Updates,
        [Parameter(Mandatory = $true)][string]$StoreParent,
        [Parameter(Mandatory = $true)][ValidateSet(0, 1)][int]$GpuOrdinal,
        [Parameter(Mandatory = $true)][ValidateSet('retest', 'successor', 'population')][string]$Mode,
        [Parameter(Mandatory = $true)][string]$LogPath,
        [Parameter(Mandatory = $true)][string]$EvidenceRoot,
        [Nullable[uint64]]$StopAfterGeneration,
        [Nullable[uint64]]$ExpectedResumeGeneration,
        [string]$RefreshChain = '',
        [string]$SlotRoots = '',
        [switch]$ResumeExistingStore
    )
    $hostCommand = Get-Command pwsh -ErrorAction SilentlyContinue
    if ($null -eq $hostCommand) { $hostCommand = Get-Command powershell -ErrorAction Stop }
    $label = [IO.Path]::GetFileNameWithoutExtension($LogPath)
    $stdout = Join-Path $EvidenceRoot "$label.stdout.log"
    $stderr = Join-Path $EvidenceRoot "$label.stderr.log"
    $completion = Join-Path $EvidenceRoot "$label.completion.json"
    $childArgs = @('-NoProfile', '-WindowStyle', 'Hidden', '-File', (Join-Path $script:ScaledScriptRoot 'run-native.ps1'),
        '-Executable', $Executable, '-Seed', $Seed, '-Updates', $Updates,
        '-StoreParent', $StoreParent, '-GpuOrdinal', $GpuOrdinal, '-Mode', $Mode,
        '-LogPath', $LogPath, '-CompletionPath', $completion)
    if ($null -ne $StopAfterGeneration) { $childArgs += @('-StopAfterGeneration', [string]$StopAfterGeneration) }
    if ($null -ne $ExpectedResumeGeneration) { $childArgs += @('-ExpectedResumeGeneration', [string]$ExpectedResumeGeneration) }
    if (-not [string]::IsNullOrWhiteSpace($RefreshChain)) { $childArgs += @('-RefreshChain', $RefreshChain) }
    if (-not [string]::IsNullOrWhiteSpace($SlotRoots)) { $childArgs += @('-SlotRoots', $SlotRoots) }
    if ($ResumeExistingStore) { $childArgs += '-ResumeExistingStore' }
    $argText = ($childArgs | ForEach-Object { '"' + ([string]$_).Replace('"', '\"') + '"' }) -join ' '
    $started = [DateTimeOffset]::UtcNow
    $process = Start-Process -FilePath $hostCommand.Source -ArgumentList $argText -WorkingDirectory $script:RepoRoot -PassThru -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    return [pscustomobject]@{
        process = $process; gpu_ordinal = $GpuOrdinal; store_parent = $StoreParent
        log = $LogPath; stdout = $stdout; stderr = $stderr; completion = $completion
        executable = $Executable; seed = $Seed; updates = $Updates
        policy_anchor_beta = '0.1'; mode = $Mode
        started_utc = $started.ToString('O'); started = $started
    }
}

function Get-ScaledEndpointRecord {
    param(
        [Parameter(Mandatory = $true)][string]$StoreRoot,
        [Parameter(Mandatory = $true)][uint64]$Generation
    )
    Assert-GenerationCheckpoint -Store $StoreRoot -Generation $Generation
    $prefix = Join-Path $StoreRoot ('checkpoints\update-{0:d8}' -f $Generation)
    $checkpoint = Get-Content -Raw -LiteralPath "$prefix.checkpoint.json" | ConvertFrom-Json
    return [ordered]@{
        store_root = (Resolve-Path -LiteralPath $StoreRoot).Path
        tree_sha256 = Get-StoreTreeHash -Path $StoreRoot
        generation = $Generation
        run_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $StoreRoot 'run.json')).Hash.ToLowerInvariant()
        checkpoint_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath "$prefix.checkpoint.json").Hash.ToLowerInvariant()
        sidecar_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath "$prefix.sidecar.json").Hash.ToLowerInvariant()
        state_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath "$prefix.state.f32le").Hash.ToLowerInvariant()
        model_parameter_sha256 = [string]$checkpoint.train_state.model_parameter_sha256
        adam_step = [uint64]$checkpoint.train_state.adam_step
        completed_episode_count = [uint64]$checkpoint.progress.completed_episode_count
    }
}
