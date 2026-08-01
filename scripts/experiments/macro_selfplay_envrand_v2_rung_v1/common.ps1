Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:ExpectedGpuUuid = 'GPU-0642d3ca-e3d4-ba16-96ab-c561c6da90e3'
$script:PoolRoot = 'D:\mtg-kernel-ladder-pilot-20260725\pool3'
$script:PoolJson = Join-Path $script:PoolRoot 'pool.json'
$script:InitStore = Join-Path $script:PoolRoot 'primary'
$script:InitGeneration = 384
$script:RunnerTest = 'native_science_loop_v1::windows_science_loop_tests::multirun_pilot_v1'
$script:H2hTest = 'native_science_loop_v1::windows_science_loop_tests::ladder_head_to_head_eval_v1'

function Get-Gpu1Record {
    $rows = & nvidia-smi --query-gpu=index,uuid,memory.used,utilization.gpu --format=csv,noheader,nounits
    if ($LASTEXITCODE -ne 0) {
        throw 'nvidia-smi failed'
    }
    foreach ($row in $rows) {
        $parts = $row.Split(',') | ForEach-Object { $_.Trim() }
        if ($parts[0] -eq '1') {
            return [ordered]@{
                ordinal = 1
                uuid = $parts[1]
                memory_used_mib = [int]$parts[2]
                utilization_percent = [int]$parts[3]
            }
        }
    }
    throw 'GPU ordinal 1 was not reported'
}

function Assert-Gpu1Idle {
    $gpu = Get-Gpu1Record
    if ($gpu.uuid -ne $script:ExpectedGpuUuid) {
        throw "GPU 1 UUID mismatch: $($gpu.uuid)"
    }
    if ($gpu.memory_used_mib -gt 32 -or $gpu.utilization_percent -gt 1) {
        throw "GPU 1 is not idle: $($gpu.memory_used_mib) MiB, $($gpu.utilization_percent)%"
    }
    return $gpu
}

function Get-StoreTreeHash {
    param([Parameter(Mandatory = $true)][string]$Path)

    $resolved = (Resolve-Path -LiteralPath $Path).Path
    $relativePrefix = $resolved.TrimEnd('\') + '\'
    $hasher = [System.Security.Cryptography.IncrementalHash]::CreateHash(
        [System.Security.Cryptography.HashAlgorithmName]::SHA256
    )
    try {
        $files = Get-ChildItem -LiteralPath $resolved -Recurse -File | Sort-Object FullName
        foreach ($file in $files) {
            if (-not $file.FullName.StartsWith($relativePrefix, [StringComparison]::OrdinalIgnoreCase)) {
                throw "file escaped the requested Store root: $($file.FullName)"
            }
            $relative = $file.FullName.Substring($relativePrefix.Length).Replace('\', '/')
            $fileHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $file.FullName).Hash.ToLowerInvariant()
            $frame = "$relative`n$($file.Length)`n$fileHash`n"
            $hasher.AppendData([System.Text.Encoding]::UTF8.GetBytes($frame))
        }
        return [BitConverter]::ToString($hasher.GetHashAndReset()).Replace('-', '').ToLowerInvariant()
    }
    finally {
        $hasher.Dispose()
    }
}

function Get-ReleaseTestExecutable {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$EvidenceRoot
    )

    New-Item -ItemType Directory -Force -Path $EvidenceRoot | Out-Null
    $jsonPath = Join-Path $EvidenceRoot 'cargo-release-build.jsonl'
    $stderrPath = Join-Path $EvidenceRoot 'cargo-release-build.stderr.log'
    Push-Location $RepoRoot
    try {
        $previousErrorAction = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try {
            $jsonLines = @(& cargo test -p mtg-kernel --release --features experimental-burn-net8-packed-cuda-v1 --no-run --message-format=json 2> $stderrPath)
            $cargoExitCode = $LASTEXITCODE
        }
        finally {
            $ErrorActionPreference = $previousErrorAction
        }
        if ($cargoExitCode -ne 0) {
            throw "release build failed; see $stderrPath"
        }
    }
    finally {
        Pop-Location
    }
    $jsonLines | Set-Content -LiteralPath $jsonPath -Encoding utf8
    $executables = foreach ($line in $jsonLines) {
        try {
            $item = $line | ConvertFrom-Json
            if (
                $item.reason -eq 'compiler-artifact' -and
                $item.target.name -eq 'mtg_kernel' -and
                $item.target.kind -contains 'lib' -and
                $null -ne $item.executable
            ) {
                [string]$item.executable
            }
        }
        catch {
        }
    }
    $executable = $executables | Select-Object -Last 1
    if ([string]::IsNullOrWhiteSpace($executable) -or -not (Test-Path -LiteralPath $executable)) {
        throw 'release mtg_kernel lib-test executable was not resolved from Cargo JSON'
    }
    return (Resolve-Path -LiteralPath $executable).Path
}

function Get-ToolchainRecord {
    $rustc = (& rustc -Vv) -join "`n"
    $cargo = (& cargo -V) -join "`n"
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) {
        throw 'vswhere.exe is required to identify the linker'
    }
    $linker = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find 'VC\Tools\MSVC\**\bin\Hostx64\x64\link.exe' |
        Select-Object -First 1
    if ([string]::IsNullOrWhiteSpace($linker) -or -not (Test-Path -LiteralPath $linker)) {
        throw 'MSVC link.exe was not resolved'
    }
    $linkerOutput = (& $linker 2>&1 | Select-Object -First 2) -join ' '
    return [ordered]@{
        rustc_vv = $rustc
        cargo_v = $cargo
        linker_path = $linker
        linker_banner = $linkerOutput
    }
}

function Invoke-MacroTrainingRun {
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][uint64]$Seed,
        [Parameter(Mandatory = $true)][uint64]$Updates,
        [Parameter(Mandatory = $true)][string]$StoreParent,
        [Parameter(Mandatory = $true)][string]$LogPath
    )

    if (Test-Path -LiteralPath $StoreParent) {
        throw "refusing to reuse Store parent: $StoreParent"
    }
    New-Item -ItemType Directory -Force -Path $StoreParent | Out-Null
    $env:MULTIRUN_RUNS = '1'
    $env:MULTIRUN_UPDATES = [string]$Updates
    $env:MULTIRUN_WORKERS = '2'
    $env:MULTIRUN_SESSIONS = '32'
    $env:MULTIRUN_BROKER_TARGET = '16'
    $env:MULTIRUN_RECORD_ONLY = '1'
    $env:MULTIRUN_BASE_SEED = [string]$Seed
    $env:MULTIRUN_SEED_OFFSET = '0'
    $env:MULTIRUN_STORE_PARENT = $StoreParent
    $env:MULTIRUN_LADDER = '1'
    $env:MULTIRUN_LADDER_INIT_STORE = $script:InitStore
    $env:MULTIRUN_LADDER_INIT_GEN = [string]$script:InitGeneration
    $env:MULTIRUN_LADDER_POOL_DIR = $script:PoolRoot
    $env:MULTIRUN_ENVIRONMENT_RANDOMIZATION_V2 = '1'
    $env:MTG_KERNEL_PILOT_CUDA_ORDINAL = '1'
    try {
        & $Executable $script:RunnerTest --ignored --exact --nocapture --test-threads=1 2>&1 |
            Tee-Object -FilePath $LogPath
        $exitCode = $LASTEXITCODE
    }
    finally {
        @(
            'MULTIRUN_RUNS', 'MULTIRUN_UPDATES', 'MULTIRUN_WORKERS',
            'MULTIRUN_SESSIONS', 'MULTIRUN_BROKER_TARGET', 'MULTIRUN_RECORD_ONLY',
            'MULTIRUN_BASE_SEED', 'MULTIRUN_SEED_OFFSET', 'MULTIRUN_STORE_PARENT',
            'MULTIRUN_LADDER', 'MULTIRUN_LADDER_INIT_STORE',
            'MULTIRUN_LADDER_INIT_GEN', 'MULTIRUN_LADDER_POOL_DIR',
            'MULTIRUN_ENVIRONMENT_RANDOMIZATION_V2', 'MTG_KERNEL_PILOT_CUDA_ORDINAL'
        ) | ForEach-Object { Remove-Item -Path "Env:$_" -ErrorAction SilentlyContinue }
    }
    if ($exitCode -ne 0) {
        throw "macro training runner failed with exit code $exitCode; see $LogPath"
    }
    $text = Get-Content -LiteralPath $LogPath -Raw
    if ($text -notmatch 'MULTIRUN CONFIG .*envrand_v2=true') {
        throw 'positive envrand-v2 runner marker is absent'
    }
    if ($text -notmatch "MULTIRUN AGGREGATE runs=1 episodes=$([uint64]64 * $Updates)") {
        throw 'positive aggregate completion marker is absent'
    }
}

function Invoke-MacroH2hEvaluation {
    param(
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string]$CandidateStore,
        [Parameter(Mandatory = $true)][uint64]$CandidateSeed,
        [Parameter(Mandatory = $true)][uint64]$EvaluationSeed,
        [Parameter(Mandatory = $true)][string]$LogPath
    )

    $env:H2H_CANDIDATE_STORE_ROOT = $CandidateStore
    $env:H2H_CANDIDATE_GEN = '512'
    $env:H2H_CANDIDATE_BASE_SEED = [string]$CandidateSeed
    $env:H2H_CANDIDATE_POOL_JSON = $script:PoolJson
    $env:H2H_UPDATES = '512'
    $env:H2H_INIT_STORE = $script:InitStore
    $env:H2H_INIT_GEN = [string]$script:InitGeneration
    $env:H2H_OPPONENT_STORE_ROOT = $script:InitStore
    $env:H2H_OPPONENT_GEN = [string]$script:InitGeneration
    $env:H2H_PAIRS = '1024'
    $env:H2H_EVAL_SEED = [string]$EvaluationSeed
    $env:H2H_ENVIRONMENT_RANDOMIZATION_V2 = '1'
    try {
        & $Executable $script:H2hTest --ignored --exact --nocapture --test-threads=1 2>&1 |
            Tee-Object -FilePath $LogPath
        $exitCode = $LASTEXITCODE
    }
    finally {
        @(
            'H2H_CANDIDATE_STORE_ROOT', 'H2H_CANDIDATE_GEN',
            'H2H_CANDIDATE_BASE_SEED', 'H2H_CANDIDATE_POOL_JSON', 'H2H_UPDATES',
            'H2H_INIT_STORE', 'H2H_INIT_GEN', 'H2H_OPPONENT_STORE_ROOT',
            'H2H_OPPONENT_GEN', 'H2H_PAIRS', 'H2H_EVAL_SEED',
            'H2H_ENVIRONMENT_RANDOMIZATION_V2'
        ) | ForEach-Object { Remove-Item -Path "Env:$_" -ErrorAction SilentlyContinue }
    }
    if ($exitCode -ne 0) {
        throw "head-to-head evaluator failed with exit code $exitCode; see $LogPath"
    }
    $text = Get-Content -LiteralPath $LogPath -Raw
    if ($text -notmatch 'H2H opponent_resolved_gen=384 pinned=true') {
        throw 'the opponent did not resolve to pinned generation 384'
    }
    if ($text -notmatch 'H2H envrand_v2=true') {
        throw 'the evaluator did not report envrand-v2'
    }
    $match = [regex]::Match($text, 'H2H candidate_gen=512 W/L/D (\d+)/(\d+)/(\d+) of 2048')
    if (-not $match.Success) {
        throw 'the 2,048-game result marker is absent'
    }
    return [ordered]@{
        seed = $CandidateSeed
        wins = [uint64]$match.Groups[1].Value
        losses = [uint64]$match.Groups[2].Value
        draws = [uint64]$match.Groups[3].Value
        passes_55_percent = ([uint64]$match.Groups[1].Value -ge 1127)
        log = $LogPath
    }
}
