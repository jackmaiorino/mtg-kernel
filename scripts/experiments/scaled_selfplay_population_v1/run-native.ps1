param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][uint64]$Seed,
    [Parameter(Mandatory = $true)][uint64]$Updates,
    [Parameter(Mandatory = $true)][string]$StoreParent,
    [Parameter(Mandatory = $true)][ValidateSet(0, 1)][int]$GpuOrdinal,
    [Parameter(Mandatory = $true)][ValidateSet('retest', 'successor', 'population', 'response-exploiter')][string]$Mode,
    [Parameter(Mandatory = $true)][string]$LogPath,
    [Parameter(Mandatory = $true)][string]$CompletionPath,
    [Nullable[uint64]]$StopAfterGeneration,
    [Nullable[uint64]]$ExpectedResumeGeneration,
    [string]$RefreshChain = '',
    [string]$SlotRoots = '',
    [ValidateSet('0.1', '0.03', '0')][string]$PolicyAnchorBeta = '0.1',
    [switch]$DenovoFreshInit,
    [switch]$ResumeExistingStore
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path

Invoke-ScaledNativePilot -Executable $Executable -Seed $Seed -Updates $Updates -StoreParent $StoreParent -GpuOrdinal $GpuOrdinal -Mode $Mode -LogPath $LogPath -StopAfterGeneration $StopAfterGeneration -ExpectedResumeGeneration $ExpectedResumeGeneration -RefreshChain $RefreshChain -SlotRoots $SlotRoots -PolicyAnchorBeta $PolicyAnchorBeta -DenovoFreshInit:$DenovoFreshInit -ResumeExistingStore:$ResumeExistingStore

Write-JsonFile -Value ([ordered]@{
    schema = 'regularized-continuation-native-lane-completion/v1'
    success = $true
    process_id = $PID
    seed = $Seed
    updates = $Updates
    gpu_ordinal = $GpuOrdinal
    policy_anchor_beta = $PolicyAnchorBeta
    denovo_fresh_init = [bool]$DenovoFreshInit
    store_parent = $StoreParent
    log_path = $LogPath
    executable_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Executable).Hash.ToLowerInvariant()
    log_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $LogPath).Hash.ToLowerInvariant()
    mode = $Mode
    completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
}) -Path $CompletionPath
