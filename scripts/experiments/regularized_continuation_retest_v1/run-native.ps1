param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][uint64]$Seed,
    [Parameter(Mandatory = $true)][uint64]$Updates,
    [Parameter(Mandatory = $true)][string]$StoreParent,
    [Parameter(Mandatory = $true)][int]$GpuOrdinal,
    [ValidateSet('0', '0.01', '0.03', '0.1', '0.3')][string]$PolicyAnchorBeta = '0',
    [Parameter(Mandatory = $true)][string]$LogPath
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path

try {
    Invoke-NativePilot -Executable $Executable -Seed $Seed -Updates $Updates -StoreParent $StoreParent -GpuOrdinal $GpuOrdinal -PolicyAnchorBeta $PolicyAnchorBeta -LogPath $LogPath -RequirePolicyAnchorMarker
    exit 0
}
catch {
    Write-Error $_
    exit 1
}
