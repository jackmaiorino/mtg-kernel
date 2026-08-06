param(
    [string]$EvidenceRoot = 'D:\mtg-kernel-regularized-continuation-retest-v1\development',
    [string]$TrainingManifestPath = 'D:\mtg-kernel-regularized-continuation-retest-v1\development\full-horizon-training\attempt-003\training-manifest.json',
    [string]$DesignDocumentPath = 'C:\Users\Jack\IdeaProjects\mtg-kernel-composed-factorial-v1-codex\docs\native_regularized_continuation_retest_v1.md',
    [string]$ClassifierPath = '',
    [string]$EbCsReferencePath = 'C:\Users\Jack\IdeaProjects\collab\eb_cs_reference_v1.py',
    [string]$ParentDriftManifestPath = '',
    [string]$PythonCommand = 'python',
    [uint64]$PreflightPairs = 64,
    [uint64]$WatchdogSeconds = 7200,
    [switch]$PreflightOnly
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'common.ps1')

$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
$script:H2hTest = 'native_science_loop_v1::windows_science_loop_tests::ladder_head_to_head_eval_v1'
$script:DesignCommit = 'e9bd7e5b4ef7b8320bb22edfc573ba50a8496ba7'
$script:DesignSha256 = '1f6ea9128e7d5b44f80c34c96c42ccd47325224fb11d22bc4aae0d43cef40b00'
$script:SelectedBeta = '0.1'
$script:TrainingSeeds = @([uint64]970001, [uint64]970002, [uint64]970003)
$script:AllGenerations = @([uint64]64, [uint64]128, [uint64]256, [uint64]384, [uint64]512)
$script:EndpointGenerations = @([uint64]384, [uint64]512)
$script:Updates = [uint64]512
$script:EvaluationSeed = [uint64]982001
$script:ThroughputSeed = [uint64]969999
$script:OpponentGeneration = [uint64]384
$script:DiagnosticPairs = [uint64]512
$script:EndpointPairs = [uint64]2048
$script:ExpectedTrainingSchema = 'regularized-continuation-full-horizon-training/v1'
$script:ExpectedTrainingDisposition = 'TRAINING-COMPLETE; DEVELOPMENT-EVALUATION-RELEASED'
$script:ArmScript = Join-Path $PSScriptRoot 'run-full-horizon-eval-arm.ps1'
$script:ClassifierPath = if ([string]::IsNullOrWhiteSpace($ClassifierPath)) {
    Join-Path $PSScriptRoot 'full-horizon-classifier.py'
}
else {
    $ClassifierPath
}

function Get-FileRecord {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "required file is missing: $Path"
    }
    $item = Get-Item -LiteralPath $Path
    [ordered]@{
        path = $item.FullName
        bytes = [uint64]$item.Length
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $item.FullName).Hash.ToLowerInvariant()
    }
}

function Assert-FileSha256 {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$ExpectedSha256,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $record = Get-FileRecord -Path $Path
    if ([string]$record.sha256 -ne $ExpectedSha256) {
        throw "$Label SHA-256 mismatch: observed $($record.sha256), expected $ExpectedSha256"
    }
    $record
}

function Get-HarnessRecord {
    [ordered]@{
        controller = Get-FileRecord -Path $PSCommandPath
        arm_wrapper = Get-FileRecord -Path $script:ArmScript
        common = Get-FileRecord -Path (Join-Path $PSScriptRoot 'common.ps1')
        gross_safety_controller = Get-FileRecord -Path (Join-Path $PSScriptRoot 'gross-safety.ps1')
        gross_safety_arm_pattern = Get-FileRecord -Path (Join-Path $PSScriptRoot 'run-gross-safety-arm.ps1')
        classifier = Get-FileRecord -Path $script:ClassifierPath
    }
}

function Get-StoreBinding {
    param(
        [Parameter(Mandatory = $true)]$TrainingRecord,
        [Parameter(Mandatory = $true)][ValidateSet('candidate', 'control')][string]$Role
    )
    $store = [string]$TrainingRecord.store_root
    if ([string]::IsNullOrWhiteSpace($store)) {
        throw "$Role training record has no store_root"
    }
    Assert-GenerationCheckpoint -Store $store -Generation $script:Updates
    $runPath = Join-Path $store 'run.json'
    $run = Get-Content -LiteralPath $runPath -Raw | ConvertFrom-Json
    if ([uint64]$TrainingRecord.generation -ne $script:Updates -or
        [uint64]$TrainingRecord.adam_step -ne $script:Updates -or
        [uint64]$TrainingRecord.completed_episode_count -ne 32768 -or
        [uint64]$run.schedule.base_seed -ne [uint64]$TrainingRecord.seed -or
        [uint64]$run.schedule.requested_successful_updates -ne $script:Updates -or
        [uint64]$run.schedule.batch_episodes -ne 64 -or
        [string]$run.environment.environment_randomization_v2.identity -ne 'mtg-kernel-environment-randomization-sha256-v2') {
        throw "$Role seed-$($TrainingRecord.seed) Store schedule or envrand-v2 binding mismatch"
    }
    $inventory = @(Get-StoreFileInventory -Path $store)
    $tree = Get-StoreTreeHash -Path $store
    if ($tree -ne [string]$TrainingRecord.store_tree_sha256 -or
        [uint64]$inventory.Count -ne [uint64]$TrainingRecord.store_file_count) {
        throw "$Role seed-$($TrainingRecord.seed) Store no longer matches the training manifest"
    }
    [ordered]@{
        role = $Role
        training_seed = [uint64]$TrainingRecord.seed
        store_root = (Resolve-Path -LiteralPath $store).Path
        store_tree_sha256 = $tree
        store_file_count = [uint64]$inventory.Count
        generation = $script:Updates
        adam_step = [uint64]$TrainingRecord.adam_step
        completed_episode_count = [uint64]$TrainingRecord.completed_episode_count
        training_record_store_tree_sha256 = [string]$TrainingRecord.store_tree_sha256
    }
}

function Assert-StoreBindingUnchanged {
    param([Parameter(Mandatory = $true)]$Binding)
    Assert-GenerationCheckpoint -Store ([string]$Binding.store_root) -Generation $script:Updates
    $tree = Get-StoreTreeHash -Path ([string]$Binding.store_root)
    $count = [uint64]@((Get-StoreFileInventory -Path ([string]$Binding.store_root))).Count
    if ($tree -ne [string]$Binding.store_tree_sha256 -or $count -ne [uint64]$Binding.store_file_count) {
        throw "$($Binding.role) seed-$($Binding.training_seed) Store changed during evaluation"
    }
    [ordered]@{
        role = [string]$Binding.role
        training_seed = [uint64]$Binding.training_seed
        store_root = [string]$Binding.store_root
        store_tree_sha256 = $tree
        store_file_count = $count
    }
}

function Get-PrerequisiteRecord {
    param([Parameter(Mandatory = $true)]$Record, [Parameter(Mandatory = $true)][string]$Name)
    if ($null -eq $Record) {
        throw "training manifest prerequisite is missing: $Name"
    }
    $path = [string]$Record.path
    $actual = Get-FileRecord -Path $path
    if ([string]$actual.sha256 -ne [string]$Record.sha256 -or [uint64]$actual.bytes -ne [uint64]$Record.bytes) {
        throw "training prerequisite changed: $Name"
    }
    $actual
}

function Get-AllowedProcessIds {
    param([Parameter(Mandatory = $true)]$Runs)
    $all = @(Get-CimInstance Win32_Process -ErrorAction Stop)
    $allowed = New-Object 'System.Collections.Generic.HashSet[int]'
    $frontier = @($Runs | ForEach-Object { [int]$_.process_id })
    foreach ($id in $frontier) {
        [void]$allowed.Add($id)
    }
    while ($frontier.Count -ne 0) {
        $next = @()
        foreach ($parent in $frontier) {
            foreach ($child in @($all | Where-Object { [int]$_.ParentProcessId -eq $parent })) {
                $childId = [int]$child.ProcessId
                if ($allowed.Add($childId)) {
                    $next += $childId
                }
            }
        }
        $frontier = $next
    }
    return ,$allowed
}

function Assert-NoForeignTrainerEvalProcesses {
    param([Parameter(Mandatory = $true)]$Runs)
    $allowed = Get-AllowedProcessIds -Runs @($Runs)
    $foreign = @(
        foreach ($row in @(Get-CimInstance Win32_Process -ErrorAction Stop)) {
            $name = [IO.Path]::GetFileNameWithoutExtension([string]$row.Name)
            $commandLine = [string]$row.CommandLine
            $knownNative = $name -match '(?i)(mtg_kernel|native_science|training-executable|evaluator-[0-9a-f]{64}|cargo|rustc)'
            $knownWrapper = ($name -match '(?i)^(powershell|pwsh)$' -and
                $commandLine -match '(?i)(run-full-horizon-eval-arm|run-gross-safety-arm|run-native)\.ps1')
            if (($knownNative -or $knownWrapper) -and -not $allowed.Contains([int]$row.ProcessId)) {
                "$($row.ProcessId):$name $commandLine"
            }
        }
    )
    if ($foreign.Count -ne 0) {
        throw "foreign trainer/evaluator processes are present: $($foreign -join '; ')"
    }
}

function Stop-ProcessTreeLocal {
    param([Parameter(Mandatory = $true)][int]$RootProcessId)
    $all = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)
    $descendants = New-Object 'System.Collections.Generic.List[int]'
    $frontier = @($RootProcessId)
    while ($frontier.Count -ne 0) {
        $next = @()
        foreach ($parent in $frontier) {
            foreach ($child in @($all | Where-Object { [int]$_.ParentProcessId -eq $parent })) {
                $id = [int]$child.ProcessId
                $descendants.Add($id)
                $next += $id
            }
        }
        $frontier = $next
    }
    $ordered = $descendants.ToArray()
    [array]::Reverse($ordered)
    foreach ($id in $ordered) {
        Stop-Process -Id $id -Force -ErrorAction SilentlyContinue
    }
    Stop-Process -Id $RootProcessId -Force -ErrorAction SilentlyContinue
}

function Stop-H2hRuns {
    param([Parameter(Mandatory = $true)]$Runs)
    foreach ($run in @($Runs)) {
        if ($null -ne $run) {
            Stop-ProcessTreeLocal -RootProcessId ([int]$run.process_id)
        }
    }
}

function Start-H2hArm {
    param(
        [Parameter(Mandatory = $true)]$Arm,
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string]$RunRoot
    )
    $label = [string]$Arm.label
    $outcome = Join-Path $RunRoot "$label-terminal-stream.json"
    $stdout = Join-Path $RunRoot "$label.stdout.log"
    $stderr = Join-Path $RunRoot "$label.stderr.log"
    $completion = Join-Path $RunRoot "$label.completion.json"
    $wrapperStdout = Join-Path $RunRoot "$label.wrapper.stdout.log"
    $wrapperStderr = Join-Path $RunRoot "$label.wrapper.stderr.log"
    foreach ($path in @($outcome, $stdout, $stderr, $completion, $wrapperStdout, $wrapperStderr)) {
        if (Test-Path -LiteralPath $path) {
            throw "arm output already exists: $path"
        }
    }
    $arguments = @(
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $script:ArmScript,
        '-Executable', $Executable, '-TestName', $script:H2hTest,
        '-Label', $label, '-Role', [string]$Arm.role,
        '-TrainingSeed', [uint64]$Arm.training_seed, '-Generation', [uint64]$Arm.generation,
        '-PairCount', [uint64]$Arm.pairs, '-EvaluationSeed', [uint64]$Arm.evaluation_seed,
        '-Updates', $script:Updates, '-CandidateStoreRoot', [string]$Arm.store_root,
        '-PoolJson', $script:PoolJson, '-InitStore', $script:InitStore,
        '-InitGeneration', $script:InitGeneration, '-OpponentStoreRoot', $script:InitStore,
        '-OpponentGeneration', $script:OpponentGeneration, '-OutcomePath', $outcome,
        '-StdoutPath', $stdout, '-StderrPath', $stderr, '-CompletionPath', $completion
    )
    $launched = [DateTimeOffset]::UtcNow
    $process = Start-Process -FilePath 'powershell.exe' -ArgumentList $arguments -WorkingDirectory $script:RepoRoot -PassThru -WindowStyle Hidden -RedirectStandardOutput $wrapperStdout -RedirectStandardError $wrapperStderr
    [ordered]@{
        arm = $Arm
        process = $process
        process_id = [int]$process.Id
        launched_utc = $launched.ToString('O')
        outcome_path = $outcome
        stdout_path = $stdout
        stderr_path = $stderr
        completion_path = $completion
        wrapper_stdout_path = $wrapperStdout
        wrapper_stderr_path = $wrapperStderr
    }
}

function Get-ResourceSummaryLocal {
    param([Parameter(Mandatory = $true)]$Samples)
    $rows = @($Samples)
    if ($rows.Count -eq 0) {
        throw 'resource summary requires at least one sample'
    }
    $cpu = @($rows | ForEach-Object { [double]$_.cpu_total_percent })
    $used = @($rows | ForEach-Object { [double]$_.host_memory_used_mib })
    $total = [double]$rows[0].host_memory_total_mib
    [ordered]@{
        sample_count = $rows.Count
        mean_cpu_percent = ($cpu | Measure-Object -Average).Average
        maximum_cpu_percent = ($cpu | Measure-Object -Maximum).Maximum
        maximum_host_memory_used_mib = ($used | Measure-Object -Maximum).Maximum
        minimum_host_memory_free_mib = $total - ($used | Measure-Object -Maximum).Maximum
        gpu_samples = @(
            foreach ($ordinal in @(0, 1)) {
                $gpuRows = @($rows | ForEach-Object { $_.gpus } | Where-Object { $_.ordinal -eq $ordinal })
                [ordered]@{
                    ordinal = $ordinal
                    sample_count = $gpuRows.Count
                    mean_utilization_percent = if ($gpuRows.Count -eq 0) { $null } else { ($gpuRows | ForEach-Object { [double]$_.utilization_percent } | Measure-Object -Average).Average }
                    maximum_utilization_percent = if ($gpuRows.Count -eq 0) { $null } else { ($gpuRows | ForEach-Object { [double]$_.utilization_percent } | Measure-Object -Maximum).Maximum }
                    maximum_memory_used_mib = if ($gpuRows.Count -eq 0) { $null } else { ($gpuRows | ForEach-Object { [double]$_.memory_used_mib } | Measure-Object -Maximum).Maximum }
                    minimum_memory_free_mib = if ($gpuRows.Count -eq 0) { $null } else { [double]$gpuRows[0].memory_total_mib - ($gpuRows | ForEach-Object { [double]$_.memory_used_mib } | Measure-Object -Maximum).Maximum }
                }
            }
        )
    }
}

function Assert-ArmCompletion {
    param(
        [Parameter(Mandatory = $true)]$Run,
        [Parameter(Mandatory = $true)][string]$ExecutableSha256
    )
    if (-not (Test-Path -LiteralPath $Run.completion_path -PathType Leaf)) {
        throw "$($Run.arm.label) exited without a completion record"
    }
    $completion = Get-Content -LiteralPath $Run.completion_path -Raw | ConvertFrom-Json
    $arm = $Run.arm
    if ([string]$completion.schema -ne 'regularized-continuation-full-horizon-eval-arm-completion/v1' -or
        $completion.success -ne $true -or [int]$completion.native_exit_code -ne 0 -or
        [int]$completion.wrapper_process_id -ne [int]$Run.process_id -or
        [string]$completion.label -ne [string]$arm.label -or
        [string]$completion.role -ne [string]$arm.role -or
        [uint64]$completion.training_seed -ne [uint64]$arm.training_seed -or
        [uint64]$completion.generation -ne [uint64]$arm.generation -or
        [uint64]$completion.pair_count -ne [uint64]$arm.pairs -or
        [uint64]$completion.evaluation_seed -ne [uint64]$arm.evaluation_seed -or
        [uint64]$completion.updates -ne $script:Updates -or
        [uint64]$completion.init_generation -ne $script:InitGeneration -or
        [uint64]$completion.opponent_generation -ne $script:OpponentGeneration -or
        [string]$completion.candidate_store_root -ne [string]$arm.store_root -or
        [string]$completion.pool_json -ne $script:PoolJson -or
        [string]$completion.init_store -ne $script:InitStore -or
        [string]$completion.opponent_store_root -ne $script:InitStore -or
        $completion.environment_randomization_v2 -ne $true -or
        [uint64]$completion.worker_count -ne 2 -or
        [uint64]$completion.sessions_per_worker -ne 32 -or
        [uint64]$completion.broker_batch_target -ne 16 -or
        [string]$completion.executable_sha256 -ne $ExecutableSha256) {
        throw "$($arm.label) completion binding is invalid"
    }
    foreach ($path in @($Run.outcome_path, $Run.stdout_path, $Run.stderr_path)) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "$($arm.label) output is missing: $path"
        }
    }
    $outcomeSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $Run.outcome_path).Hash.ToLowerInvariant()
    $stdoutSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $Run.stdout_path).Hash.ToLowerInvariant()
    $stderrSha = (Get-FileHash -Algorithm SHA256 -LiteralPath $Run.stderr_path).Hash.ToLowerInvariant()
    if ([string]$completion.outcome_sha256 -ne $outcomeSha -or
        [string]$completion.stdout_sha256 -ne $stdoutSha -or
        [string]$completion.stderr_sha256 -ne $stderrSha) {
        throw "$($arm.label) completion output hashes do not match"
    }
    if ((Get-Item -LiteralPath $Run.stderr_path).Length -ne 0) {
        throw "$($arm.label) evaluator wrote stderr"
    }
    $Run.process.Refresh()
    if ([int]$Run.process.ExitCode -ne 0 -or
        (Get-Item -LiteralPath $Run.wrapper_stdout_path).Length -ne 0 -or
        (Get-Item -LiteralPath $Run.wrapper_stderr_path).Length -ne 0) {
        throw "$($arm.label) wrapper exit or console binding is invalid"
    }
    [ordered]@{
        label = [string]$arm.label
        role = [string]$arm.role
        training_seed = [uint64]$arm.training_seed
        generation = [uint64]$arm.generation
        pairs = [uint64]$arm.pairs
        evaluation_seed = [uint64]$arm.evaluation_seed
        outcome = Get-FileRecord -Path $Run.outcome_path
        completion = Get-FileRecord -Path $Run.completion_path
        stdout = Get-FileRecord -Path $Run.stdout_path
        stderr = Get-FileRecord -Path $Run.stderr_path
        wrapper_stdout = Get-FileRecord -Path $Run.wrapper_stdout_path
        wrapper_stderr = Get-FileRecord -Path $Run.wrapper_stderr_path
        completion_payload = $completion
    }
}

function Wait-H2hBatch {
    param(
        [Parameter(Mandatory = $true)]$Runs,
        [Parameter(Mandatory = $true)][uint64]$TimeoutSeconds,
        [Parameter(Mandatory = $true)][string]$ExecutableSha256
    )
    $samples = New-Object 'System.Collections.Generic.List[object]'
    $started = @($Runs | ForEach-Object { [DateTimeOffset]::Parse([string]$_.launched_utc) } | Sort-Object | Select-Object -First 1)[0]
    $nextResourceSample = [DateTimeOffset]::MinValue
    try {
        while (@($Runs | Where-Object { -not $_.process.HasExited }).Count -ne 0) {
            $now = [DateTimeOffset]::UtcNow
            if ($now -ge $nextResourceSample) {
                Assert-NoForeignTrainerEvalProcesses -Runs @($Runs)
                Assert-NoForeignGpu1ComputeProcesses
                $samples.Add((Get-ResourceSample))
                $nextResourceSample = $now.AddSeconds(5)
            }
            if (($now - $started).TotalSeconds -gt $TimeoutSeconds) {
                throw "H2H watchdog exceeded $TimeoutSeconds seconds"
            }
            Start-Sleep -Milliseconds 200
            foreach ($run in @($Runs)) {
                $run.process.Refresh()
            }
        }
        $samples.Add((Get-ResourceSample))
        foreach ($run in @($Runs)) {
            $run.process.WaitForExit()
        }
        $completed = [DateTimeOffset]::UtcNow
        $records = @($Runs | ForEach-Object { Assert-ArmCompletion -Run $_ -ExecutableSha256 $ExecutableSha256 })
        $sampleArray = $samples.ToArray()
        [ordered]@{
            started_utc = $started.ToString('O')
            completed_utc = $completed.ToString('O')
            wall_seconds = ($completed - $started).TotalSeconds
            samples = $sampleArray
            resources = Get-ResourceSummaryLocal -Samples $sampleArray
            records = $records
        }
    }
    catch {
        Stop-H2hRuns -Runs @($Runs)
        throw
    }
}

function Invoke-H2hBatch {
    param(
        [Parameter(Mandatory = $true)]$Arms,
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][int]$BatchIndex,
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string]$ExecutableSha256,
        [Parameter(Mandatory = $true)][uint64]$TimeoutSeconds
    )
    Assert-NoForeignTrainerEvalProcesses -Runs @()
    Assert-PrelaunchResourceWindow | Out-Null
    $batchRoot = Join-Path $Root ('batch-{0:d3}' -f $BatchIndex)
    New-Item -ItemType Directory -Force -Path $batchRoot -ErrorAction Stop | Out-Null
    $runs = New-Object 'System.Collections.Generic.List[object]'
    try {
        foreach ($arm in @($Arms)) {
            $runs.Add((Start-H2hArm -Arm $arm -Executable $Executable -RunRoot $batchRoot))
        }
        return Wait-H2hBatch -Runs ($runs.ToArray()) -TimeoutSeconds $TimeoutSeconds -ExecutableSha256 $ExecutableSha256
    }
    catch {
        Stop-H2hRuns -Runs ($runs.ToArray())
        throw
    }
}

function Get-ArmOutputRecord {
    param([Parameter(Mandatory = $true)]$Record)
    [ordered]@{
        id = [string]$Record.label
        path = [string]$Record.outcome.path
        sha256 = [string]$Record.outcome.sha256
        arm = [string]$Record.role
        training_seed = [uint64]$Record.training_seed
        generation = [uint64]$Record.generation
        pairs = [uint64]$Record.pairs
        evaluation_seed = [uint64]$Record.evaluation_seed
        completion = $Record.completion
        stdout = $Record.stdout
        stderr = $Record.stderr
    }
}

function Get-ParentDriftObject {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)]$ExpectedTraining,
        [Parameter(Mandatory = $true)]$ExpectedTrainingFile
    )
    $manifest = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    if ([string]$manifest.schema -ne 'regularized-continuation-full-horizon-parent-drift/v1' -or
        $manifest.passed -ne $true -or [string]$manifest.disposition -ne 'DIAGNOSTIC-COMPLETE' -or
        $manifest.terminal_outcomes_read -ne $false -or
        [string]$manifest.training.sha256 -ne [string]$ExpectedTrainingFile.sha256 -or
        [string]$manifest.design.sha256 -ne $script:DesignSha256) {
        throw 'parent-drift manifest identity or completion mismatch'
    }
    foreach ($binding in @(
        [ordered]@{ record = $manifest.training; label = 'parent-drift training manifest' },
        [ordered]@{ record = $manifest.report; label = 'parent-drift report' },
        [ordered]@{ record = $manifest.classification; label = 'parent-drift classification' }
    )) {
        $actual = Get-FileRecord -Path ([string]$binding.record.path)
        if ([string]$actual.sha256 -ne [string]$binding.record.sha256 -or [uint64]$actual.bytes -ne [uint64]$binding.record.bytes) {
            throw "$($binding.label) file binding changed"
        }
    }
    $report = Get-Content -LiteralPath ([string]$manifest.report.path) -Raw | ConvertFrom-Json
    if ([string]$report.schema -ne 'regularized-continuation-full-horizon-parent-drift-report/v1' -or
        $report.terminal_outcomes_read -ne $false -or
        [uint64]$report.corpus.evaluation_base_seed -ne 1941001 -or
        [uint64]$report.corpus.pair_count -ne 512 -or [uint64]$report.corpus.episode_count -ne 1024 -or
        $report.corpus.all_natural -ne $true -or @($report.arms).Count -ne 6 -or
        [string]$report.corpus.parent_identity.run_sha256 -ne '2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae' -or
        [string]$report.corpus.parent_identity.checkpoint_manifest_sha256 -ne '4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8' -or
        [string]$report.corpus.parent_identity.checkpoint_payload_sha256 -ne 'a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99' -or
        [string]$report.corpus.parent_identity.model_parameter_sha256 -ne 'db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d') {
        throw 'parent-drift report corpus, parent, or completeness binding mismatch'
    }
    $expectedRoots = @(
        @($ExpectedTraining.candidates | Sort-Object { [uint64]$_.seed } | ForEach-Object { [string]$_.store_root }) +
        @($ExpectedTraining.controls | Sort-Object { [uint64]$_.seed } | ForEach-Object { [string]$_.store_root })
    )
    for ($index = 0; $index -lt 6; $index++) {
        if ([string]$report.arms[$index].store_root -ne $expectedRoots[$index] -or
            $report.arms[$index].complete -ne $true -or $report.arms[$index].finite -ne $true) {
            throw "parent-drift arm $index Store or completion binding mismatch"
        }
    }
    $classification = Get-Content -LiteralPath ([string]$manifest.classification.path) -Raw | ConvertFrom-Json
    if ([string]$classification.schema -ne 'regularized-continuation-full-horizon-parent-drift-classification/v1' -or
        $classification.complete -ne $true -or $classification.terminal_outcomes_read -ne $false -or
        [uint64]$classification.evaluation_base_seed -ne 1941001 -or [uint64]$classification.pair_count -ne 512 -or
        @($classification.seeds).Count -ne 3 -or
        $classification.escalation_available -ne $false -or
        [string]$classification.source_report.sha256 -ne [string]$manifest.report.sha256) {
        throw 'parent-drift classification identity or source binding mismatch'
    }
    $ratios = [ordered]@{}
    foreach ($seed in $script:TrainingSeeds) {
        $rows = @($classification.seeds | Where-Object { [uint64]$_.seed -eq $seed })
        if ($rows.Count -ne 1) { throw "parent-drift classification seed-$seed binding mismatch" }
        $ratio = [double]$rows[0].R_512
        if ([double]::IsNaN($ratio) -or [double]::IsInfinity($ratio) -or $ratio -lt 0.0) {
            throw "parent-drift classification seed-$seed R_512 is invalid"
        }
        $published = [double]$classification.R512.([string]$seed)
        if ($published -ne $ratio) { throw "parent-drift classification seed-$seed R512 summary mismatch" }
        $ratios[[string]$seed] = $ratio
    }
    [ordered]@{
        R512 = $ratios
        escalation_available = $false
        escalation_unavailable_reason = [string]$classification.escalation_unavailable_reason
        source_manifest_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
    }
}

function Get-ArmPlan {
    param([Parameter(Mandatory = $true)]$Candidates, [Parameter(Mandatory = $true)]$Controls)
    $arms = New-Object 'System.Collections.Generic.List[object]'
    foreach ($candidate in @($Candidates | Sort-Object training_seed)) {
        foreach ($generation in $script:AllGenerations) {
            $pairs = if ($generation -in @(64, 128, 256)) { $script:DiagnosticPairs } else { $script:EndpointPairs }
            $arms.Add([ordered]@{
                label = "candidate-seed-$($candidate.training_seed)-gen-$generation-pairs-$pairs"
                role = 'candidate'
                training_seed = [uint64]$candidate.training_seed
                generation = [uint64]$generation
                pairs = [uint64]$pairs
                evaluation_seed = $script:EvaluationSeed
                store_root = [string]$candidate.store_root
                store_tree_sha256 = [string]$candidate.store_tree_sha256
            })
        }
    }
    foreach ($control in @($Controls | Sort-Object training_seed)) {
        foreach ($generation in $script:EndpointGenerations) {
            $arms.Add([ordered]@{
                label = "control-seed-$($control.training_seed)-gen-$generation-pairs-$($script:EndpointPairs)"
                role = 'control'
                training_seed = [uint64]$control.training_seed
                generation = [uint64]$generation
                pairs = $script:EndpointPairs
                evaluation_seed = $script:EvaluationSeed
                store_root = [string]$control.store_root
                store_tree_sha256 = [string]$control.store_tree_sha256
            })
        }
    }
    if ($arms.Count -ne 21) {
        throw "full-horizon arm plan has $($arms.Count) arms, expected 21"
    }
    $arms.ToArray()
}

$phase = 'preflight'
$root = $null
try {
    if ($PreflightPairs -ne 64) {
        throw 'the revealed throughput screen is frozen at 64 pairs'
    }
    if ([string]::IsNullOrWhiteSpace($ParentDriftManifestPath)) {
        throw 'the completed parent-drift manifest is required before full-horizon evaluation'
    }
    if (-not (Test-Path -LiteralPath $TrainingManifestPath -PathType Leaf)) {
        throw 'a passed create-new full-horizon training manifest is required'
    }
    Assert-ExclusiveWindow
    Assert-NoForeignTrainerEvalProcesses -Runs @()
    Assert-NoForeignGpu1ComputeProcesses

    $trainingFile = Get-FileRecord -Path $TrainingManifestPath
    $training = Get-Content -LiteralPath $TrainingManifestPath -Raw | ConvertFrom-Json
    if ([string]$training.schema -ne $script:ExpectedTrainingSchema -or
        $training.passed -ne $true -or
        [string]$training.disposition -ne $script:ExpectedTrainingDisposition -or
        $training.terminal_outcomes_read -ne $false -or
        [uint64]$training.updates_per_seed -ne $script:Updates) {
        throw 'training manifest is not the required passed terminal-blind create-new release'
    }
    $candidateTraining = @($training.candidates)
    $controlTraining = @($training.controls)
    if ($candidateTraining.Count -ne 3 -or $controlTraining.Count -ne 3) {
        throw 'training manifest must contain exactly three candidates and three controls'
    }
    $expectedSeedsText = (@($script:TrainingSeeds | Sort-Object) -join ',')
    $candidateSeedsText = (@($candidateTraining | ForEach-Object { [uint64]$_.seed } | Sort-Object) -join ',')
    $controlSeedsText = (@($controlTraining | ForEach-Object { [uint64]$_.seed } | Sort-Object) -join ',')
    if ($candidateSeedsText -ne $expectedSeedsText -or $controlSeedsText -ne $expectedSeedsText) {
        throw 'training seeds are not exactly 970001, 970002, and 970003 in both arms'
    }
    if ([string]$training.selected_beta -ne $script:SelectedBeta) {
        throw 'training manifest selected beta is not the frozen 0.1 continuation'
    }

    $design = Assert-FileSha256 -Path $DesignDocumentPath -ExpectedSha256 $script:DesignSha256 -Label 'frozen full-horizon design'
    $git = Get-GitRecord -RepoRoot $script:RepoRoot
    $toolchain = Get-ToolchainRecord
    $cuda = Get-CudaRecord
    $inputs = Get-InputRecord
    if (-not (Test-Path -LiteralPath $script:PoolJson -PathType Leaf)) {
        throw 'Pool3 pool.json is missing'
    }
    $pool = Get-Content -LiteralPath $script:PoolJson -Raw | ConvertFrom-Json
    if ([uint64]$pool.primary.generation -ne $script:OpponentGeneration) {
        throw 'Pool3 primary is not promoted(2) generation 384'
    }
    $prerequisites = [ordered]@{}
    foreach ($name in @('coefficient_screen', 'gross_safety', 'throughput')) {
        $prerequisites[$name] = Get-PrerequisiteRecord -Record $training.prerequisites.$name -Name $name
    }
    $harness = Get-HarnessRecord
    $gpu0 = Assert-GpuIdentity -Ordinal 0
    $gpu1 = Assert-Gpu1Idle
    $prelaunchResources = Assert-PrelaunchResourceWindow

    $candidateBindings = @($candidateTraining | ForEach-Object { Get-StoreBinding -TrainingRecord $_ -Role 'candidate' })
    $controlBindings = @($controlTraining | ForEach-Object { Get-StoreBinding -TrainingRecord $_ -Role 'control' })
    $parentDriftFile = Get-FileRecord -Path $ParentDriftManifestPath
    $parentDriftObject = Get-ParentDriftObject -Path $ParentDriftManifestPath -ExpectedTraining $training -ExpectedTrainingFile $trainingFile
    $arms = Get-ArmPlan -Candidates $candidateBindings -Controls $controlBindings
    $root = New-UniqueAttemptRoot -EvidenceRoot (Join-Path $EvidenceRoot 'seed-982001') -GateName 'full-horizon-evaluation'
    $executable = Get-ReleaseTestExecutable -RepoRoot $script:RepoRoot -EvidenceRoot $root -Label 'full-horizon-eval'
    $executableRecord = Get-FileRecord -Path $executable
    $archivedExecutable = Join-Path $root "evaluator-$($executableRecord.sha256).exe"
    Copy-Item -LiteralPath $executable -Destination $archivedExecutable -ErrorAction Stop
    $archivedExecutableRecord = Get-FileRecord -Path $archivedExecutable

    $plan = [ordered]@{
        schema = 'regularized-continuation-full-horizon-evaluation-plan/v1'
        status = 'preflight complete; revealed throughput screen not started'
        created_utc = [DateTimeOffset]::UtcNow.ToString('O')
        design = [ordered]@{ commit = $script:DesignCommit; document = $design }
        git = $git
        toolchain = $toolchain
        cuda = $cuda
        harness = $harness
        training = $trainingFile
        prerequisites = $prerequisites
        parent_drift_manifest = $parentDriftFile
        inputs = $inputs
        pool = Get-FileRecord -Path $script:PoolJson
        opponent = [ordered]@{ role = 'promoted(2) primary'; generation = $script:OpponentGeneration; store_root = $script:InitStore; checkpoint_sha256 = [string]$pool.primary.checkpoint_sha256 }
        executable = $archivedExecutableRecord
        prelaunch = [ordered]@{ gpu0 = $gpu0; gpu1 = $gpu1; resources = $prelaunchResources }
        frozen_runtime = [ordered]@{ candidate_base_seed = 'training seed'; updates = 512; init_generation = 384; opponent_generation = 384; environment_randomization_v2 = $true; worker_count = 2; sessions_per_worker = 32; broker_batch_target = 16; evaluation_seed = $script:EvaluationSeed; pool = 'Pool3' }
        arm_count = $arms.Count
        arms = $arms
        terminal_outcomes_read = $false
        nonclaim = 'The throughput screen and evaluation harness do not establish playing strength until all 21 streams are classified.'
    }
    $planPath = Join-Path $root 'full-horizon-evaluation-plan.json'
    Write-Utf8NoBomJsonFile -Value $plan -Path $planPath

    $throughputRoot = Join-Path $root 'throughput-screen'
    New-Item -ItemType Directory -Path $throughputRoot -ErrorAction Stop | Out-Null
    $screenArm = @($arms | Where-Object { $_.role -eq 'candidate' -and [uint64]$_.training_seed -eq 970001 -and [uint64]$_.generation -eq 512 })[0]
    $singleOne = Invoke-H2hBatch -Arms @($screenArm | ForEach-Object { [ordered]@{ label = "$($_.label)-single-01"; role = $_.role; training_seed = $_.training_seed; generation = $_.generation; pairs = $PreflightPairs; evaluation_seed = $script:ThroughputSeed; store_root = $_.store_root; store_tree_sha256 = $_.store_tree_sha256 } }) -Root (Join-Path $throughputRoot 'single-01') -BatchIndex 1 -Executable $archivedExecutable -ExecutableSha256 ([string]$archivedExecutableRecord.sha256) -TimeoutSeconds 1800
    $singleTwo = Invoke-H2hBatch -Arms @($screenArm | ForEach-Object { [ordered]@{ label = "$($_.label)-single-02"; role = $_.role; training_seed = $_.training_seed; generation = $_.generation; pairs = $PreflightPairs; evaluation_seed = $script:ThroughputSeed; store_root = $_.store_root; store_tree_sha256 = $_.store_tree_sha256 } }) -Root (Join-Path $throughputRoot 'single-02') -BatchIndex 1 -Executable $archivedExecutable -ExecutableSha256 ([string]$archivedExecutableRecord.sha256) -TimeoutSeconds 1800
    $singleRecords = @($singleOne.records + $singleTwo.records)
    $singleRepeatedBitIdentical = ([string]$singleRecords[0].outcome.sha256 -eq [string]$singleRecords[1].outcome.sha256)
    if (-not $singleRepeatedBitIdentical) {
        throw 'repeated revealed-seed single-arm stream is not bit-identical'
    }
    $twoArms = @(
        1..2 | ForEach-Object {
            [ordered]@{ label = "$($screenArm.label)-two-$('{0:d2}' -f $_)"; role = $screenArm.role; training_seed = $screenArm.training_seed; generation = $screenArm.generation; pairs = $PreflightPairs; evaluation_seed = $script:ThroughputSeed; store_root = $screenArm.store_root; store_tree_sha256 = $screenArm.store_tree_sha256 }
        }
    )
    $eightArms = @(
        1..8 | ForEach-Object {
            [ordered]@{ label = "$($screenArm.label)-eight-$('{0:d2}' -f $_)"; role = $screenArm.role; training_seed = $screenArm.training_seed; generation = $screenArm.generation; pairs = $PreflightPairs; evaluation_seed = $script:ThroughputSeed; store_root = $screenArm.store_root; store_tree_sha256 = $screenArm.store_tree_sha256 }
        }
    )
    $two = Invoke-H2hBatch -Arms $twoArms -Root (Join-Path $throughputRoot 'two') -BatchIndex 1 -Executable $archivedExecutable -ExecutableSha256 ([string]$archivedExecutableRecord.sha256) -TimeoutSeconds 1800
    $eight = Invoke-H2hBatch -Arms $eightArms -Root (Join-Path $throughputRoot 'eight') -BatchIndex 1 -Executable $archivedExecutable -ExecutableSha256 ([string]$archivedExecutableRecord.sha256) -TimeoutSeconds 1800
    $referenceOutcomeSha256 = [string]$singleRecords[0].outcome.sha256
    $twoBitIdentical = @($two.records | Where-Object { [string]$_.outcome.sha256 -ne $referenceOutcomeSha256 }).Count -eq 0
    $eightBitIdentical = @($eight.records | Where-Object { [string]$_.outcome.sha256 -ne $referenceOutcomeSha256 }).Count -eq 0
    $gamesPerArm = 2.0 * [double]$PreflightPairs
    $singleRate = (2.0 * $gamesPerArm) / ([double]$singleOne.wall_seconds + [double]$singleTwo.wall_seconds)
    $twoRate = (2.0 * $gamesPerArm) / [double]$two.wall_seconds
    $eightRate = (8.0 * $gamesPerArm) / [double]$eight.wall_seconds
    $singleSafe = [double]$singleOne.resources.minimum_host_memory_free_mib -ge 4096 -and
        [double]$singleTwo.resources.minimum_host_memory_free_mib -ge 4096 -and
        @($singleOne.resources.gpu_samples | Where-Object { $_.sample_count -eq 0 -or [double]$_.minimum_memory_free_mib -lt 512 }).Count -eq 0 -and
        @($singleTwo.resources.gpu_samples | Where-Object { $_.sample_count -eq 0 -or [double]$_.minimum_memory_free_mib -lt 512 }).Count -eq 0
    $twoSafe = [double]$two.resources.minimum_host_memory_free_mib -ge 4096 -and
        @($two.resources.gpu_samples | Where-Object { $_.sample_count -eq 0 -or [double]$_.minimum_memory_free_mib -lt 512 }).Count -eq 0
    $eightSafe = [double]$eight.resources.minimum_host_memory_free_mib -ge 4096 -and
        @($eight.resources.gpu_samples | Where-Object { $_.sample_count -eq 0 -or [double]$_.minimum_memory_free_mib -lt 512 }).Count -eq 0
    $safeOptions = @()
    if ($singleRepeatedBitIdentical -and $singleSafe) { $safeOptions += [pscustomobject]@{ concurrency = 1; rate = $singleRate } }
    if ($singleRepeatedBitIdentical -and $twoBitIdentical -and $twoSafe) { $safeOptions += [pscustomobject]@{ concurrency = 2; rate = $twoRate } }
    if ($singleRepeatedBitIdentical -and $eightBitIdentical -and $eightSafe) { $safeOptions += [pscustomobject]@{ concurrency = 8; rate = $eightRate } }
    $selectedOption = @($safeOptions | Sort-Object @{ Expression = 'rate'; Descending = $true }, @{ Expression = 'concurrency'; Descending = $false } | Select-Object -First 1)
    $selectedConcurrency = if ($selectedOption.Count -eq 0) { 0 } else { [int]$selectedOption[0].concurrency }
    $selectedRate = if ($selectedOption.Count -eq 0) { 0.0 } else { [double]$selectedOption[0].rate }
    $formalGameCount = [uint64](9 * 2 * $script:DiagnosticPairs + 12 * 2 * $script:EndpointPairs)
    $throughputManifest = [ordered]@{
        schema = 'regularized-continuation-full-horizon-evaluation-throughput/v1'
        passed = ($selectedConcurrency -ne 0)
        topology_scope = 'H2H evaluator process concurrency only; the already-frozen training GPU topology is unchanged'
        revealed_seed = 969999
        pairs_per_arm = $PreflightPairs
        games_per_arm = 2 * $PreflightPairs
        selected_concurrency = $selectedConcurrency
        selection_rule = 'select the highest measured resource-safe aggregate games per second; exact ties choose lower concurrency'
        repeated_stream_bit_identical = $singleRepeatedBitIdentical
        concurrency_streams_bit_identical = [ordered]@{ two = $twoBitIdentical; eight = $eightBitIdentical; reference_outcome_sha256 = $referenceOutcomeSha256 }
        rates = [ordered]@{ one_arm_games_per_second = $singleRate; two_arm_aggregate_games_per_second = $twoRate; eight_arm_aggregate_games_per_second = $eightRate; eight_over_two = $eightRate / $twoRate; two_over_one = $twoRate / $singleRate }
        resource_safe = [ordered]@{ one = $singleSafe; two = $twoSafe; eight = $eightSafe }
        projected_formal = [ordered]@{ game_count = $formalGameCount; selected_games_per_second = $selectedRate; wall_seconds = if ($selectedRate -le 0.0) { $null } else { $formalGameCount / $selectedRate } }
        measurements = [ordered]@{
            one = @($singleOne, $singleTwo) | ForEach-Object { [ordered]@{ wall_seconds = $_.wall_seconds; resources = $_.resources; samples = $_.samples; records = $_.records } }
            two = [ordered]@{ wall_seconds = $two.wall_seconds; resources = $two.resources; samples = $two.samples; records = $two.records }
            eight = [ordered]@{ wall_seconds = $eight.wall_seconds; resources = $eight.resources; samples = $eight.samples; records = $eight.records }
        }
        plan = Get-FileRecord -Path $planPath
        design = [ordered]@{ commit = $script:DesignCommit; document = $design }
        git = $git
        toolchain = $toolchain
        cuda = $cuda
        harness = $harness
        executable = $archivedExecutableRecord
        training = $trainingFile
        prerequisites = $prerequisites
        stores = [ordered]@{ candidates = $candidateBindings; controls = $controlBindings }
        terminal_outcomes_read = $false
        nonclaim = 'The revealed throughput screen measures runtime and determinism only.'
    }
    $throughputManifestPath = Join-Path $throughputRoot 'throughput-manifest.json'
    Write-Utf8NoBomJsonFile -Value $throughputManifest -Path $throughputManifestPath
    if (-not $throughputManifest.passed) {
        throw "full-horizon throughput screen failed; see $throughputManifestPath"
    }
    if ($PreflightOnly) {
        Write-Host "Full-horizon evaluation preflight complete: $throughputManifestPath"
        return
    }

    $phase = 'formal-evaluation'
    $candidateBeforeFormal = @($candidateBindings | ForEach-Object { Assert-StoreBindingUnchanged -Binding $_ })
    $controlBeforeFormal = @($controlBindings | ForEach-Object { Assert-StoreBindingUnchanged -Binding $_ })
    $formalRoot = Join-Path $root 'formal'
    New-Item -ItemType Directory -Path $formalRoot -ErrorAction Stop | Out-Null
    $formalStart = [ordered]@{
        schema = 'regularized-continuation-full-horizon-evaluation-start/v1'
        utc = [DateTimeOffset]::UtcNow.ToString('O')
        plan = Get-FileRecord -Path $planPath
        throughput = Get-FileRecord -Path $throughputManifestPath
        selected_concurrency = [int]$selectedConcurrency
        arm_count = 21
        evaluation_seed = $script:EvaluationSeed
        terminal_outcomes_read = $false
    }
    $formalStartPath = Join-Path $formalRoot 'formal-start.json'
    Write-Utf8NoBomJsonFile -Value $formalStart -Path $formalStartPath
    $formalStarted = [DateTimeOffset]::UtcNow
    $formalBatches = New-Object 'System.Collections.Generic.List[object]'
    $armIndex = 0
    while ($armIndex -lt $arms.Count) {
        $batch = @($arms | Select-Object -Skip $armIndex -First $selectedConcurrency)
        $formalBatches.Add((Invoke-H2hBatch -Arms $batch -Root $formalRoot -BatchIndex ($formalBatches.Count + 1) -Executable $archivedExecutable -ExecutableSha256 ([string]$archivedExecutableRecord.sha256) -TimeoutSeconds $WatchdogSeconds))
        $armIndex += $selectedConcurrency
    }
    $formalCompleted = [DateTimeOffset]::UtcNow
    $formalRecords = @($formalBatches | ForEach-Object { $_.records })
    if ($formalRecords.Count -ne 21) {
        throw "formal evaluation completed $($formalRecords.Count) streams, expected 21"
    }
    $candidateAfter = @($candidateBindings | ForEach-Object { Assert-StoreBindingUnchanged -Binding $_ })
    $controlAfter = @($controlBindings | ForEach-Object { Assert-StoreBindingUnchanged -Binding $_ })

    $parentDriftFileAfter = Get-FileRecord -Path $ParentDriftManifestPath
    if ([string]$parentDriftFileAfter.sha256 -ne [string]$parentDriftFile.sha256 -or
        [uint64]$parentDriftFileAfter.bytes -ne [uint64]$parentDriftFile.bytes) {
        throw 'parent-drift manifest changed during full-horizon evaluation'
    }
    $parentDriftObject = Get-ParentDriftObject -Path $ParentDriftManifestPath -ExpectedTraining $training -ExpectedTrainingFile $trainingFile
    $request = [ordered]@{
        schema = 'regularized-continuation-full-horizon-classifier-request/v1'
        evaluation_base_seed = $script:EvaluationSeed
        streams = @($formalRecords | Sort-Object training_seed, role, generation | ForEach-Object { Get-ArmOutputRecord -Record $_ })
        parent_drift_manifest_path = $parentDriftFile.path
        parent_drift_report = $parentDriftObject
    }
    $requestPath = Join-Path $formalRoot 'full-horizon-classifier-request.json'
    Write-Utf8NoBomJsonFile -Value $request -Path $requestPath

    $classificationPath = Join-Path $formalRoot 'full-horizon-classification.json'
    $classifierStdout = Join-Path $formalRoot 'full-horizon-classifier.stdout.log'
    $classifierStderr = Join-Path $formalRoot 'full-horizon-classifier.stderr.log'
    if (-not (Test-Path -LiteralPath $script:ClassifierPath -PathType Leaf) -or -not (Test-Path -LiteralPath $EbCsReferencePath -PathType Leaf)) {
        throw 'classifier and EB-CS reference files are required after all streams complete'
    }
    $python = Get-Command $PythonCommand -ErrorAction Stop
    & $python.Source $script:ClassifierPath '--request' $requestPath '--output' $classificationPath '--eb-cs-reference' $EbCsReferencePath 1> $classifierStdout 2> $classifierStderr
    $classifierExit = $LASTEXITCODE
    Assert-LastExitCode $classifierExit "full-horizon classifier; see $classifierStderr"
    if (-not (Test-Path -LiteralPath $classificationPath -PathType Leaf)) {
        throw 'full-horizon classifier did not publish its create-new classification'
    }
    if ((Get-Item -LiteralPath $classifierStderr).Length -ne 0) {
        throw 'full-horizon classifier wrote stderr'
    }
    $classification = Get-Content -LiteralPath $classificationPath -Raw | ConvertFrom-Json
    if ([string]$classification.schema -ne 'regularized-continuation-full-horizon-classification/v1') {
        throw 'full-horizon classifier output schema mismatch'
    }
    $allSamples = @($formalBatches | ForEach-Object { $_.samples })
    $finalManifest = [ordered]@{
        schema = 'regularized-continuation-full-horizon-evaluation/v1'
        passed = $true
        disposition = [string]$classification.disposition
        completed_utc = $formalCompleted.ToString('O')
        wall_seconds = ($formalCompleted - $formalStarted).TotalSeconds
        aggregate_games_per_second = (9.0 * 2.0 * $script:DiagnosticPairs + 12.0 * 2.0 * $script:EndpointPairs) / ($formalCompleted - $formalStarted).TotalSeconds
        selected_concurrency = [int]$selectedConcurrency
        arm_count = 21
        evaluation_seed = $script:EvaluationSeed
        plan = Get-FileRecord -Path $planPath
        throughput = Get-FileRecord -Path $throughputManifestPath
        formal_start = Get-FileRecord -Path $formalStartPath
        design = [ordered]@{ commit = $script:DesignCommit; document = $design }
        git = $git
        toolchain = $toolchain
        cuda = $cuda
        gpus = [ordered]@{ gpu0 = $gpu0; gpu1 = $gpu1 }
        harness = $harness
        executable = $archivedExecutableRecord
        training = $trainingFile
        prerequisites = $prerequisites
        inputs = $inputs
        pool = Get-FileRecord -Path $script:PoolJson
        stores_before_and_after = [ordered]@{ candidates_before = $candidateBindings; controls_before = $controlBindings; candidates_before_formal = $candidateBeforeFormal; controls_before_formal = $controlBeforeFormal; candidates_after = $candidateAfter; controls_after = $controlAfter }
        resources = Get-ResourceSummaryLocal -Samples $allSamples
        resource_samples = $allSamples
        formal_batches = @($formalBatches | ForEach-Object { [ordered]@{ started_utc = $_.started_utc; completed_utc = $_.completed_utc; wall_seconds = $_.wall_seconds; resources = $_.resources; samples = $_.samples } })
        classifier_request = Get-FileRecord -Path $requestPath
        classifier = [ordered]@{ output = Get-FileRecord -Path $classificationPath; stdout = Get-FileRecord -Path $classifierStdout; stderr = Get-FileRecord -Path $classifierStderr }
        parent_drift_manifest = $parentDriftFile
        streams = @($formalRecords | Sort-Object training_seed, role, generation | ForEach-Object { Get-ArmOutputRecord -Record $_ })
        result = $classification
        terminal_outcomes_read_after_all_21_completed = $true
        nonclaim = 'This is development evidence on the fixed Rally BO1 native Pool3 panel. It does not establish human, metagame-wide, tournament, or professional-level strength.'
    }
    $finalManifestPath = Join-Path $formalRoot 'full-horizon-evaluation-manifest.json'
    Write-Utf8NoBomJsonFile -Value $finalManifest -Path $finalManifestPath
    Write-Host "Full-horizon evaluation complete: $finalManifestPath"
}
catch {
    if ($null -ne $root) {
        $voidPath = Join-Path $root "void-$phase.log"
        $line = "$([DateTimeOffset]::UtcNow.ToString('O')) phase=$phase VOID environmental_or_harness_interruption=$($_.Exception.Message)"
        $line | Set-Content -LiteralPath $voidPath -Encoding utf8
    }
    throw
}
