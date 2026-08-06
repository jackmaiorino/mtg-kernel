Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:ExpectedBaseCommit = '308842554b1cbca7ea091b154e8a33addeea995d'
$script:BaselineRepoRoot = 'C:\Users\Jack\IdeaProjects\mtg-kernel-macro-selfplay-rung-v1-codex'
$script:ExpectedGpu0Uuid = 'GPU-3502709e-6aef-8ed7-4abe-562838793e3d'
$script:ExpectedGpu1Uuid = 'GPU-0642d3ca-e3d4-ba16-96ab-c561c6da90e3'
$script:PoolRoot = 'D:\mtg-kernel-ladder-pilot-20260725\pool3'
$script:PoolJson = Join-Path $script:PoolRoot 'pool.json'
$script:InitStore = Join-Path $script:PoolRoot 'primary'
$script:InitGeneration = 384
$script:RunnerTest = 'native_science_loop_v1::windows_science_loop_tests::multirun_pilot_v1'
$script:RequiredCloseReopenMarker = 'STORE CLOSE_REOPEN'
$script:PolicyAnchorAuthorityFileName = 'policy-anchor-authority.json'
$script:EnvironmentNames = @(
    'MULTIRUN_RUNS', 'MULTIRUN_UPDATES', 'MULTIRUN_WORKERS',
    'MULTIRUN_SESSIONS', 'MULTIRUN_BROKER_TARGET', 'MULTIRUN_RECORD_ONLY',
    'MULTIRUN_BASE_SEED', 'MULTIRUN_SEED_OFFSET', 'MULTIRUN_STORE_PARENT',
    'MULTIRUN_LADDER', 'MULTIRUN_LADDER_INIT_STORE',
    'MULTIRUN_LADDER_INIT_GEN', 'MULTIRUN_LADDER_POOL_DIR',
    'MULTIRUN_ENVIRONMENT_RANDOMIZATION_V2', 'MULTIRUN_POLICY_ANCHOR_BETA',
    'MULTIRUN_STOP_AFTER_GENERATION', 'MULTIRUN_EXPECT_RESUME_GENERATION',
    'MTG_KERNEL_PILOT_CUDA_ORDINAL', 'CUDA_DEVICE_ORDER', 'CUDA_VISIBLE_DEVICES'
)

function Assert-LastExitCode {
    param([Parameter(Mandatory = $true)][int]$ExitCode, [Parameter(Mandatory = $true)][string]$Label)
    if ($ExitCode -ne 0) {
        throw "$Label failed with exit code $ExitCode"
    }
}

function Get-TextSha256 {
    param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Text)
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
    $hasher = [System.Security.Cryptography.SHA256]::Create()
    try {
        $hash = $hasher.ComputeHash($bytes)
        return ([BitConverter]::ToString($hash)).Replace('-', '').ToLowerInvariant()
    }
    finally {
        $hasher.Dispose()
    }
}

function Get-GitRecord {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [switch]$RequireExactBase
    )

    $safe = $RepoRoot.Replace('\', '/')
    $status = @(& git -c "safe.directory=$safe" -C $RepoRoot status --porcelain 2>&1)
    Assert-LastExitCode $LASTEXITCODE 'git status'
    $head = (@(& git -c "safe.directory=$safe" -C $RepoRoot rev-parse HEAD 2>&1) -join "`n").Trim()
    Assert-LastExitCode $LASTEXITCODE 'git rev-parse'
    & git -c "safe.directory=$safe" -C $RepoRoot merge-base --is-ancestor $script:ExpectedBaseCommit $head
    Assert-LastExitCode $LASTEXITCODE 'git base ancestry'
    if ($RequireExactBase -and $head -ne $script:ExpectedBaseCommit) {
        throw "unexpected baseline HEAD: $head; expected $script:ExpectedBaseCommit"
    }
    if ($status.Count -ne 0) {
        throw "preflight requires a clean worktree at $RepoRoot"
    }
    $diff = @(& git -c "safe.directory=$safe" -C $RepoRoot diff --binary HEAD 2>&1)
    Assert-LastExitCode $LASTEXITCODE 'git diff'
    return [ordered]@{
        commit = $head
        base_commit = $script:ExpectedBaseCommit
        base_is_ancestor = $true
        dirty = $false
        status_sha256 = Get-TextSha256 (($status -join "`n"))
        worktree_diff_sha256 = Get-TextSha256 (($diff -join "`n"))
    }
}

function Get-ToolchainRecord {
    $rustcLines = @(& rustc -Vv 2>&1 | ForEach-Object { $_.ToString() })
    Assert-LastExitCode $LASTEXITCODE 'rustc -Vv'
    $cargoLines = @(& cargo -Vv 2>&1 | ForEach-Object { $_.ToString() })
    Assert-LastExitCode $LASTEXITCODE 'cargo -Vv'
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) {
        throw 'vswhere.exe is required to identify the linker'
    }
    $linkerLines = @(& $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find 'VC\Tools\MSVC\**\bin\Hostx64\x64\link.exe' 2>&1)
    Assert-LastExitCode $LASTEXITCODE 'vswhere linker discovery'
    $linker = $linkerLines | Select-Object -First 1
    if ([string]::IsNullOrWhiteSpace($linker) -or -not (Test-Path -LiteralPath $linker)) {
        throw 'MSVC link.exe was not resolved'
    }
    $linkerPath = (Resolve-Path -LiteralPath $linker).Path
    $item = Get-Item -LiteralPath $linkerPath
    return [ordered]@{
        rustc_vv = ($rustcLines -join "`n")
        cargo_vv = ($cargoLines -join "`n")
        linker_path = $linkerPath
        linker_file_version = $item.VersionInfo.FileVersion
        linker_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $linkerPath).Hash.ToLowerInvariant()
        linker_banner = $item.VersionInfo.FileDescription
    }
}

function Get-CudaRecord {
    $driverLines = @(nvidia-smi --query-gpu=index,uuid,driver_version --format=csv,noheader,nounits 2>&1)
    Assert-LastExitCode $LASTEXITCODE 'nvidia-smi driver query'
    $nvccLines = @(& nvcc --version 2>&1 | ForEach-Object { $_.ToString() })
    Assert-LastExitCode $LASTEXITCODE 'nvcc --version'
    return [ordered]@{
        driver_rows = @($driverLines | ForEach-Object { $_.ToString() })
        nvcc_version = ($nvccLines -join "`n")
    }
}

function Get-ReleaseTestExecutable {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$EvidenceRoot,
        [Parameter(Mandatory = $true)][ValidatePattern('^[a-z0-9-]+$')][string]$Label
    )

    New-Item -ItemType Directory -Force -Path $EvidenceRoot | Out-Null
    $jsonPath = Join-Path $EvidenceRoot "cargo-release-build-$Label.jsonl"
    $stderrPath = Join-Path $EvidenceRoot "cargo-release-build-$Label.stderr.log"
    Push-Location $RepoRoot
    try {
        $previous = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try {
            $jsonLines = @(& cargo test -p mtg-kernel --release --features experimental-burn-net8-packed-cuda-v1 --lib --no-run --message-format=json 2> $stderrPath)
            $cargoExit = $LASTEXITCODE
        }
        finally {
            $ErrorActionPreference = $previous
        }
        Assert-LastExitCode $cargoExit "cargo release build; see $stderrPath"
    }
    finally {
        Pop-Location
    }
    $jsonLines | Set-Content -LiteralPath $jsonPath -Encoding utf8
    $executables = foreach ($line in $jsonLines) {
        try {
            $item = $line | ConvertFrom-Json
            if ($item.reason -eq 'compiler-artifact' -and $item.target.name -eq 'mtg_kernel' -and $item.target.kind -contains 'lib' -and $null -ne $item.executable) {
                [string]$item.executable
            }
        }
        catch {
        }
    }
    $executable = $executables | Select-Object -Last 1
    if ([string]::IsNullOrWhiteSpace($executable) -or -not (Test-Path -LiteralPath $executable)) {
        throw 'release mtg_kernel test executable was not resolved from Cargo JSON'
    }
    return (Resolve-Path -LiteralPath $executable).Path
}

function Get-GpuSnapshot {
    $rows = @(nvidia-smi --query-gpu=index,uuid,name,memory.used,memory.total,utilization.gpu --format=csv,noheader,nounits 2>&1)
    Assert-LastExitCode $LASTEXITCODE 'nvidia-smi GPU query'
    $result = foreach ($row in $rows) {
        $parts = @($row.ToString().Split(',') | ForEach-Object { $_.Trim() })
        if ($parts.Count -ne 6) {
            throw "unexpected nvidia-smi row: $row"
        }
        [ordered]@{
            ordinal = [int]$parts[0]
            uuid = $parts[1]
            name = $parts[2]
            memory_used_mib = [int]$parts[3]
            memory_total_mib = [int]$parts[4]
            utilization_percent = [int]$parts[5]
        }
    }
    return @($result)
}

function Get-GpuRecord {
    param([Parameter(Mandatory = $true)][int]$Ordinal)
    $gpu = Get-GpuSnapshot | Where-Object { $_.ordinal -eq $Ordinal } | Select-Object -First 1
    if ($null -eq $gpu) {
        throw "GPU ordinal $Ordinal was not reported"
    }
    return $gpu
}

function Assert-Gpu1Idle {
    $gpu = Get-GpuRecord -Ordinal 1
    if ($gpu.uuid -ne $script:ExpectedGpu1Uuid) {
        throw "GPU 1 UUID mismatch: $($gpu.uuid)"
    }
    if ($gpu.memory_used_mib -gt 32 -or $gpu.utilization_percent -gt 1) {
        throw "GPU 1 is not idle: $($gpu.memory_used_mib) MiB, $($gpu.utilization_percent)%"
    }
    return $gpu
}

function Assert-GpuIdentity {
    param([Parameter(Mandatory = $true)][ValidateSet(0, 1)][int]$Ordinal)
    $gpu = Get-GpuRecord -Ordinal $Ordinal
    $expected = if ($Ordinal -eq 0) { $script:ExpectedGpu0Uuid } else { $script:ExpectedGpu1Uuid }
    if ($gpu.uuid -ne $expected) {
        throw "GPU $Ordinal UUID mismatch: $($gpu.uuid); expected $expected"
    }
    return $gpu
}

function Assert-NoForeignGpu1ComputeProcesses {
    $rows = @(nvidia-smi --query-compute-apps=pid,gpu_uuid,used_memory --format=csv,noheader,nounits 2>&1)
    Assert-LastExitCode $LASTEXITCODE 'nvidia-smi compute-process query'
    $active = @($rows | Where-Object {
        $text = $_.ToString().Trim()
        -not [string]::IsNullOrWhiteSpace($text) -and
            $text -notmatch '^No running processes found' -and
            $text -match [regex]::Escape($script:ExpectedGpu1Uuid)
    })
    if ($active.Count -ne 0) {
        throw "foreign GPU 1 compute processes are present: $($active -join '; ')"
    }
}

function Assert-PrelaunchResourceWindow {
    $sample = Get-ResourceSample
    $gpu0 = $sample.gpus | Where-Object { $_.ordinal -eq 0 } | Select-Object -First 1
    if ($sample.cpu_total_percent -gt 25.0) {
        throw "prelaunch host CPU exceeds 25 percent: $($sample.cpu_total_percent)"
    }
    if ($sample.host_memory_total_mib -le 0 -or ($sample.host_memory_used_mib / [double]$sample.host_memory_total_mib) -gt 0.90) {
        throw 'prelaunch host memory exceeds 90 percent'
    }
    if ($null -eq $gpu0 -or $gpu0.utilization_percent -gt 20) {
        throw 'prelaunch GPU 0 desktop load exceeds 20 percent or is unavailable'
    }
    return $sample
}

function Assert-ExclusiveWindow {
    $known = @(Get-Process | Where-Object { $_.ProcessName -match '^(mtg_kernel|native_science|cargo|rustc)' })
    if ($known.Count -ne 0) {
        throw 'an existing trainer, evaluator, or build process is present'
    }
}

function Get-HostResourceSample {
    $counter = Get-Counter '\Processor(_Total)\% Processor Time' -ErrorAction Stop
    $os = Get-CimInstance Win32_OperatingSystem -ErrorAction Stop
    $processes = @(Get-Process -ErrorAction Stop)
    [ordered]@{
        utc = [DateTimeOffset]::UtcNow.ToString('O')
        cpu_total_percent = [double]$counter.CounterSamples[0].CookedValue
        process_count = $processes.Count
        host_memory_used_mib = [math]::Round(($os.TotalVisibleMemorySize - $os.FreePhysicalMemory) / 1024.0, 1)
        host_memory_total_mib = [math]::Round($os.TotalVisibleMemorySize / 1024.0, 1)
    }
}

function Get-ResourceSample {
    $gpus = Get-GpuSnapshot
    $hostSample = Get-HostResourceSample
    $gpu0 = $gpus | Where-Object { $_.ordinal -eq 0 } | Select-Object -First 1
    $hostSample['gpus'] = @($gpus)
    $hostSample['gpu0_desktop_load_percent'] = if ($null -eq $gpu0) { $null } else { $gpu0.utilization_percent }
    return $hostSample
}

function Get-StoreTreeHash {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        throw "Store root is missing: $Path"
    }
    $resolved = (Resolve-Path -LiteralPath $Path).Path
    $prefix = $resolved.TrimEnd('\') + '\'
    $hasher = [System.Security.Cryptography.IncrementalHash]::CreateHash([System.Security.Cryptography.HashAlgorithmName]::SHA256)
    try {
        foreach ($file in (Get-ChildItem -LiteralPath $resolved -Recurse -File | Sort-Object FullName)) {
            if (-not $file.FullName.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
                throw "file escaped Store root: $($file.FullName)"
            }
            $relative = $file.FullName.Substring($prefix.Length).Replace('\', '/')
            $fileHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $file.FullName).Hash.ToLowerInvariant()
            $frame = "$relative`n$($file.Length)`n$fileHash`n"
            $hasher.AppendData([System.Text.Encoding]::UTF8.GetBytes($frame))
        }
        return ([BitConverter]::ToString($hasher.GetHashAndReset())).Replace('-', '').ToLowerInvariant()
    }
    finally {
        $hasher.Dispose()
    }
}

function Get-StoreFileInventory {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        throw "Store root is missing: $Path"
    }
    $resolved = (Resolve-Path -LiteralPath $Path).Path
    $prefix = $resolved.TrimEnd('\') + '\'
    return @(
        foreach ($file in (Get-ChildItem -LiteralPath $resolved -Recurse -File | Sort-Object FullName)) {
            if (-not $file.FullName.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
                throw "file escaped Store root: $($file.FullName)"
            }
            [ordered]@{
                path = $file.FullName.Substring($prefix.Length).Replace('\', '/')
                bytes = $file.Length
                sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $file.FullName).Hash.ToLowerInvariant()
            }
        }
    )
}

function Get-InputRecord {
    $files = [ordered]@{
        pool_json = $script:PoolJson
        init_checkpoint = Join-Path $script:InitStore 'checkpoints\update-00000384.checkpoint.json'
        init_sidecar = Join-Path $script:InitStore 'checkpoints\update-00000384.sidecar.json'
        init_state = Join-Path $script:InitStore 'checkpoints\update-00000384.state.f32le'
    }
    $record = [ordered]@{}
    foreach ($entry in $files.GetEnumerator()) {
        if (-not (Test-Path -LiteralPath $entry.Value -PathType Leaf)) {
            throw "required input is missing: $($entry.Value)"
        }
        $record["$($entry.Key)_path"] = $entry.Value
        $record["$($entry.Key)_sha256"] = (Get-FileHash -Algorithm SHA256 -LiteralPath $entry.Value).Hash.ToLowerInvariant()
    }
    return $record
}

function Get-PolicyAnchorAuthorityRecord {
    param([Parameter(Mandatory = $true)][ValidateSet('0', '0.01', '0.03', '0.1', '0.3')][string]$PolicyAnchorBeta)
    $inputs = Get-InputRecord
    return [ordered]@{
        schema = 'regularized-continuation-policy-anchor-authority/v1'
        beta = $PolicyAnchorBeta
        init_generation = $script:InitGeneration
        parent_checkpoint_sha256 = $inputs.init_checkpoint_sha256
        parent_sidecar_sha256 = $inputs.init_sidecar_sha256
        parent_state_sha256 = $inputs.init_state_sha256
        pool_json_sha256 = $inputs.pool_json_sha256
    }
}

function Assert-OrCreatePolicyAnchorAuthority {
    param(
        [Parameter(Mandatory = $true)][string]$StoreParent,
        [Parameter(Mandatory = $true)][ValidateSet('0', '0.01', '0.03', '0.1', '0.3')][string]$PolicyAnchorBeta,
        [switch]$ResumeExistingStore
    )
    $path = Join-Path $StoreParent $script:PolicyAnchorAuthorityFileName
    $expected = Get-PolicyAnchorAuthorityRecord -PolicyAnchorBeta $PolicyAnchorBeta
    if ($ResumeExistingStore) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "resume is missing policy-anchor authority: $path"
        }
        $actual = Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
        foreach ($field in @('schema', 'beta', 'init_generation', 'parent_checkpoint_sha256', 'parent_sidecar_sha256', 'parent_state_sha256', 'pool_json_sha256')) {
            if ([string]$actual.$field -ne [string]$expected[$field]) {
                throw "resume policy-anchor authority mismatch for $field"
            }
        }
    }
    else {
        Write-JsonFile -Value $expected -Path $path
    }
    return [ordered]@{
        path = $path
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
        beta = $PolicyAnchorBeta
    }
}

function Set-NativeEnvironment {
    param(
        [Parameter(Mandatory = $true)][uint64]$Seed,
        [Parameter(Mandatory = $true)][uint64]$Updates,
        [Parameter(Mandatory = $true)][string]$StoreParent,
        [Parameter(Mandatory = $true)][int]$GpuOrdinal,
        [ValidateSet('0', '0.01', '0.03', '0.1', '0.3')][string]$PolicyAnchorBeta = '0',
        [Nullable[uint64]]$StopAfterGeneration,
        [Nullable[uint64]]$ExpectedResumeGeneration,
        [switch]$ResumeExistingStore
    )
    $storeExists = Test-Path -LiteralPath $StoreParent
    if ($ResumeExistingStore) {
        if (-not $storeExists) {
            throw "resume Store parent does not exist: $StoreParent"
        }
    }
    elseif ($storeExists) {
        throw "refusing to reuse Store parent: $StoreParent"
    }
    else {
        New-Item -ItemType Directory -Force -Path $StoreParent | Out-Null
    }
    $gpu = Assert-GpuIdentity -Ordinal $GpuOrdinal
    Assert-OrCreatePolicyAnchorAuthority -StoreParent $StoreParent -PolicyAnchorBeta $PolicyAnchorBeta -ResumeExistingStore:$ResumeExistingStore | Out-Null
    $saved = @{}
    foreach ($name in $script:EnvironmentNames) {
        $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
    }
    $values = @{
        MULTIRUN_RUNS = '1'; MULTIRUN_UPDATES = [string]$Updates; MULTIRUN_WORKERS = '2'
        MULTIRUN_SESSIONS = '32'; MULTIRUN_BROKER_TARGET = '16'; MULTIRUN_RECORD_ONLY = '1'
        MULTIRUN_BASE_SEED = [string]$Seed; MULTIRUN_SEED_OFFSET = '0'; MULTIRUN_STORE_PARENT = $StoreParent
        MULTIRUN_LADDER = '1'; MULTIRUN_LADDER_INIT_STORE = $script:InitStore
        MULTIRUN_LADDER_INIT_GEN = [string]$script:InitGeneration; MULTIRUN_LADDER_POOL_DIR = $script:PoolRoot
        MULTIRUN_ENVIRONMENT_RANDOMIZATION_V2 = '1'; MULTIRUN_POLICY_ANCHOR_BETA = $PolicyAnchorBeta
        MULTIRUN_STOP_AFTER_GENERATION = if ($null -eq $StopAfterGeneration) { $null } else { [string]$StopAfterGeneration }
        MULTIRUN_EXPECT_RESUME_GENERATION = if ($null -eq $ExpectedResumeGeneration) { $null } else { [string]$ExpectedResumeGeneration }
        CUDA_DEVICE_ORDER = 'PCI_BUS_ID'; CUDA_VISIBLE_DEVICES = $gpu.uuid
        MTG_KERNEL_PILOT_CUDA_ORDINAL = '0'
    }
    foreach ($name in $script:EnvironmentNames) {
        [Environment]::SetEnvironmentVariable($name, $values[$name], 'Process')
    }
    return $saved
}

function Restore-NativeEnvironment {
    param([Parameter(Mandatory = $true)][hashtable]$Saved)
    foreach ($name in $script:EnvironmentNames) {
        [Environment]::SetEnvironmentVariable($name, $Saved[$name], 'Process')
    }
}

function Invoke-NativePilot {
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][uint64]$Seed,
        [Parameter(Mandatory = $true)][uint64]$Updates,
        [Parameter(Mandatory = $true)][string]$StoreParent,
        [Parameter(Mandatory = $true)][int]$GpuOrdinal,
        [Parameter(Mandatory = $true)][string]$LogPath,
        [ValidateSet('0', '0.01', '0.03', '0.1', '0.3')][string]$PolicyAnchorBeta = '0',
        [Nullable[uint64]]$StopAfterGeneration,
        [Nullable[uint64]]$ExpectedResumeGeneration,
        [switch]$ResumeExistingStore,
        [switch]$RequirePolicyAnchorMarker
    )
    $saved = Set-NativeEnvironment -Seed $Seed -Updates $Updates -StoreParent $StoreParent -GpuOrdinal $GpuOrdinal -PolicyAnchorBeta $PolicyAnchorBeta -StopAfterGeneration $StopAfterGeneration -ExpectedResumeGeneration $ExpectedResumeGeneration -ResumeExistingStore:$ResumeExistingStore
    try {
        $previous = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try {
            & $Executable $script:RunnerTest --ignored --exact --nocapture --test-threads=1 2>&1 |
                Tee-Object -FilePath $LogPath |
                Out-Null
            $nativeExit = $LASTEXITCODE
        }
        finally {
            $ErrorActionPreference = $previous
        }
        Assert-LastExitCode $nativeExit 'native pilot'
    }
    finally {
        Restore-NativeEnvironment -Saved $saved
    }
    $text = Get-Content -LiteralPath $LogPath -Raw
    if ($text -notmatch 'MULTIRUN CONFIG .*envrand_v2=true') {
        throw 'envrand-v2 runner marker is absent'
    }
    $escapedBeta = [regex]::Escape($PolicyAnchorBeta)
    if ($RequirePolicyAnchorMarker -and $text -notmatch "policy_anchor_beta=$escapedBeta(?:\s|$)") {
        throw 'beta-zero identity marker is absent'
    }
    $startGeneration = if ($null -eq $ExpectedResumeGeneration) { [uint64]0 } else { [uint64]$ExpectedResumeGeneration }
    $endGeneration = if ($null -eq $StopAfterGeneration) { $Updates } else { [uint64]$StopAfterGeneration }
    if ($endGeneration -lt $startGeneration) {
        throw "invalid generation interval: $startGeneration to $endGeneration"
    }
    $expectedEpisodes = [uint64]64 * ($endGeneration - $startGeneration)
    if ($text -notmatch "MULTIRUN AGGREGATE runs=1 episodes=$expectedEpisodes") {
        throw 'native aggregate completion marker is absent'
    }
}

function Start-NativeLane {
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][uint64]$Seed,
        [Parameter(Mandatory = $true)][uint64]$Updates,
        [Parameter(Mandatory = $true)][string]$StoreParent,
        [Parameter(Mandatory = $true)][int]$GpuOrdinal,
        [ValidateSet('0', '0.01', '0.03', '0.1', '0.3')][string]$PolicyAnchorBeta = '0',
        [Parameter(Mandatory = $true)][string]$LogPath,
        [Parameter(Mandatory = $true)][string]$EvidenceRoot
    )
    $hostCommand = Get-Command pwsh -ErrorAction SilentlyContinue
    if ($null -eq $hostCommand) {
        $hostCommand = Get-Command powershell -ErrorAction Stop
    }
    $pwsh = $hostCommand.Source
    $laneScript = Join-Path $PSScriptRoot 'run-native.ps1'
    $logLabel = [IO.Path]::GetFileNameWithoutExtension($LogPath)
    $stdout = Join-Path $EvidenceRoot "$logLabel.stdout.log"
    $stderr = Join-Path $EvidenceRoot "$logLabel.stderr.log"
    $completion = Join-Path $EvidenceRoot "$logLabel.completion.json"
    $childArgs = @('-NoProfile', '-WindowStyle', 'Hidden', '-File', $laneScript,
        '-Executable', $Executable, '-Seed', $Seed, '-Updates', $Updates,
        '-StoreParent', $StoreParent, '-GpuOrdinal', $GpuOrdinal,
        '-PolicyAnchorBeta', $PolicyAnchorBeta, '-LogPath', $LogPath,
        '-CompletionPath', $completion)
    $argText = ($childArgs | ForEach-Object { '"' + ([string]$_).Replace('"', '\"') + '"' }) -join ' '
    $started = [DateTimeOffset]::UtcNow
    $process = Start-Process -FilePath $pwsh -ArgumentList $argText -WorkingDirectory $script:RepoRoot -PassThru -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    return [pscustomobject]@{
        process = $process
        gpu_ordinal = $GpuOrdinal
        store_parent = $StoreParent
        log = $LogPath
        stdout = $stdout
        stderr = $stderr
        completion = $completion
        executable = $Executable
        seed = $Seed
        updates = $Updates
        policy_anchor_beta = $PolicyAnchorBeta
        started_utc = $started.ToString('O')
        started = $started
    }
}

function Get-VerifiedNativeLaneCompletion {
    param(
        [Parameter(Mandatory = $true)]$Lane,
        [Parameter(Mandatory = $true)][int]$ProcessId
    )
    if ((Get-Item -LiteralPath $Lane.stderr).Length -ne 0) {
        throw "GPU $($Lane.gpu_ordinal) lane wrote child stderr; see $($Lane.stderr)"
    }
    if (-not (Test-Path -LiteralPath $Lane.completion -PathType Leaf)) {
        throw "GPU $($Lane.gpu_ordinal) lane has no verified child-completion record; see $($Lane.stderr)"
    }
    $completion = Get-Content -LiteralPath $Lane.completion -Raw | ConvertFrom-Json
    if ([string]$completion.schema -ne 'regularized-continuation-native-lane-completion/v1' -or
        $completion.success -ne $true -or
        [int]$completion.process_id -ne $ProcessId -or
        [uint64]$completion.seed -ne [uint64]$Lane.seed -or
        [uint64]$completion.updates -ne [uint64]$Lane.updates -or
        [int]$completion.gpu_ordinal -ne [int]$Lane.gpu_ordinal -or
        [string]$completion.policy_anchor_beta -ne [string]$Lane.policy_anchor_beta -or
        [string]$completion.store_parent -ne [string]$Lane.store_parent -or
        [string]$completion.log_path -ne [string]$Lane.log) {
        throw "GPU $($Lane.gpu_ordinal) lane child-completion record does not match its launch"
    }
    $expectedExecutableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $Lane.executable).Hash.ToLowerInvariant()
    $expectedLogHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $Lane.log).Hash.ToLowerInvariant()
    if ([string]$completion.executable_sha256 -ne $expectedExecutableHash -or
        [string]$completion.log_sha256 -ne $expectedLogHash) {
        throw "GPU $($Lane.gpu_ordinal) lane child-completion hashes do not match"
    }
    $logText = Get-Content -LiteralPath $Lane.log -Raw
    if ($logText -notmatch 'test result: ok\.') {
        throw "GPU $($Lane.gpu_ordinal) lane native test-success marker is absent"
    }
    return [ordered]@{
        path = $Lane.completion
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Lane.completion).Hash.ToLowerInvariant()
    }
}

function Wait-NativeLane {
    param([Parameter(Mandatory = $true)]$Lane)
    $samples = New-Object System.Collections.Generic.List[object]
    try {
        while (-not $Lane.process.HasExited) {
            $samples.Add((Get-ResourceSample))
            Start-Sleep -Seconds 1
        }
        $Lane.process.WaitForExit()
        $Lane.process.Refresh()
        $samples.Add((Get-ResourceSample))
    }
    catch {
        Stop-NativeLane -Lane $Lane
        throw
    }
    $verifiedCompletion = Get-VerifiedNativeLaneCompletion -Lane $Lane -ProcessId $Lane.process.Id
    $finished = [DateTimeOffset]::UtcNow
    return [ordered]@{
        seed = [uint64]$Lane.seed
        gpu_ordinal = $Lane.gpu_ordinal
        store_parent = $Lane.store_parent
        log = $Lane.log
        stdout = $Lane.stdout
        stderr = $Lane.stderr
        completion = $verifiedCompletion
        started_utc = $Lane.started_utc
        completed_utc = $finished.ToString('O')
        wall_seconds = ($finished - $Lane.started).TotalSeconds
        exit_code = 0
        process_exit_code_observed = $Lane.process.ExitCode
        resource_samples = @($samples | ForEach-Object { $_ })
    }
}

function Stop-ProcessTree {
    param(
        [Parameter(Mandatory = $true)][int]$RootProcessId,
        [switch]$SkipRoot
    )
    $all = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)
    $descendants = New-Object System.Collections.Generic.List[int]
    $frontier = @($RootProcessId)
    while ($frontier.Count -ne 0) {
        $next = @()
        foreach ($parent in $frontier) {
            foreach ($child in @($all | Where-Object { [int]$_.ParentProcessId -eq $parent })) {
                $childId = [int]$child.ProcessId
                $descendants.Add($childId)
                $next += $childId
            }
        }
        $frontier = $next
    }
    $ordered = @($descendants)
    [array]::Reverse($ordered)
    foreach ($processId in $ordered) {
        Stop-Process -Id $processId -Force -ErrorAction SilentlyContinue
    }
    if (-not $SkipRoot) {
        Stop-Process -Id $RootProcessId -Force -ErrorAction SilentlyContinue
    }
}

function Stop-NativeLane {
    param($Lane)
    if ($null -ne $Lane) {
        Stop-ProcessTree -RootProcessId ([int]$Lane.process.Id) -SkipRoot:$Lane.process.HasExited
    }
}

function Assert-GenerationCheckpoint {
    param(
        [Parameter(Mandatory = $true)][string]$Store,
        [Parameter(Mandatory = $true)][uint64]$Generation
    )
    $latest = Get-Content -LiteralPath (Join-Path $Store 'latest.json') -Raw | ConvertFrom-Json
    if ([uint64]$latest.generation_index -ne $Generation) {
        throw "Store latest generation is not $Generation`: $($latest.generation_index)"
    }
    foreach ($suffix in @('checkpoint.json', 'sidecar.json', 'state.f32le')) {
        $path = Join-Path $Store (("checkpoints\update-{0:d8}.{{0}}" -f $Generation) -f $suffix)
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "generation-$Generation checkpoint file is missing: $path"
        }
    }
}

function Write-JsonFile {
    param([Parameter(Mandatory = $true)]$Value, [Parameter(Mandatory = $true)][string]$Path)
    $Value | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $Path -Encoding utf8
}

function Write-Utf8NoBomJsonFile {
    param([Parameter(Mandatory = $true)]$Value, [Parameter(Mandatory = $true)][string]$Path)
    $json = $Value | ConvertTo-Json -Depth 12
    [IO.File]::WriteAllText($Path, $json, [Text.UTF8Encoding]::new($false))
}

function New-UniqueAttemptRoot {
    param(
        [Parameter(Mandatory = $true)][string]$EvidenceRoot,
        [Parameter(Mandatory = $true)][string]$GateName
    )
    $gateRoot = Join-Path $EvidenceRoot $GateName
    New-Item -ItemType Directory -Force -Path $gateRoot | Out-Null
    for ($attempt = 1; $attempt -le 999; $attempt++) {
        $candidate = Join-Path $gateRoot ('attempt-{0:d3}' -f $attempt)
        try {
            New-Item -ItemType Directory -Path $candidate -ErrorAction Stop | Out-Null
            return $candidate
        }
        catch {
            if (Test-Path -LiteralPath $candidate -PathType Container) {
                continue
            }
            throw
        }
    }
    throw "no free attempt directory remains under $gateRoot"
}

function Get-PassedIdentityPrerequisite {
    param(
        [Parameter(Mandatory = $true)][string]$EvidenceRoot,
        [Parameter(Mandatory = $true)][string]$CandidateCommit,
        [Parameter(Mandatory = $true)][string]$CandidateExecutableSha256,
        [Parameter(Mandatory = $true)][string]$RepoRoot
    )
    $gateRoot = Join-Path $EvidenceRoot 'beta-zero-identity'
    $latestAttempt = Get-ChildItem -LiteralPath $gateRoot -Directory -ErrorAction Stop |
        Sort-Object Name -Descending |
        Select-Object -First 1
    if ($null -eq $latestAttempt) {
        throw 'throughput gate requires a completed beta-zero identity attempt'
    }
    $manifestPath = Join-Path $latestAttempt.FullName 'identity-manifest.json'
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        throw "latest identity attempt has no manifest: $manifestPath"
    }
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    if ($manifest.passed -ne $true -or $manifest.output_hashes_bit_identical -ne $true -or $manifest.close_reopen_observed -ne $true) {
        throw 'latest beta-zero identity attempt did not pass every prerequisite'
    }
    $sourceCandidateCommit = [string]$manifest.git.candidate.commit
    $sourceExecutableSha256 = [string]$manifest.executables.candidate.sha256
    $transfer = $false
    $transferFiles = @()
    if ($sourceCandidateCommit -ne $CandidateCommit) {
        $safe = $RepoRoot.Replace('\', '/')
        & git -c "safe.directory=$safe" -C $RepoRoot merge-base --is-ancestor $sourceCandidateCommit $CandidateCommit
        Assert-LastExitCode $LASTEXITCODE 'identity prerequisite commit ancestry'
        $transferFiles = @(& git -c "safe.directory=$safe" -C $RepoRoot diff --name-only $sourceCandidateCommit $CandidateCommit 2>&1)
        Assert-LastExitCode $LASTEXITCODE 'identity prerequisite transfer diff'
        $approvedTransferFiles = @(
            'scripts/experiments/regularized_continuation_retest_v1/coefficient-screen.ps1',
            'scripts/experiments/regularized_continuation_retest_v1/common.ps1',
            'scripts/experiments/regularized_continuation_retest_v1/throughput-screen.ps1'
        )
        $sortedTransferFiles = @($transferFiles | Sort-Object)
        if ($sortedTransferFiles.Count -ne $approvedTransferFiles.Count -or
            @(Compare-Object -ReferenceObject $approvedTransferFiles -DifferenceObject $sortedTransferFiles).Count -ne 0) {
            throw "identity prerequisite commit differs outside the approved harness-only fix and transfer wiring: $($transferFiles -join ', ')"
        }
        if ($sourceExecutableSha256 -ne $CandidateExecutableSha256) {
            throw 'identity prerequisite transfer executable SHA mismatch'
        }
        $transfer = $true
    }
    elseif ($sourceExecutableSha256 -ne $CandidateExecutableSha256) {
        throw 'identity prerequisite executable SHA mismatch'
    }
    return [ordered]@{
        manifest_path = $manifestPath
        manifest_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $manifestPath).Hash.ToLowerInvariant()
        candidate_commit = $CandidateCommit
        source_candidate_commit = $sourceCandidateCommit
        candidate_executable_sha256 = $CandidateExecutableSha256
        harness_only_transfer = $transfer
        transfer_files = @($transferFiles)
        store_tree_sha256 = [string]$manifest.outputs.candidate.store_tree_sha256
    }
}

function Get-PassedThroughputPrerequisite {
    param(
        [Parameter(Mandatory = $true)][string]$EvidenceRoot,
        [Parameter(Mandatory = $true)][string]$CandidateCommit,
        [Parameter(Mandatory = $true)][string]$IdentityManifestSha256
    )
    $candidates = @(
        foreach ($gateName in @('throughput-screen', 'throughput-screen-hotfix-v1')) {
            $gateRoot = Join-Path $EvidenceRoot $gateName
            if (Test-Path -LiteralPath $gateRoot -PathType Container) {
                Get-ChildItem -LiteralPath $gateRoot -Directory -ErrorAction Stop |
                    ForEach-Object {
                        $manifestPath = Join-Path $_.FullName 'throughput-manifest.json'
                        if (Test-Path -LiteralPath $manifestPath -PathType Leaf) {
                            [pscustomobject]@{
                                path = $manifestPath
                                item = Get-Item -LiteralPath $manifestPath
                            }
                        }
                    }
            }
        }
    )
    foreach ($candidate in ($candidates | Sort-Object { $_.item.LastWriteTimeUtc } -Descending)) {
        $manifest = Get-Content -LiteralPath $candidate.path -Raw | ConvertFrom-Json
        if ($manifest.passed -eq $true -and
            $manifest.same_device_repeat_bit_identical -eq $true -and
            [string]$manifest.git.commit -eq $CandidateCommit -and
            [string]$manifest.prerequisite_identity.manifest_sha256 -eq $IdentityManifestSha256 -and
            [string]$manifest.selected_topology -in @('gpu0+gpu1', 'gpu1-only')) {
            return [ordered]@{
                manifest_path = $candidate.path
                manifest_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $candidate.path).Hash.ToLowerInvariant()
                candidate_commit = [string]$manifest.git.commit
                identity_manifest_sha256 = [string]$manifest.prerequisite_identity.manifest_sha256
                selected_topology = [string]$manifest.selected_topology
                aggregate_speedup = [double]$manifest.aggregate_speedup
            }
        }
    }
    throw "coefficient screen requires a passed throughput manifest for candidate commit $CandidateCommit and the current identity prerequisite"
}
