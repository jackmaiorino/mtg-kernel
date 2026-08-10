# Gate A3, legacy side. Runs the pilot the HISTORICAL MANUAL way: a
# hand-typed environment block and a direct shell invocation of the
# resolved test binary, no launcher machinery. The env values are typed
# literals deliberately mirroring a3-degenerate.json's parameters so the
# pair differs ONLY in invocation mechanism (launcher spawn path vs direct
# shell); placement is the same UUID masking so no placement variable
# contaminates the store comparison. Clean-environment harness: every
# whitelist name is unset first, so nothing inherited leaks in.
# GPU RUN: stage-2 only, requires the orchestrator all-clear.
param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..\..')).Path
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\common.ps1')

$storeParent = 'D:\mtg-kernel-oppoint-a3-legacy\proc-0'
if (Test-Path -LiteralPath 'D:\mtg-kernel-oppoint-a3-legacy') { throw 'legacy A3 root must be fresh' }

$saved = @{}
foreach ($name in Get-FullWhitelist) {
    $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
    [Environment]::SetEnvironmentVariable($name, $null, 'Process')
}
try {
    # Hand-typed literal block: the manual invocation shape.
    [Environment]::SetEnvironmentVariable('MULTIRUN_RUNS', '1', 'Process')
    [Environment]::SetEnvironmentVariable('MULTIRUN_UPDATES', '4', 'Process')
    [Environment]::SetEnvironmentVariable('MULTIRUN_WORKERS', '2', 'Process')
    [Environment]::SetEnvironmentVariable('MULTIRUN_SESSIONS', '32', 'Process')
    [Environment]::SetEnvironmentVariable('MULTIRUN_BROKER_TARGET', '16', 'Process')
    [Environment]::SetEnvironmentVariable('MULTIRUN_BASE_SEED', '424242', 'Process')
    [Environment]::SetEnvironmentVariable('MULTIRUN_SEED_OFFSET', '0', 'Process')
    [Environment]::SetEnvironmentVariable('MULTIRUN_STORE_PARENT', $storeParent, 'Process')
    [Environment]::SetEnvironmentVariable('CUDA_DEVICE_ORDER', 'PCI_BUS_ID', 'Process')
    [Environment]::SetEnvironmentVariable('CUDA_VISIBLE_DEVICES', 'GPU-3502709e-6aef-8ed7-4abe-562838793e3d', 'Process')
    [Environment]::SetEnvironmentVariable('MTG_KERNEL_PILOT_CUDA_ORDINAL', '0', 'Process')

    New-Item -ItemType Directory -Force -Path $storeParent | Out-Null
    $executable = Resolve-PilotExecutable -RepoRoot $RepoRoot -StderrPath 'D:\mtg-kernel-oppoint-a3-legacy\cargo-build.stderr.log'
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & $executable $script:RunnerTest --ignored --exact --nocapture --test-threads=1 2>&1 |
            Tee-Object -FilePath 'D:\mtg-kernel-oppoint-a3-legacy\legacy-invoke.log' | Out-Null
        $pilotExit = $LASTEXITCODE
    }
    finally { $ErrorActionPreference = $previous }
    if ($pilotExit -ne 0) { throw "legacy A3 pilot failed (exit $pilotExit)" }
    Write-Output "A3 LEGACY SIDE COMPLETE store=$storeParent"
}
finally {
    foreach ($name in Get-FullWhitelist) {
        [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process')
    }
}
