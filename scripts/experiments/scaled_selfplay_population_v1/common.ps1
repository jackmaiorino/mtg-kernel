Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:ScaledScriptRoot = $PSScriptRoot
. (Join-Path $PSScriptRoot '..\regularized_continuation_retest_v1\common.ps1')

$script:EnvironmentNames = @($script:EnvironmentNames) + @(
    'MULTIRUN_POPULATION_AUTHORITY', 'MULTIRUN_POPULATION_RUNTIME',
    'MULTIRUN_POPULATION_REFRESH_CHAIN', 'MULTIRUN_POPULATION_SLOT_ROOTS',
    'MULTIRUN_RESPONSE_EXPLOITER_RUNTIME',
    'MULTIRUN_RESPONSE_EXPLOITER_REFRESH_CHAIN',
    'MULTIRUN_RESPONSE_EXPLOITER_SLOT_ROOTS',
    'MULTIRUN_RESPONSE_EXPLOITER_DENOVO',
    'MULTIRUN_WIDE'
)

function Set-ScaledNativeEnvironment {
    param(
        [Parameter(Mandatory = $true)][uint64]$Seed,
        [Parameter(Mandatory = $true)][uint64]$Updates,
        [Parameter(Mandatory = $true)][string]$StoreParent,
        [Parameter(Mandatory = $true)][ValidateSet(0, 1)][int]$GpuOrdinal,
        [Parameter(Mandatory = $true)][ValidateSet('retest', 'successor', 'population', 'response-exploiter')][string]$Mode,
        [Nullable[uint64]]$StopAfterGeneration,
        [Nullable[uint64]]$ExpectedResumeGeneration,
        [string]$RefreshChain = '',
        [string]$SlotRoots = '',
        [ValidateSet('0.1', '0.03', '0')][string]$PolicyAnchorBeta = '0.1',
        # De-novo response screen (CLAUDE-DENOVO-SCREEN-SHEET-V1.md): only
        # meaningful with -Mode response-exploiter and -PolicyAnchorBeta '0'
        # (both required together, fail-closed below, matching the Rust
        # runtime's own `ladder_init_present != denovo_enabled` XOR gate).
        # Skips the promoted(2)-bound MULTIRUN_LADDER_INIT_STORE/GEN
        # assignment entirely (genesis falls through to the frozen common
        # model snapshot, the ordinary non-ladder-init path every other
        # fresh-init run already uses) and writes a denovo-specific genesis
        # authority record instead of the promoted(2)-bound
        # policy-anchor-authority one.
        [switch]$DenovoFreshInit,
        [switch]$ResumeExistingStore
    )
    if ($DenovoFreshInit -and ($Mode -ne 'response-exploiter' -or $PolicyAnchorBeta -ne '0')) {
        throw '-DenovoFreshInit requires -Mode response-exploiter and -PolicyAnchorBeta 0'
    }
    if ($Mode -eq 'response-exploiter' -and $PolicyAnchorBeta -eq '0' -and -not $DenovoFreshInit) {
        throw "-PolicyAnchorBeta 0 under -Mode response-exploiter requires -DenovoFreshInit (beta=0 with a warm-started parent is not a defined recipe)"
    }

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
    if ($DenovoFreshInit) {
        Assert-OrCreateDenovoGenesisAuthority -StoreParent $StoreParent -ResumeExistingStore:$ResumeExistingStore | Out-Null
    }
    else {
        Assert-OrCreatePolicyAnchorAuthority -StoreParent $StoreParent -PolicyAnchorBeta $PolicyAnchorBeta -ResumeExistingStore:$ResumeExistingStore | Out-Null
    }
    $saved = @{}
    foreach ($name in $script:EnvironmentNames) {
        $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
    }
    $populationAuthority = $Mode -ne 'retest'
    $populationRuntime = $Mode -eq 'population'
    $responseExploiterRuntime = $Mode -eq 'response-exploiter'
    if ($responseExploiterRuntime) { $populationAuthority = $false }
    $values = @{
        MULTIRUN_RUNS = '1'; MULTIRUN_UPDATES = [string]$Updates
        MULTIRUN_WORKERS = '2'; MULTIRUN_SESSIONS = '32'; MULTIRUN_BROKER_TARGET = '16'
        MULTIRUN_RECORD_ONLY = '1'; MULTIRUN_BASE_SEED = [string]$Seed
        MULTIRUN_SEED_OFFSET = '0'; MULTIRUN_STORE_PARENT = $StoreParent
        MULTIRUN_LADDER = '1'
        MULTIRUN_LADDER_INIT_STORE = if ($DenovoFreshInit) { $null } else { $script:InitStore }
        MULTIRUN_LADDER_INIT_GEN = if ($DenovoFreshInit) { $null } else { [string]$script:InitGeneration }
        MULTIRUN_LADDER_POOL_DIR = $script:PoolRoot
        MULTIRUN_ENVIRONMENT_RANDOMIZATION_V2 = '1'; MULTIRUN_POLICY_ANCHOR_BETA = $PolicyAnchorBeta
        MULTIRUN_STOP_AFTER_GENERATION = if ($null -eq $StopAfterGeneration) { $null } else { [string]$StopAfterGeneration }
        MULTIRUN_EXPECT_RESUME_GENERATION = if ($null -eq $ExpectedResumeGeneration) { $null } else { [string]$ExpectedResumeGeneration }
        MULTIRUN_POPULATION_AUTHORITY = if ($populationAuthority) { '1' } else { '0' }
        MULTIRUN_POPULATION_RUNTIME = if ($populationRuntime) { '1' } else { '0' }
        MULTIRUN_POPULATION_REFRESH_CHAIN = if ($populationRuntime) { $RefreshChain } else { $null }
        MULTIRUN_POPULATION_SLOT_ROOTS = if ($populationRuntime) { $SlotRoots } else { $null }
        MULTIRUN_RESPONSE_EXPLOITER_RUNTIME = if ($responseExploiterRuntime) { '1' } else { '0' }
        MULTIRUN_RESPONSE_EXPLOITER_REFRESH_CHAIN = if ($responseExploiterRuntime) { $RefreshChain } else { $null }
        MULTIRUN_RESPONSE_EXPLOITER_SLOT_ROOTS = if ($responseExploiterRuntime) { $SlotRoots } else { $null }
        MULTIRUN_RESPONSE_EXPLOITER_DENOVO = if ($DenovoFreshInit) { '1' } else { '0' }
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
        [Parameter(Mandatory = $true)][ValidateSet('retest', 'successor', 'population', 'response-exploiter')][string]$Mode,
        [Parameter(Mandatory = $true)][string]$LogPath,
        [Nullable[uint64]]$StopAfterGeneration,
        [Nullable[uint64]]$ExpectedResumeGeneration,
        [string]$RefreshChain = '',
        [string]$SlotRoots = '',
        [ValidateSet('0.1', '0.03', '0')][string]$PolicyAnchorBeta = '0.1',
        [switch]$DenovoFreshInit,
        [switch]$ResumeExistingStore
    )
    $saved = Set-ScaledNativeEnvironment -Seed $Seed -Updates $Updates -StoreParent $StoreParent -GpuOrdinal $GpuOrdinal -Mode $Mode -StopAfterGeneration $StopAfterGeneration -ExpectedResumeGeneration $ExpectedResumeGeneration -RefreshChain $RefreshChain -SlotRoots $SlotRoots -PolicyAnchorBeta $PolicyAnchorBeta -DenovoFreshInit:$DenovoFreshInit -ResumeExistingStore:$ResumeExistingStore
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
    $responseRuntime = if ($Mode -eq 'response-exploiter') { 'true' } else { 'false' }
    if ($Mode -eq 'response-exploiter') { $authority = 'false' }
    if ($text -notmatch "MULTIRUN CONFIG .*envrand_v2=true .*population_authority=$authority .*population_runtime=$runtime .*response_exploiter_runtime=$responseRuntime") {
        throw 'scaled runner configuration marker is absent'
    }
    $denovoMarker = if ($DenovoFreshInit) { 'true' } else { 'false' }
    if ($text -notmatch "response_exploiter_denovo=$denovoMarker") {
        throw 'scaled runner denovo configuration marker is absent'
    }
    $escapedBeta = [regex]::Escape($PolicyAnchorBeta)
    if ($text -notmatch "policy_anchor_beta=$escapedBeta(?:\s|$)") {
        throw "policy-anchor beta $PolicyAnchorBeta marker is absent"
    }
    if ($Mode -eq 'response-exploiter' -and
        $text -notmatch 'training_only=true terminal_outcomes_read=false') {
        throw 'response-exploiter training-only marker is absent'
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
        [Parameter(Mandatory = $true)][ValidateSet('retest', 'successor', 'population', 'response-exploiter')][string]$Mode,
        [Parameter(Mandatory = $true)][string]$LogPath,
        [Parameter(Mandatory = $true)][string]$EvidenceRoot,
        [Nullable[uint64]]$StopAfterGeneration,
        [Nullable[uint64]]$ExpectedResumeGeneration,
        [string]$RefreshChain = '',
        [string]$SlotRoots = '',
        [ValidateSet('0.1', '0.03', '0')][string]$PolicyAnchorBeta = '0.1',
        [switch]$DenovoFreshInit,
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
        '-LogPath', $LogPath, '-CompletionPath', $completion,
        '-PolicyAnchorBeta', $PolicyAnchorBeta)
    if ($null -ne $StopAfterGeneration) { $childArgs += @('-StopAfterGeneration', [string]$StopAfterGeneration) }
    if ($null -ne $ExpectedResumeGeneration) { $childArgs += @('-ExpectedResumeGeneration', [string]$ExpectedResumeGeneration) }
    if (-not [string]::IsNullOrWhiteSpace($RefreshChain)) { $childArgs += @('-RefreshChain', $RefreshChain) }
    if (-not [string]::IsNullOrWhiteSpace($SlotRoots)) { $childArgs += @('-SlotRoots', $SlotRoots) }
    if ($DenovoFreshInit) { $childArgs += '-DenovoFreshInit' }
    if ($ResumeExistingStore) { $childArgs += '-ResumeExistingStore' }
    $argText = ($childArgs | ForEach-Object { '"' + ([string]$_).Replace('"', '\"') + '"' }) -join ' '
    $started = [DateTimeOffset]::UtcNow
    $process = Start-Process -FilePath $hostCommand.Source -ArgumentList $argText -WorkingDirectory $script:RepoRoot -PassThru -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    return [pscustomobject]@{
        process = $process; gpu_ordinal = $GpuOrdinal; store_parent = $StoreParent
        log = $LogPath; stdout = $stdout; stderr = $stderr; completion = $completion
        executable = $Executable; seed = $Seed; updates = $Updates
        policy_anchor_beta = $PolicyAnchorBeta; mode = $Mode
        denovo_fresh_init = [bool]$DenovoFreshInit
        started_utc = $started.ToString('O'); started = $started
    }
}

function Get-ScaledEndpointRecord {
    param(
        [Parameter(Mandatory = $true)][string]$StoreRoot,
        [Parameter(Mandatory = $true)][uint64]$Generation
    )
    $latest = Get-Content -Raw -LiteralPath (Join-Path $StoreRoot 'latest.json') | ConvertFrom-Json
    if ([uint64]$latest.generation_index -lt $Generation) {
        throw "Store latest generation precedes requested checkpoint $Generation"
    }
    $prefix = Join-Path $StoreRoot ('checkpoints\update-{0:d8}' -f $Generation)
    foreach ($suffix in @('checkpoint.json', 'sidecar.json', 'state.f32le')) {
        if (-not (Test-Path -LiteralPath "$prefix.$suffix" -PathType Leaf)) {
            throw "generation-$Generation checkpoint file is missing: $prefix.$suffix"
        }
    }
    $checkpoint = Get-Content -Raw -LiteralPath "$prefix.checkpoint.json" | ConvertFrom-Json
    $stateFileSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath "$prefix.state.f32le").Hash.ToLowerInvariant()
    if ([string]$checkpoint.payload.sha256 -cne $stateFileSha256) {
        throw "generation-$Generation checkpoint payload and state-file hashes differ"
    }
    return [ordered]@{
        store_root = (Resolve-Path -LiteralPath $StoreRoot).Path
        tree_sha256 = Get-StoreTreeHash -Path $StoreRoot
        generation = $Generation
        run_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $StoreRoot 'run.json')).Hash.ToLowerInvariant()
        checkpoint_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath "$prefix.checkpoint.json").Hash.ToLowerInvariant()
        sidecar_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath "$prefix.sidecar.json").Hash.ToLowerInvariant()
        state_sha256 = $stateFileSha256
        checkpoint_payload_sha256 = [string]$checkpoint.payload.sha256
        train_state_sha256 = [string]$checkpoint.train_state.state_sha256
        model_parameter_sha256 = [string]$checkpoint.train_state.model_parameter_sha256
        identity_bundle_sha256 = [string]$checkpoint.identity_bundle_sha256
        adam_step = [uint64]$checkpoint.train_state.adam_step
        completed_episode_count = [uint64]$checkpoint.progress.completed_episode_count
    }
}
