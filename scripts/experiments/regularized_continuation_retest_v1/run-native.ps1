param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][uint64]$Seed,
    [Parameter(Mandatory = $true)][uint64]$Updates,
    [Parameter(Mandatory = $true)][string]$StoreParent,
    [Parameter(Mandatory = $true)][int]$GpuOrdinal,
    [ValidateSet('0', '0.01', '0.03', '0.1', '0.3')][string]$PolicyAnchorBeta = '0',
    [Parameter(Mandatory = $true)][string]$LogPath,
    [Parameter(Mandatory = $true)][string]$CompletionPath
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path

try {
    if (Test-Path -LiteralPath $CompletionPath) {
        throw "refusing to overwrite child-completion record: $CompletionPath"
    }
    Invoke-NativePilot -Executable $Executable -Seed $Seed -Updates $Updates -StoreParent $StoreParent -GpuOrdinal $GpuOrdinal -PolicyAnchorBeta $PolicyAnchorBeta -LogPath $LogPath -RequirePolicyAnchorMarker
    $completion = [ordered]@{
        schema = 'regularized-continuation-native-lane-completion/v1'
        success = $true
        process_id = $PID
        executable_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Executable).Hash.ToLowerInvariant()
        seed = $Seed
        updates = $Updates
        store_parent = $StoreParent
        gpu_ordinal = $GpuOrdinal
        policy_anchor_beta = $PolicyAnchorBeta
        log_path = $LogPath
        log_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $LogPath).Hash.ToLowerInvariant()
        completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
    }
    $temporaryCompletion = "$CompletionPath.tmp-$PID"
    Write-JsonFile -Value $completion -Path $temporaryCompletion
    Move-Item -LiteralPath $temporaryCompletion -Destination $CompletionPath
    exit 0
}
catch {
    Write-Error $_
    exit 1
}
