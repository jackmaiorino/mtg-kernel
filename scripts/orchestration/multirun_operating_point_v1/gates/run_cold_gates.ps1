# Cold gates. Side-effect free except scratch files in $env:TEMP: render-only,
# no cargo, no GPU. Exit 0 = all green.
param()
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\common.ps1')

$configsDir = Join-Path $PSScriptRoot '..\configs'
$repoSrc = Resolve-Path (Join-Path $PSScriptRoot '..\..\..\..\mtg-kernel\src')
$failures = @()
$passes = @()

function Test-Gate {
    param([string]$Name, [scriptblock]$Body)
    try { & $Body; $script:passes += $Name; Write-Output "PASS $Name" }
    catch { $script:failures += "$Name : $($_.Exception.Message)"; Write-Output "FAIL $Name : $($_.Exception.Message)" }
}

# --- A1: renderer conformance against the DERIVED golden (generated from the
# certified fair-campaign records by capture_a1_golden.ps1). For the five
# run-semantics names the certified runs managed, the render must equal the
# legacy mapping applied to the same config; for the placement and
# seed/store names, the render must equal the fixture's declared v1
# divergence policy; everything else must be cleared.
Test-Gate 'A1-certified-mapping-conformance' {
    $golden = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'a1-golden-legacy-env.json') -Raw | ConvertFrom-Json
    if ($golden.schema -cne 'multirun-operating-point-gate-a1-golden/v2') { throw 'golden fixture has wrong schema; regenerate via capture_a1_golden.ps1' }
    foreach ($configFile in Get-ChildItem -LiteralPath $configsDir -Filter '*.json') {
        $config = Read-LaunchConfig -ConfigPath $configFile.FullName
        $rendered = Get-RenderedLaunch -Config $config
        # Certified parameter cross-check: every leg runs the certified
        # 2/32/16 point; updates match the certified 64 except the probe,
        # whose depth the design sheet pins and reconciles explicitly.
        if ([string]$config.workers -cne $golden.certified_env.managed_names.MULTIRUN_WORKERS) { throw "$($configFile.Name): workers diverge from certified" }
        if ([string]$config.sessions -cne $golden.certified_env.managed_names.MULTIRUN_SESSIONS) { throw "$($configFile.Name): sessions diverge from certified" }
        if ([string]$config.broker_target -cne $golden.certified_env.managed_names.MULTIRUN_BROKER_TARGET) { throw "$($configFile.Name): broker_target diverges from certified" }
        foreach ($process in $rendered.processes) {
            # Legacy-managed semantic names: exact mapping equality.
            if ($process.env.MULTIRUN_RUNS -cne [string]$process.runs) { throw 'MULTIRUN_RUNS mapping broken' }
            if ($process.env.MULTIRUN_UPDATES -cne [string]$config.updates) { throw 'MULTIRUN_UPDATES mapping broken' }
            if ($process.env.MULTIRUN_WORKERS -cne [string]$config.workers) { throw 'MULTIRUN_WORKERS mapping broken' }
            if ($process.env.MULTIRUN_SESSIONS -cne [string]$config.sessions) { throw 'MULTIRUN_SESSIONS mapping broken' }
            if ($process.env.MULTIRUN_BROKER_TARGET -cne [string]$config.broker_target) { throw 'MULTIRUN_BROKER_TARGET mapping broken' }
            # Declared v1 divergences (fixture v1_divergences): placement by
            # UUID masking with ordinal 0, explicit seeds, durable stores.
            if ($process.env.CUDA_VISIBLE_DEVICES -cne $process.device_uuid) { throw 'divergence policy: CUDA_VISIBLE_DEVICES must be the pinned uuid' }
            if ($process.env.MTG_KERNEL_PILOT_CUDA_ORDINAL -cne '0') { throw 'divergence policy: ordinal must be 0' }
            if ($process.env.CUDA_DEVICE_ORDER -cne 'PCI_BUS_ID') { throw 'divergence policy: CUDA_DEVICE_ORDER must be PCI_BUS_ID' }
            if ($process.env.MULTIRUN_BASE_SEED -cne [string]$config.base_seed) { throw 'divergence policy: explicit base seed' }
            if ($process.env.MULTIRUN_SEED_OFFSET -cne [string]$process.seed_offset) { throw 'divergence policy: cumulative seed offset' }
            if ($process.env.MULTIRUN_STORE_PARENT -cne $process.store_parent) { throw 'divergence policy: derived store parent' }
        }
    }
}

# --- A2: over every config: all whitelist names accounted for, value and
# placement names set, every mode knob cleared, offsets cumulative, store
# parents distinct, monitor rendered.
Test-Gate 'A2-whitelist-clearing-all-configs' {
    $valueNames = $script:ValueNames
    $clearedNames = $script:ClearedNames
    $placementNames = $script:PlacementNames
    foreach ($configFile in Get-ChildItem -LiteralPath $configsDir -Filter '*.json') {
        $config = Read-LaunchConfig -ConfigPath $configFile.FullName
        $rendered = Get-RenderedLaunch -Config $config
        if (@($rendered.monitor.device_rails).Count -lt 1) { throw "$($configFile.Name): monitor not rendered" }
        $expectedOffset = [long]0
        $parents = @{}
        foreach ($process in $rendered.processes) {
            $envNames = @($process.env.PSObject.Properties.Name)
            foreach ($name in Get-FullWhitelist) {
                if ($envNames -cnotcontains $name) { throw "$($configFile.Name) proc $($process.process_index): whitelist name $name missing from env map" }
            }
            if ($envNames.Count -ne (Get-FullWhitelist).Count) { throw "$($configFile.Name): env map has extra names" }
            foreach ($name in $valueNames + $placementNames) {
                if ([string]::IsNullOrEmpty($process.env.$name)) { throw "$($configFile.Name) proc $($process.process_index): $name must be set" }
            }
            foreach ($name in $clearedNames) {
                if ($null -ne $process.env.$name) { throw "$($configFile.Name) proc $($process.process_index): mode knob $name must be cleared" }
            }
            if ($process.seed_offset -ne $expectedOffset) { throw "$($configFile.Name): offset must be cumulative runs" }
            $expectedOffset += [long]$process.runs
            if ($parents.ContainsKey($process.store_parent)) { throw 'store parents must be distinct' }
            $parents[$process.store_parent] = $true
        }
    }
}

# --- A2b: the whitelist cannot silently drift from the env surface the Rust
# actually reads at the pinned commit. Union of value+cleared MULTIRUN names
# must exactly equal the MULTIRUN_* identifiers in native_science_loop_v1.rs
# plus MULTIRUN_EXECUTION_MODE (read in bounded_staleness_async_production);
# the ordinal knob must exist in bridge.rs.
Test-Gate 'A2b-whitelist-matches-rust-source' {
    $sciencePath = Join-Path $repoSrc 'native_science_loop_v1.rs'
    $sourceNames = @{}
    foreach ($match in [regex]::Matches((Get-Content -LiteralPath $sciencePath -Raw), 'MULTIRUN_[A-Z_0-9]+')) {
        $sourceNames[$match.Value] = $true
    }
    $expected = @{}
    foreach ($name in ($script:ValueNames + $script:ClearedNames)) {
        if ($name -clike 'MULTIRUN_*') { $expected[$name] = $true }
    }
    foreach ($name in $sourceNames.Keys) {
        if (-not $expected.ContainsKey($name)) { throw "pilot reads $name but the whitelist does not manage it" }
    }
    foreach ($name in $expected.Keys) {
        if ($name -ceq 'MULTIRUN_EXECUTION_MODE') { continue }
        if (-not $sourceNames.ContainsKey($name)) { throw "whitelist manages $name but the pilot source does not read it" }
    }
    $asyncPath = Join-Path $repoSrc 'bounded_staleness_async_production_v1.rs'
    if ((Get-Content -LiteralPath $asyncPath -Raw) -cnotmatch 'MULTIRUN_EXECUTION_MODE') { throw 'MULTIRUN_EXECUTION_MODE not found in async production module' }
    $bridgePath = Join-Path $repoSrc 'experimental_burn_net8_packed_v1\bridge.rs'
    if ((Get-Content -LiteralPath $bridgePath -Raw) -cnotmatch 'MTG_KERNEL_PILOT_CUDA_ORDINAL') { throw 'ordinal knob not found in bridge.rs' }
}

# --- Capture reproducibility: rerunning the capture harness must reproduce
# the committed golden byte-for-byte (proves the fixture is derived, not
# hand-authored). Reads the certified artifacts on this host.
Test-Gate 'A1-golden-reproducibility' {
    $scratch = Join-Path $env:TEMP ("oppoint-golden-repro-{0}.json" -f [guid]::NewGuid().ToString('n'))
    try {
        & (Join-Path $PSScriptRoot 'capture_a1_golden.ps1') -OutputPath $scratch | Out-Null
        $committed = Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'a1-golden-legacy-env.json') -Algorithm SHA256
        $regenerated = Get-FileHash -LiteralPath $scratch -Algorithm SHA256
        if ($committed.Hash -cne $regenerated.Hash) { throw 'regenerated golden differs from committed fixture' }
    }
    finally { if (Test-Path -LiteralPath $scratch) { Remove-Item -LiteralPath $scratch -Force -Confirm:$false } }
}

# --- Validator negative checks: every validation rule must fail closed.
Test-Gate 'validator-fail-closed' {
    $base = Get-Content -LiteralPath (Join-Path $configsDir 'leg-a-solo.json') -Raw | ConvertFrom-Json
    $cases = @(
        @{ name = 'unknown-key';  mutate = { param($c) $c | Add-Member -NotePropertyName 'extra' -NotePropertyValue 1 } },
        @{ name = 'unknown-key-case'; raw_replace = @('"schema"', '"Schema"') },
        @{ name = 'missing-key';  mutate = { param($c) $c.PSObject.Properties.Remove('base_seed') } },
        @{ name = 'wrong-schema-id'; mutate = { param($c) $c.schema = 'multirun-operating-point-launch/v2' } },
        @{ name = 'label-uppercase'; mutate = { param($c) $c.label = 'Leg-A-Solo' } },
        @{ name = 'updates-zero'; mutate = { param($c) $c.updates = 0 } },
        @{ name = 'runs-zero';    mutate = { param($c) $c.processes[0].runs = 0 } },
        @{ name = 'workers-17';   mutate = { param($c) $c.workers = 17 } },
        @{ name = 'sessions-65';  mutate = { param($c) $c.sessions = 65 } },
        @{ name = 'broker-gt-actors'; mutate = { param($c) $c.broker_target = 65 } },
        @{ name = 'actors-not-64'; mutate = { param($c) $c.sessions = 16 } },
        @{ name = 'short-sha';    mutate = { param($c) $c.expected_commit = 'f986378' } },
        @{ name = 'uppercase-sha'; mutate = { param($c) $c.expected_commit = $c.expected_commit.ToUpperInvariant() } },
        @{ name = 'uppercase-uuid'; mutate = { param($c) $c.processes[0].device_uuid = $c.processes[0].device_uuid.ToUpperInvariant() } },
        @{ name = 'dup-uuid';     mutate = { param($c) $c.processes = @($c.processes[0], $c.processes[0]) } },
        @{ name = 'drive-relative-root'; mutate = { param($c) $c.store_parent_root = 'D:relative\path' } },
        @{ name = 'empty-processes'; mutate = { param($c) $c.processes = @() } },
        @{ name = 'seed-overflow'; mutate = { param($c) $c.base_seed = 9223372036854775806 } },
        @{ name = 'monitor-missing'; mutate = { param($c) $c.PSObject.Properties.Remove('monitor') } },
        @{ name = 'monitor-unknown-key'; mutate = { param($c) $c.monitor | Add-Member -NotePropertyName 'extra' -NotePropertyValue 1 } },
        @{ name = 'monitor-census-61'; mutate = { param($c) $c.monitor.census_seconds = 61 } },
        @{ name = 'monitor-timeout-low'; mutate = { param($c) $c.monitor.wall_clock_timeout_seconds = 30 } },
        @{ name = 'monitor-rail-missing-device'; mutate = { param($c) $c.monitor.device_rails = @() } },
        @{ name = 'monitor-rail-unused-device'; mutate = { param($c) $c.monitor.device_rails = @($c.monitor.device_rails[0], [pscustomobject]@{ device_uuid = 'GPU-0642d3ca-e3d4-ba16-96ab-c561c6da90e3'; rail_mib = 5393 }) } }
    )
    $scratch = Join-Path $env:TEMP ("oppoint-gate-negative-{0}.json" -f [guid]::NewGuid().ToString('n'))
    try {
        foreach ($case in $cases) {
            if ($case.ContainsKey('raw_replace')) {
                # Case-variant keys cannot be built through PowerShell's
                # case-insensitive member bag; mutate the JSON text directly.
                $text = ($base | ConvertTo-Json -Depth 8).Replace($case.raw_replace[0], $case.raw_replace[1])
                $text | Out-File -LiteralPath $scratch -Encoding utf8
            }
            else {
                $mutant = $base | ConvertTo-Json -Depth 8 | ConvertFrom-Json
                & $case.mutate $mutant
                ($mutant | ConvertTo-Json -Depth 8) | Out-File -LiteralPath $scratch -Encoding utf8
            }
            $threw = $false
            try { [void](Read-LaunchConfig -ConfigPath $scratch) } catch { $threw = $true }
            if (-not $threw) { throw "negative case '$($case.name)' was accepted" }
        }
    }
    finally { if (Test-Path -LiteralPath $scratch) { Remove-Item -LiteralPath $scratch -Force -Confirm:$false } }
}

Write-Output ('COLD GATES: {0} pass, {1} fail' -f $passes.Count, $failures.Count)
if ($failures.Count -gt 0) { throw ('cold gates failed: ' + ($failures -join '; ')) }
