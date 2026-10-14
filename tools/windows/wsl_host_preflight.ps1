# Read-only: no VM setters, guest workload, WSL restart or runner activation.
param(
    [string]$Python = 'D:/mtg-ci/toolcache/Python/3.13.14/x64/python.exe',
    [string]$Output = 'D:/e-scratch/wsl-isolation-qualification-20261013/host-preflight.json'
)
$ErrorActionPreference = 'Stop'
$processes = Get-CimInstance Win32_Process
$vmmem = @($processes | Where-Object { $_.Name -eq 'vmmemWSL' })
$relay = @($processes | Where-Object {
    $_.Name -eq 'wslrelay.exe' -and $_.CommandLine -match '--vm-id \{([0-9a-fA-F-]{36})\}'
})
if ($vmmem.Count -ne 1 -or $relay.Count -ne 1) {
    throw 'Expected one existing WSL VM and one vmmemWSL process; do not start or restart WSL to satisfy this probe'
}
$vmId = [regex]::Match($relay[0].CommandLine, '--vm-id \{([0-9a-fA-F-]{36})\}').Groups[1].Value
$source = Join-Path $PSScriptRoot '../../python/tools/wsl_host_preflight_v1.py'
$fresh = Join-Path ([IO.Path]::GetTempPath()) ('wsl-host-preflight-' + [guid]::NewGuid() + '.json')
try {
    & $Python -B $source --system-id $vmId --vmmem-pid $vmmem[0].ProcessId --output $fresh
    $probeExit = $LASTEXITCODE
    if ($probeExit -notin @(0, 6) -or -not (Test-Path -LiteralPath $fresh -PathType Leaf)) {
        throw "Unexpected Python probe failure (exit $probeExit); previous output is preserved"
    }
    $report = Get-Content -LiteralPath $fresh -Raw | ConvertFrom-Json
    if ($report.schema -ne 'wsl-host-preflight-v1' -or $report.qualified -ne $false) {
        throw 'Unexpected preflight report; previous output is preserved'
    }
    $event = Get-WinEvent -FilterHashtable @{
        LogName = 'System'; ProviderName = 'Microsoft-Windows-Hyper-V-Hypervisor'; Id = 2
    } -MaxEvents 1 -ErrorAction SilentlyContinue
    $report | Add-Member -NotePropertyName scheduler_event -NotePropertyValue @{
        message = $event.Message; time_created = [string]$event.TimeCreated
    }
    $parent = Split-Path -Parent ([IO.Path]::GetFullPath($Output))
    New-Item -ItemType Directory -Force $parent | Out-Null
    $report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $Output -Encoding UTF8
    Write-Host "Preflight exit $probeExit; result saved to $Output. This does not qualify isolation or enable Linux CI."
}
finally {
    if (Test-Path -LiteralPath $fresh -PathType Leaf) { Remove-Item -LiteralPath $fresh }
}
exit $probeExit
