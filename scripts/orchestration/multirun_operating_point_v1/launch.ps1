# Entry point. Render-only mode (the cold-gate surface) validates the config
# and emits the rendered launch JSON without side effects. Launch mode is
# window-only per THROUGHPUT-MULTIRUN-OPERATING-POINT-DESIGN-V1.md and must
# never run outside an orchestrator-scheduled exclusive window.
param(
    [Parameter(Mandatory = $true)][string]$ConfigPath,
    [switch]$RenderOnly,
    [string]$LaunchRoot,
    [string]$RepoRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
# RepoRoot is derived in the body, not a param default: under
# powershell -File, $PSScriptRoot proved empty during param-default
# evaluation (observed live), while it is always set here.
if ([string]::IsNullOrWhiteSpace($RepoRoot)) {
    $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
}
. (Join-Path $PSScriptRoot 'common.ps1')

$config = Read-LaunchConfig -ConfigPath $ConfigPath
$rendered = Get-RenderedLaunch -Config $config

if ($RenderOnly) {
    $rendered | ConvertTo-Json -Depth 8
    exit 0
}

if ([string]::IsNullOrWhiteSpace($LaunchRoot)) { throw 'launch mode requires -LaunchRoot' }
$recordPath = Start-RenderedLaunch -Rendered $rendered -RepoRoot $RepoRoot -LaunchRoot $LaunchRoot -ConfigPath $ConfigPath
Write-Output "LAUNCH COMPLETE record=$recordPath"
