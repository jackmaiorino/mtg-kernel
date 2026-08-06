param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][string]$TestName,
    [Parameter(Mandatory = $true)][string]$Label,
    [Parameter(Mandatory = $true)][ValidateSet('candidate', 'control')][string]$Role,
    [Parameter(Mandatory = $true)][uint64]$TrainingSeed,
    [Parameter(Mandatory = $true)][uint64]$Generation,
    [Parameter(Mandatory = $true)][uint64]$PairCount,
    [Parameter(Mandatory = $true)][uint64]$EvaluationSeed,
    [Parameter(Mandatory = $true)][uint64]$Updates,
    [Parameter(Mandatory = $true)][string]$CandidateStoreRoot,
    [Parameter(Mandatory = $true)][string]$PoolJson,
    [Parameter(Mandatory = $true)][string]$InitStore,
    [Parameter(Mandatory = $true)][uint64]$InitGeneration,
    [Parameter(Mandatory = $true)][string]$OpponentStoreRoot,
    [Parameter(Mandatory = $true)][uint64]$OpponentGeneration,
    [Parameter(Mandatory = $true)][string]$OutcomePath,
    [Parameter(Mandatory = $true)][string]$StdoutPath,
    [Parameter(Mandatory = $true)][string]$StderrPath,
    [Parameter(Mandatory = $true)][string]$CompletionPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$h2hEnvironmentNames = @(
    'H2H_CANDIDATE_STORE_ROOT', 'H2H_CANDIDATE_GEN', 'H2H_CANDIDATE_BASE_SEED',
    'H2H_CANDIDATE_POOL_JSON', 'H2H_UPDATES', 'H2H_INIT_STORE', 'H2H_INIT_GEN',
    'H2H_OPPONENT_STORE_ROOT', 'H2H_OPPONENT_GEN', 'H2H_PAIRS', 'H2H_EVAL_SEED',
    'H2H_ENVIRONMENT_RANDOMIZATION_V2', 'H2H_OUTCOME_JSON', 'WIDE'
)

function Clear-H2hEnvironment {
    foreach ($name in $h2hEnvironmentNames) {
        Remove-Item -Path "Env:$name" -ErrorAction SilentlyContinue
    }
}

function Set-H2hEnvironment {
    Clear-H2hEnvironment
    $env:H2H_CANDIDATE_STORE_ROOT = $CandidateStoreRoot
    $env:H2H_CANDIDATE_GEN = [string]$Generation
    $env:H2H_CANDIDATE_BASE_SEED = [string]$TrainingSeed
    $env:H2H_CANDIDATE_POOL_JSON = $PoolJson
    $env:H2H_UPDATES = [string]$Updates
    $env:H2H_INIT_STORE = $InitStore
    $env:H2H_INIT_GEN = [string]$InitGeneration
    $env:H2H_OPPONENT_STORE_ROOT = $OpponentStoreRoot
    $env:H2H_OPPONENT_GEN = [string]$OpponentGeneration
    $env:H2H_PAIRS = [string]$PairCount
    $env:H2H_EVAL_SEED = [string]$EvaluationSeed
    $env:H2H_ENVIRONMENT_RANDOMIZATION_V2 = '1'
    $env:H2H_OUTCOME_JSON = $OutcomePath
}

function Get-Sha256 {
    param([Parameter(Mandatory = $true)][string]$Path)
    (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Write-CreateNewJson {
    param([Parameter(Mandatory = $true)]$Value, [Parameter(Mandatory = $true)][string]$Path)
    $json = $Value | ConvertTo-Json -Depth 12
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($json)
    $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush($true)
    }
    finally {
        $stream.Dispose()
    }
}

try {
    foreach ($path in @($OutcomePath, $StdoutPath, $StderrPath, $CompletionPath)) {
        if (Test-Path -LiteralPath $path) {
            throw "refusing to overwrite arm output: $path"
        }
    }
    if ($Role -eq 'candidate' -and $Generation -notin @(64, 128, 256, 384, 512)) {
        throw "candidate generation is outside the frozen panel: $Generation"
    }
    if ($Role -eq 'control' -and $Generation -notin @(384, 512)) {
        throw "control generation is outside the frozen panel: $Generation"
    }
    if (($PairCount -eq 64 -and $EvaluationSeed -ne 969999) -or
        ($PairCount -in @(512, 2048) -and $EvaluationSeed -ne 982001)) {
        throw "H2H pair-count/evaluation-seed binding is invalid: pairs=$PairCount seed=$EvaluationSeed"
    }
    if ($Updates -ne 512 -or $InitGeneration -ne 384 -or $OpponentGeneration -ne 384) {
        throw 'H2H update/init/opponent generations are not the frozen 512/384/384 values'
    }
    if ($PairCount -notin @(64, 512, 2048)) {
        throw "unexpected H2H pair count: $PairCount"
    }

    Set-H2hEnvironment
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & $Executable $TestName --ignored --exact --nocapture --test-threads=1 1> $StdoutPath 2> $StderrPath
        $nativeExitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previous
        Clear-H2hEnvironment
    }

    $outcomeCreated = Test-Path -LiteralPath $OutcomePath -PathType Leaf
    $stdoutCreated = Test-Path -LiteralPath $StdoutPath -PathType Leaf
    $stderrCreated = Test-Path -LiteralPath $StderrPath -PathType Leaf
    $completion = [ordered]@{
        schema = 'regularized-continuation-full-horizon-eval-arm-completion/v1'
        success = ($nativeExitCode -eq 0 -and $outcomeCreated -and $stdoutCreated -and $stderrCreated)
        wrapper_process_id = $PID
        native_exit_code = $nativeExitCode
        label = $Label
        role = $Role
        training_seed = $TrainingSeed
        generation = $Generation
        pair_count = $PairCount
        evaluation_seed = $EvaluationSeed
        updates = $Updates
        init_generation = $InitGeneration
        opponent_generation = $OpponentGeneration
        candidate_store_root = $CandidateStoreRoot
        pool_json = $PoolJson
        init_store = $InitStore
        opponent_store_root = $OpponentStoreRoot
        environment_randomization_v2 = $true
        worker_count = 2
        sessions_per_worker = 32
        broker_batch_target = 16
        executable_sha256 = Get-Sha256 -Path $Executable
        outcome_created = $outcomeCreated
        outcome_sha256 = if ($outcomeCreated) { Get-Sha256 -Path $OutcomePath } else { $null }
        stdout_sha256 = if ($stdoutCreated) { Get-Sha256 -Path $StdoutPath } else { $null }
        stderr_sha256 = if ($stderrCreated) { Get-Sha256 -Path $StderrPath } else { $null }
        completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
    }
    Write-CreateNewJson -Value $completion -Path $CompletionPath
    if (-not $completion.success) {
        exit 1
    }
    exit 0
}
catch {
    Write-Error $_
    exit 1
}
