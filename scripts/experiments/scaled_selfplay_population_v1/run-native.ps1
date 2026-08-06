param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][uint64]$Seed,
    [Parameter(Mandatory = $true)][uint64]$Updates,
    [Parameter(Mandatory = $true)][string]$StoreParent,
    [Parameter(Mandatory = $true)][ValidateSet(0, 1)][int]$GpuOrdinal,
    [Parameter(Mandatory = $true)][ValidateSet('retest', 'successor', 'population')][string]$Mode,
    [Parameter(Mandatory = $true)][string]$LogPath,
    [Parameter(Mandatory = $true)][string]$CompletionPath,
    [Nullable[uint64]]$StopAfterGeneration,
    [Nullable[uint64]]$ExpectedResumeGeneration,
    [string]$RefreshChain = '',
    [string]$SlotRoots = '',
    [switch]$ResumeExistingStore
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path

Invoke-ScaledNativePilot -Executable $Executable -Seed $Seed -Updates $Updates -StoreParent $StoreParent -GpuOrdinal $GpuOrdinal -Mode $Mode -LogPath $LogPath -StopAfterGeneration $StopAfterGeneration -ExpectedResumeGeneration $ExpectedResumeGeneration -RefreshChain $RefreshChain -SlotRoots $SlotRoots -ResumeExistingStore:$ResumeExistingStore

Write-JsonFile -Value ([ordered]@{
    schema = 'regularized-continuation-native-lane-completion/v1'
    success = $true
    process_id = $PID
    seed = $Seed
    updates = $Updates
    gpu_ordinal = $GpuOrdinal
    policy_anchor_beta = '0.1'
    store_parent = $StoreParent
    log_path = $LogPath
    executable_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Executable).Hash.ToLowerInvariant()
    log_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $LogPath).Hash.ToLowerInvariant()
    mode = $Mode
    completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
}) -Path $CompletionPath
