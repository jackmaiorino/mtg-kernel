param(
    [string]$EvidenceRoot = 'D:\mtg-kernel-regularized-continuation-retest-v1',
    [string]$CoefficientManifestPath = 'D:\mtg-kernel-regularized-continuation-retest-v1\development\seed-1940001\coefficient-screen\attempt-002\coefficient-manifest.json',
    [uint64]$PreflightPairs = 16,
    [uint64]$FormalPairs = 512
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'common.ps1')

$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
$script:H2hTest = 'native_science_loop_v1::windows_science_loop_tests::ladder_head_to_head_eval_v1'
$script:H2hEnvironmentNames = @(
    'H2H_CANDIDATE_STORE_ROOT', 'H2H_CANDIDATE_GEN', 'H2H_CANDIDATE_BASE_SEED',
    'H2H_CANDIDATE_POOL_JSON', 'H2H_UPDATES', 'H2H_INIT_STORE', 'H2H_INIT_GEN',
    'H2H_OPPONENT_STORE_ROOT', 'H2H_OPPONENT_GEN', 'H2H_PAIRS', 'H2H_EVAL_SEED',
    'H2H_ENVIRONMENT_RANDOMIZATION_V2', 'H2H_OUTCOME_JSON', 'WIDE'
)

function Get-FileRecord {
    param([Parameter(Mandatory = $true)][string]$Path)
    [ordered]@{
        path = (Resolve-Path -LiteralPath $Path).Path
        bytes = (Get-Item -LiteralPath $Path).Length
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
    }
}

function Clear-H2hEnvironment {
    foreach ($name in $script:H2hEnvironmentNames) {
        Remove-Item -Path "Env:$name" -ErrorAction SilentlyContinue
    }
}

function Set-H2hEnvironment {
    param(
        [Parameter(Mandatory = $true)][string]$StoreRoot,
        [Parameter(Mandatory = $true)][string]$OutcomePath,
        [Parameter(Mandatory = $true)][uint64]$EvaluationSeed,
        [Parameter(Mandatory = $true)][uint64]$Pairs
    )
    Clear-H2hEnvironment
    $env:H2H_CANDIDATE_STORE_ROOT = $StoreRoot
    $env:H2H_CANDIDATE_GEN = '32'
    $env:H2H_CANDIDATE_BASE_SEED = '1940001'
    $env:H2H_CANDIDATE_POOL_JSON = $script:PoolJson
    $env:H2H_UPDATES = '32'
    $env:H2H_INIT_STORE = $script:InitStore
    $env:H2H_INIT_GEN = [string]$script:InitGeneration
    $env:H2H_OPPONENT_STORE_ROOT = $script:InitStore
    $env:H2H_OPPONENT_GEN = [string]$script:InitGeneration
    $env:H2H_PAIRS = [string]$Pairs
    $env:H2H_EVAL_SEED = [string]$EvaluationSeed
    $env:H2H_ENVIRONMENT_RANDOMIZATION_V2 = '1'
    $env:H2H_OUTCOME_JSON = $OutcomePath
}

function Start-H2hArm {
    param(
        [Parameter(Mandatory = $true)][string]$Label,
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string]$StoreRoot,
        [Parameter(Mandatory = $true)][string]$RunRoot,
        [Parameter(Mandatory = $true)][uint64]$EvaluationSeed,
        [Parameter(Mandatory = $true)][uint64]$Pairs
    )
    $outcome = Join-Path $RunRoot "$Label-terminal-stream.json"
    $stdout = Join-Path $RunRoot "$Label.stdout.log"
    $stderr = Join-Path $RunRoot "$Label.stderr.log"
    foreach ($path in @($outcome, $stdout, $stderr)) {
        if (Test-Path -LiteralPath $path) {
            throw "arm output already exists: $path"
        }
    }
    Set-H2hEnvironment -StoreRoot $StoreRoot -OutcomePath $outcome -EvaluationSeed $EvaluationSeed -Pairs $Pairs
    try {
        $process = Start-Process -FilePath $Executable -ArgumentList @(
            $script:H2hTest, '--ignored', '--exact', '--nocapture', '--test-threads=1'
        ) -WorkingDirectory $script:RepoRoot -PassThru -WindowStyle Hidden -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    }
    finally {
        Clear-H2hEnvironment
    }
    [ordered]@{
        label = $Label
        process = $process
        process_id = $process.Id
        launched_utc = [DateTimeOffset]::UtcNow.ToString('O')
        store_root = $StoreRoot
        outcome_path = $outcome
        stdout_path = $stdout
        stderr_path = $stderr
    }
}

function Wait-H2hArms {
    param(
        [Parameter(Mandatory = $true)]$Runs,
        [Parameter(Mandatory = $true)][uint64]$TimeoutSeconds
    )
    $started = @($Runs | ForEach-Object { [DateTimeOffset]::Parse([string]$_.launched_utc) } | Sort-Object | Select-Object -First 1)[0]
    $samples = @()
    try {
        while (@($Runs | Where-Object { -not $_.process.HasExited }).Count -ne 0) {
            if (([DateTimeOffset]::UtcNow - $started).TotalSeconds -gt $TimeoutSeconds) {
                throw "head-to-head evaluator watchdog exceeded $TimeoutSeconds seconds"
            }
            $samples += Get-ResourceSample
            Start-Sleep -Seconds 5
            foreach ($run in $Runs) {
                $run.process.Refresh()
            }
        }
        $samples += Get-ResourceSample
    }
    catch {
        foreach ($run in $Runs) {
            if (-not $run.process.HasExited) {
                Stop-Process -Id $run.process.Id -Force -ErrorAction SilentlyContinue
            }
        }
        throw
    }
    $completed = [DateTimeOffset]::UtcNow
    foreach ($run in $Runs) {
        $run.process.WaitForExit()
        if ($run.process.ExitCode -ne 0) {
            throw "$($run.label) evaluator failed with exit code $($run.process.ExitCode); see $($run.stderr_path)"
        }
        if (-not (Test-Path -LiteralPath $run.outcome_path -PathType Leaf)) {
            throw "$($run.label) did not publish its terminal stream"
        }
        $stderrText = Get-Content -LiteralPath $run.stderr_path -Raw
        if (-not [string]::IsNullOrWhiteSpace($stderrText)) {
            throw "$($run.label) wrote unexpected stderr; see $($run.stderr_path)"
        }
    }
    [ordered]@{
        wall_seconds = ($completed - $started).TotalSeconds
        started_utc = $started.ToString('O')
        completed_utc = $completed.ToString('O')
        samples = @($samples)
    }
}

function Stop-H2hArms {
    param([Parameter(Mandatory = $true)]$Runs)
    foreach ($run in $Runs) {
        if ($null -ne $run -and -not $run.process.HasExited) {
            Stop-Process -Id $run.process.Id -Force -ErrorAction SilentlyContinue
        }
    }
}

function Get-ResourceSummary {
    param([Parameter(Mandatory = $true)]$Samples)
    $hostTotal = [double]$Samples[0].host_memory_total_mib
    $used = @($Samples | ForEach-Object { [double]$_.host_memory_used_mib })
    $cpu = @($Samples | ForEach-Object { [double]$_.cpu_total_percent })
    $gpu0 = @($Samples | ForEach-Object { [double]($_.gpus | Where-Object ordinal -eq 0 | Select-Object -First 1).utilization_percent })
    $gpu1 = @($Samples | ForEach-Object { [double]($_.gpus | Where-Object ordinal -eq 1 | Select-Object -First 1).utilization_percent })
    $gpu0Memory = @($Samples | ForEach-Object { [double]($_.gpus | Where-Object ordinal -eq 0 | Select-Object -First 1).memory_used_mib })
    $gpu1Memory = @($Samples | ForEach-Object { [double]($_.gpus | Where-Object ordinal -eq 1 | Select-Object -First 1).memory_used_mib })
    [ordered]@{
        sample_count = @($Samples).Count
        mean_cpu_percent = [math]::Round(($cpu | Measure-Object -Average).Average, 3)
        maximum_cpu_percent = [math]::Round(($cpu | Measure-Object -Maximum).Maximum, 3)
        maximum_host_memory_used_mib = ($used | Measure-Object -Maximum).Maximum
        minimum_host_memory_free_mib = [math]::Round($hostTotal - ($used | Measure-Object -Maximum).Maximum, 1)
        maximum_gpu0_utilization_percent = ($gpu0 | Measure-Object -Maximum).Maximum
        maximum_gpu1_utilization_percent = ($gpu1 | Measure-Object -Maximum).Maximum
        maximum_gpu0_memory_used_mib = ($gpu0Memory | Measure-Object -Maximum).Maximum
        maximum_gpu1_memory_used_mib = ($gpu1Memory | Measure-Object -Maximum).Maximum
    }
}

function Get-ArmRecord {
    param([Parameter(Mandatory = $true)]$Manifest, [Parameter(Mandatory = $true)][string]$Beta)
    $matches = @($Manifest.arms | Where-Object { [string]$_.beta -eq $Beta })
    if ($matches.Count -ne 1) {
        throw "expected exactly one coefficient arm for beta $Beta"
    }
    $arm = $matches[0]
    Assert-GenerationCheckpoint -Store ([string]$arm.store_root) -Generation 32
    $tree = Get-StoreTreeHash ([string]$arm.store_root)
    if ($tree -ne [string]$arm.store_tree_sha256) {
        throw "coefficient Store tree changed for beta $Beta"
    }
    return $arm
}

if ($PreflightPairs -lt 2 -or $FormalPairs -ne 512) {
    throw 'preflight must contain at least two pairs and the frozen formal panel is exactly 512 pairs'
}
if (-not (Test-Path -LiteralPath $CoefficientManifestPath -PathType Leaf)) {
    throw 'the completed Gate 3 coefficient manifest is required'
}
$git = Get-GitRecord -RepoRoot $script:RepoRoot
$toolchain = Get-ToolchainRecord
$cuda = Get-CudaRecord
$coefficient = Get-Content -LiteralPath $CoefficientManifestPath -Raw | ConvertFrom-Json
if ($coefficient.passed -ne $true -or [string]$coefficient.disposition -ne 'SELECTED' -or
    $coefficient.terminal_outcomes_read -ne $false -or [uint64]$coefficient.training_seed -ne 1940001) {
    throw 'Gate 3 did not publish an admissible terminal-blind selection'
}
$selectedBeta = [string]$coefficient.selected_beta
$controlArm = Get-ArmRecord -Manifest $coefficient -Beta '0'
$selectedArm = Get-ArmRecord -Manifest $coefficient -Beta $selectedBeta
$pool = Get-Content -LiteralPath $script:PoolJson -Raw | ConvertFrom-Json
if ([uint64]$pool.primary.generation -ne 384) {
    throw 'Pool3 primary is not pinned to promoted(2) generation 384'
}

$preflightEvidence = Join-Path $EvidenceRoot 'preflight\seed-969999'
$preflightRoot = New-UniqueAttemptRoot -EvidenceRoot $preflightEvidence -GateName 'gross-safety-throughput'
$executable = Get-ReleaseTestExecutable -RepoRoot $script:RepoRoot -EvidenceRoot $preflightRoot -Label 'gross-safety'
$executableSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $executable).Hash.ToLowerInvariant()
$archivedExecutable = Join-Path $preflightRoot "evaluator-$executableSha.exe"
Copy-Item -LiteralPath $executable -Destination $archivedExecutable
Assert-ExclusiveWindow
$preflightPrelaunch = Assert-PrelaunchResourceWindow

$singleRoot = Join-Path $preflightRoot 'single-control'
$concurrentRoot = Join-Path $preflightRoot 'concurrent-arms'
New-Item -ItemType Directory -Path $singleRoot | Out-Null
New-Item -ItemType Directory -Path $concurrentRoot | Out-Null
$singleRun = Start-H2hArm -Label 'control' -Executable $archivedExecutable -StoreRoot ([string]$controlArm.store_root) -RunRoot $singleRoot -EvaluationSeed 969999 -Pairs $PreflightPairs
$singleMeasure = Wait-H2hArms -Runs @($singleRun) -TimeoutSeconds 900
$concurrentControl = Start-H2hArm -Label 'control' -Executable $archivedExecutable -StoreRoot ([string]$controlArm.store_root) -RunRoot $concurrentRoot -EvaluationSeed 969999 -Pairs $PreflightPairs
try {
    $concurrentSelected = Start-H2hArm -Label 'selected' -Executable $archivedExecutable -StoreRoot ([string]$selectedArm.store_root) -RunRoot $concurrentRoot -EvaluationSeed 969999 -Pairs $PreflightPairs
}
catch {
    Stop-H2hArms -Runs @($concurrentControl)
    throw
}
$concurrentMeasure = Wait-H2hArms -Runs @($concurrentControl, $concurrentSelected) -TimeoutSeconds 900

$singleControlHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $singleRun.outcome_path).Hash.ToLowerInvariant()
$concurrentControlHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $concurrentControl.outcome_path).Hash.ToLowerInvariant()
$controlBitIdentical = $singleControlHash -eq $concurrentControlHash
$gamesPerArm = 2 * $PreflightPairs
$singleRate = $gamesPerArm / [double]$singleMeasure.wall_seconds
$concurrentRate = (2 * $gamesPerArm) / [double]$concurrentMeasure.wall_seconds
$speedup = $concurrentRate / $singleRate
$singleResources = Get-ResourceSummary $singleMeasure.samples
$concurrentResources = Get-ResourceSummary $concurrentMeasure.samples
$resourceSafe = ([double]$concurrentResources.minimum_host_memory_free_mib -ge 4096)
$selectedTopology = if ($controlBitIdentical -and $resourceSafe -and $speedup -ge 1.20) { 'concurrent-arms' } else { 'serial-arms' }
$preflightPassed = $controlBitIdentical -and $resourceSafe
$projectedFormalSeconds = if ($selectedTopology -eq 'concurrent-arms') {
    (2 * 2 * $FormalPairs) / $concurrentRate
}
else {
    (2 * 2 * $FormalPairs) / $singleRate
}
$preflightManifest = [ordered]@{
    schema = 'regularized-continuation-gross-safety-throughput/v1'
    passed = $preflightPassed
    revealed_seed = 969999
    pairs_per_arm = $PreflightPairs
    games_per_arm = $gamesPerArm
    selected_topology = $selectedTopology
    aggregate_speedup = $speedup
    projected_formal_wall_seconds = $projectedFormalSeconds
    control_repeat_bit_identical = $controlBitIdentical
    resource_safe = $resourceSafe
    rates = [ordered]@{ single_arm_games_per_second = $singleRate; concurrent_aggregate_games_per_second = $concurrentRate }
    measurements = [ordered]@{
        prelaunch = $preflightPrelaunch
        single = [ordered]@{ wall_seconds = $singleMeasure.wall_seconds; resources = $singleResources; samples = @($singleMeasure.samples) }
        concurrent = [ordered]@{ wall_seconds = $concurrentMeasure.wall_seconds; resources = $concurrentResources; samples = @($concurrentMeasure.samples) }
    }
    git = $git
    toolchain = $toolchain
    cuda = $cuda
    executable = Get-FileRecord $archivedExecutable
    coefficient_manifest = Get-FileRecord $CoefficientManifestPath
    stores = [ordered]@{
        control_tree_sha256 = [string]$controlArm.store_tree_sha256
        selected_beta = $selectedBeta
        selected_tree_sha256 = [string]$selectedArm.store_tree_sha256
    }
    outputs = [ordered]@{
        single_control = Get-FileRecord $singleRun.outcome_path
        concurrent_control = Get-FileRecord $concurrentControl.outcome_path
        concurrent_selected = Get-FileRecord $concurrentSelected.outcome_path
    }
}
$preflightManifestPath = Join-Path $preflightRoot 'throughput-manifest.json'
Write-Utf8NoBomJsonFile -Value $preflightManifest -Path $preflightManifestPath
if (-not $preflightPassed) {
    throw "gross-safety throughput screen failed; see $preflightManifestPath"
}

$formalEvidence = Join-Path $EvidenceRoot 'development\seed-1942001'
$formalRoot = New-UniqueAttemptRoot -EvidenceRoot $formalEvidence -GateName 'gross-safety'
Assert-ExclusiveWindow
$formalPrelaunch = Assert-PrelaunchResourceWindow
$formalControl = $null
$formalSelected = $null
$formalWallStarted = [DateTimeOffset]::UtcNow
if ($selectedTopology -eq 'concurrent-arms') {
    $formalControl = Start-H2hArm -Label 'control' -Executable $archivedExecutable -StoreRoot ([string]$controlArm.store_root) -RunRoot $formalRoot -EvaluationSeed 1942001 -Pairs $FormalPairs
    try {
        $formalSelected = Start-H2hArm -Label 'selected' -Executable $archivedExecutable -StoreRoot ([string]$selectedArm.store_root) -RunRoot $formalRoot -EvaluationSeed 1942001 -Pairs $FormalPairs
    }
    catch {
        Stop-H2hArms -Runs @($formalControl)
        throw
    }
    $formalMeasure = Wait-H2hArms -Runs @($formalControl, $formalSelected) -TimeoutSeconds 3600
}
else {
    $formalControl = Start-H2hArm -Label 'control' -Executable $archivedExecutable -StoreRoot ([string]$controlArm.store_root) -RunRoot $formalRoot -EvaluationSeed 1942001 -Pairs $FormalPairs
    $controlMeasure = Wait-H2hArms -Runs @($formalControl) -TimeoutSeconds 3600
    $formalSelected = Start-H2hArm -Label 'selected' -Executable $archivedExecutable -StoreRoot ([string]$selectedArm.store_root) -RunRoot $formalRoot -EvaluationSeed 1942001 -Pairs $FormalPairs
    $selectedMeasure = Wait-H2hArms -Runs @($formalSelected) -TimeoutSeconds 3600
    $formalMeasure = [ordered]@{
        wall_seconds = [double]$controlMeasure.wall_seconds + [double]$selectedMeasure.wall_seconds
        started_utc = $controlMeasure.started_utc
        completed_utc = $selectedMeasure.completed_utc
        samples = @($controlMeasure.samples) + @($selectedMeasure.samples)
    }
}
$formalWallCompleted = [DateTimeOffset]::UtcNow
$formalMeasure.wall_seconds = ($formalWallCompleted - $formalWallStarted).TotalSeconds
$formalMeasure.started_utc = $formalWallStarted.ToString('O')
$formalMeasure.completed_utc = $formalWallCompleted.ToString('O')

# Both complete terminal streams exist before the first outcome is parsed here.
$classificationPath = Join-Path $formalRoot 'gross-safety-classification.json'
$classifier = Join-Path $PSScriptRoot 'gross-safety-classifier.ps1'
& $classifier -ControlPath $formalControl.outcome_path -SelectedPath $formalSelected.outcome_path -OutputPath $classificationPath -ExpectedSeed 1942001 -ExpectedPairs $FormalPairs -OverallFloor -26 -SeatFloor -18 -ExpectedOpponentRunSha256 ([string]$pool.primary.source_run_sha256) -ExpectedOpponentCheckpointSha256 ([string]$pool.primary.checkpoint_sha256)
$classification = Get-Content -LiteralPath $classificationPath -Raw | ConvertFrom-Json
$formalResources = Get-ResourceSummary $formalMeasure.samples
$formalManifest = [ordered]@{
    schema = 'regularized-continuation-gross-safety/v1'
    passed = [bool]$classification.passed
    disposition = [string]$classification.disposition
    selected_beta = $selectedBeta
    evaluation_seed = 1942001
    pairs_per_arm = $FormalPairs
    games_per_arm = 2 * $FormalPairs
    terminal_outcomes_read_after_both_arms_completed = $true
    selected_topology = $selectedTopology
    wall_seconds = $formalMeasure.wall_seconds
    aggregate_games_per_second = (2 * 2 * $FormalPairs) / [double]$formalMeasure.wall_seconds
    prelaunch = $formalPrelaunch
    resources = $formalResources
    resource_samples = @($formalMeasure.samples)
    git = $git
    toolchain = $toolchain
    cuda = $cuda
    design = [ordered]@{
        commit = 'e9bd7e5b4ef7b8320bb22edfc573ba50a8496ba7'
        document_sha256 = '1f6ea9128e7d5b44f80c34c96c42ccd47325224fb11d22bc4aae0d43cef40b00'
    }
    prerequisite_coefficient = Get-FileRecord $CoefficientManifestPath
    prerequisite_throughput = Get-FileRecord $preflightManifestPath
    executable = Get-FileRecord $archivedExecutable
    inputs = [ordered]@{
        pool = Get-FileRecord $script:PoolJson
        control_store_root = [string]$controlArm.store_root
        control_store_tree_sha256 = [string]$controlArm.store_tree_sha256
        selected_store_root = [string]$selectedArm.store_root
        selected_store_tree_sha256 = [string]$selectedArm.store_tree_sha256
    }
    outputs = [ordered]@{
        control_terminal_stream = Get-FileRecord $formalControl.outcome_path
        selected_terminal_stream = Get-FileRecord $formalSelected.outcome_path
        classification = Get-FileRecord $classificationPath
        control_stdout = Get-FileRecord $formalControl.stdout_path
        control_stderr = Get-FileRecord $formalControl.stderr_path
        selected_stdout = Get-FileRecord $formalSelected.stdout_path
        selected_stderr = Get-FileRecord $formalSelected.stderr_path
    }
    result = $classification
}
$formalManifestPath = Join-Path $formalRoot 'gross-safety-manifest.json'
Write-Utf8NoBomJsonFile -Value $formalManifest -Path $formalManifestPath
Write-Host "GROSS SAFETY complete disposition=$($classification.disposition) wall_seconds=$([math]::Round($formalMeasure.wall_seconds, 2)) manifest=$formalManifestPath"
