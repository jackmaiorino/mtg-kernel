# Cold gates A1 and A2 plus validator negative checks. Side-effect free:
# render-only, no cargo, no GPU, no filesystem writes outside stdout.
# Exit 0 = all green; any failure throws.
param()
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\common.ps1')

$configsDir = Join-Path $PSScriptRoot '..\configs'
$failures = @()
$passes = @()

function Test-Gate {
    param([string]$Name, [scriptblock]$Body)
    try { & $Body; $script:passes += $Name; Write-Output "PASS $Name" }
    catch { $script:failures += "$Name : $($_.Exception.Message)"; Write-Output "FAIL $Name : $($_.Exception.Message)" }
}

# --- A1: degenerate render equals the legacy golden fixture over the names
# any legacy launcher managed. Null in fixture = legacy unset; the render
# must show those names explicitly cleared (null), not carrying values.
Test-Gate 'A1-degenerate-env-equality' {
    $golden = (Get-Content -LiteralPath (Join-Path $PSScriptRoot 'a1-golden-legacy-env.json') -Raw | ConvertFrom-Json)
    $config = Read-LaunchConfig -ConfigPath (Join-Path $configsDir 'leg-a-solo.json')
    $rendered = Get-RenderedLaunch -Config $config
    if (@($rendered.processes).Count -ne 1) { throw 'degenerate config must render exactly one process' }
    $env = $rendered.processes[0].env
    foreach ($property in $golden.solo_reference.PSObject.Properties) {
        $name = $property.Name
        $expected = $property.Value
        $actual = $env.$name
        if ($null -eq $expected) {
            if ($null -ne $actual) { throw "$name expected cleared, rendered '$actual'" }
        }
        elseif ($actual -ne $expected) { throw "$name expected '$expected', rendered '$actual'" }
    }
}

# --- A2: independent of any fixture, over EVERY config: all whitelist names
# accounted for, the eight value names plus placement trio set, every mode
# knob explicitly cleared, offsets cumulative, store parents distinct.
Test-Gate 'A2-whitelist-clearing-all-configs' {
    $valueNames = $script:ValueNames
    $clearedNames = $script:ClearedNames
    $placementNames = $script:PlacementNames
    foreach ($configFile in Get-ChildItem -LiteralPath $configsDir -Filter '*.json') {
        $config = Read-LaunchConfig -ConfigPath $configFile.FullName
        $rendered = Get-RenderedLaunch -Config $config
        $expectedOffset = [long]0
        $parents = @{}
        foreach ($process in $rendered.processes) {
            $envNames = @($process.env.PSObject.Properties.Name)
            foreach ($name in Get-FullWhitelist) {
                if ($envNames -notcontains $name) { throw "$($configFile.Name) proc $($process.process_index): whitelist name $name missing from env map" }
            }
            if ($envNames.Count -ne (Get-FullWhitelist).Count) { throw "$($configFile.Name): env map has extra names" }
            foreach ($name in $valueNames + $placementNames) {
                if ([string]::IsNullOrEmpty($process.env.$name)) { throw "$($configFile.Name) proc $($process.process_index): $name must be set" }
            }
            foreach ($name in $clearedNames) {
                if ($null -ne $process.env.$name) { throw "$($configFile.Name) proc $($process.process_index): mode knob $name must be cleared" }
            }
            if ($process.env.'MTG_KERNEL_PILOT_CUDA_ORDINAL' -ne '0') { throw 'ordinal must always be 0' }
            if ($process.env.'CUDA_VISIBLE_DEVICES' -ne $process.device_uuid) { throw 'CUDA_VISIBLE_DEVICES must equal the pinned uuid' }
            if ($process.seed_offset -ne $expectedOffset) { throw "$($configFile.Name): offset must be cumulative runs" }
            $expectedOffset += [long]$process.runs
            if ($parents.ContainsKey($process.store_parent)) { throw 'store parents must be distinct' }
            $parents[$process.store_parent] = $true
        }
    }
}

# --- Validator negative checks: deny-unknown, deny-missing, bounds, and the
# v1 actor-count contract must all fail closed.
Test-Gate 'validator-fail-closed' {
    $base = Get-Content -LiteralPath (Join-Path $configsDir 'leg-a-solo.json') -Raw | ConvertFrom-Json
    $cases = @(
        @{ name = 'unknown-key';  mutate = { param($c) $c | Add-Member -NotePropertyName 'extra' -NotePropertyValue 1 } },
        @{ name = 'missing-key';  mutate = { param($c) $c.PSObject.Properties.Remove('base_seed') } },
        @{ name = 'workers-17';   mutate = { param($c) $c.workers = 17 } },
        @{ name = 'broker-gt-actors'; mutate = { param($c) $c.broker_target = 65 } },
        @{ name = 'actors-not-64'; mutate = { param($c) $c.sessions = 16 } },
        @{ name = 'short-sha';    mutate = { param($c) $c.expected_commit = 'f986378' } },
        @{ name = 'dup-uuid';     mutate = { param($c) $c.processes = @($c.processes[0], $c.processes[0]) } }
    )
    $scratch = Join-Path $env:TEMP ("oppoint-gate-negative-{0}.json" -f [guid]::NewGuid().ToString('n'))
    try {
        foreach ($case in $cases) {
            $mutant = $base | ConvertTo-Json -Depth 8 | ConvertFrom-Json
            & $case.mutate $mutant
            ($mutant | ConvertTo-Json -Depth 8) | Out-File -LiteralPath $scratch -Encoding utf8
            $threw = $false
            try { [void](Read-LaunchConfig -ConfigPath $scratch) } catch { $threw = $true }
            if (-not $threw) { throw "negative case '$($case.name)' was accepted" }
        }
    }
    finally { if (Test-Path -LiteralPath $scratch) { Remove-Item -LiteralPath $scratch -Force -Confirm:$false } }
}

Write-Output ('COLD GATES: {0} pass, {1} fail' -f $passes.Count, $failures.Count)
if ($failures.Count -gt 0) { throw ('cold gates failed: ' + ($failures -join '; ')) }
