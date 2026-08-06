param(
    [string]$TrainingManifestPath = 'D:\mtg-kernel-regularized-continuation-retest-v1\development\full-horizon-training\attempt-003\training-manifest.json',
    [string]$EvidenceRoot = 'D:\mtg-kernel-regularized-continuation-retest-v1\development\seed-1941001',
    [string]$Executable,
    [string]$DesignDocumentPath = 'C:\Users\Jack\IdeaProjects\mtg-kernel-composed-factorial-v1-codex\docs\native_regularized_continuation_retest_v1.md'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
$script:EvaluatorTest = 'native_gate3_terminal_blind_coefficient_screen_v1::full_horizon_parent_drift_v1'
$script:Generations = @([uint64]64, [uint64]128, [uint64]256, [uint64]384, [uint64]512)
$root = New-UniqueAttemptRoot -EvidenceRoot $EvidenceRoot -GateName 'full-horizon-parent-drift'

function Get-FileRecord {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "required file is missing: $Path"
    }
    $item = Get-Item -LiteralPath $Path
    return [ordered]@{
        path = $item.FullName
        bytes = $item.Length
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $item.FullName).Hash.ToLowerInvariant()
    }
}

function Assert-FiniteNonnegative {
    param($Value, [Parameter(Mandatory = $true)][string]$Label)
    $number = [double]$Value
    if ([double]::IsNaN($number) -or [double]::IsInfinity($number) -or $number -lt 0.0) {
        throw "$Label must be finite and nonnegative"
    }
    return $number
}

function Get-CheckpointReport {
    param(
        [Parameter(Mandatory = $true)]$Arm,
        [Parameter(Mandatory = $true)][uint64]$Generation,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $matches = @($Arm.checkpoints | Where-Object { [uint64]$_.generation -eq $Generation })
    if ($matches.Count -ne 1) {
        throw "$Label must contain exactly one generation-$Generation checkpoint"
    }
    $checkpoint = $matches[0]
    if ($checkpoint.overall.finite -ne $true) {
        throw "$Label generation-$Generation overall metrics are not finite"
    }
    Assert-FiniteNonnegative $checkpoint.overall.mean_forward_kl "$Label generation-$Generation mean KL" | Out-Null
    Assert-FiniteNonnegative $checkpoint.overall.mean_row_tv "$Label generation-$Generation mean TV" | Out-Null
    foreach ($seat in @('P0', 'P1')) {
        $seatRows = @($checkpoint.by_learner_seat | Where-Object { [string]$_.learner_seat -eq $seat })
        if ($seatRows.Count -ne 1 -or $seatRows[0].metrics.finite -ne $true) {
            throw "$Label generation-$Generation $seat metrics are missing or nonfinite"
        }
    }
    return $checkpoint
}

function Get-ResourceSummary {
    param([Parameter(Mandatory = $true)]$Samples)
    $rows = @($Samples)
    $cpu = @($rows | ForEach-Object { [double]$_.cpu_total_percent })
    $used = @($rows | ForEach-Object { [double]$_.host_memory_used_mib })
    $hostTotal = [double]$rows[0].host_memory_total_mib
    return [ordered]@{
        sample_count = $rows.Count
        mean_cpu_percent = ($cpu | Measure-Object -Average).Average
        maximum_cpu_percent = ($cpu | Measure-Object -Maximum).Maximum
        maximum_host_memory_used_mib = ($used | Measure-Object -Maximum).Maximum
        minimum_host_memory_free_mib = $hostTotal - ($used | Measure-Object -Maximum).Maximum
        gpus = @(
            foreach ($ordinal in @(0, 1)) {
                $gpuRows = @($rows | ForEach-Object { $_.gpus } | Where-Object { $_.ordinal -eq $ordinal })
                [ordered]@{
                    ordinal = $ordinal
                    utilization_mean_percent = ($gpuRows | ForEach-Object { [double]$_.utilization_percent } | Measure-Object -Average).Average
                    utilization_peak_percent = ($gpuRows | ForEach-Object { [double]$_.utilization_percent } | Measure-Object -Maximum).Maximum
                    memory_peak_mib = ($gpuRows | ForEach-Object { [double]$_.memory_used_mib } | Measure-Object -Maximum).Maximum
                }
            }
        )
    }
}

function Get-AllowedProcessIdsLocal {
    param([int]$RootProcessId = 0)
    $allowed = New-Object 'System.Collections.Generic.HashSet[int]'
    if ($RootProcessId -eq 0) { return ,$allowed }
    [void]$allowed.Add($RootProcessId)
    $all = @(Get-CimInstance Win32_Process -ErrorAction Stop)
    $frontier = @($RootProcessId)
    while ($frontier.Count -ne 0) {
        $next = @()
        foreach ($parent in $frontier) {
            foreach ($child in @($all | Where-Object { [int]$_.ParentProcessId -eq $parent })) {
                $childId = [int]$child.ProcessId
                if ($allowed.Add($childId)) { $next += $childId }
            }
        }
        $frontier = $next
    }
    return ,$allowed
}

function Assert-NoForeignTrainerEvalProcessesLocal {
    param([int]$RootProcessId = 0)
    $allowed = Get-AllowedProcessIdsLocal -RootProcessId $RootProcessId
    $foreign = @(
        foreach ($row in @(Get-CimInstance Win32_Process -ErrorAction Stop)) {
            $name = [IO.Path]::GetFileNameWithoutExtension([string]$row.Name)
            $commandLine = [string]$row.CommandLine
            $knownNative = $name -match '(?i)(mtg_kernel|native_science|training-executable|evaluator-[0-9a-f]{64}|cargo|rustc)'
            $knownWrapper = ($name -match '(?i)^(powershell|pwsh)$' -and
                $commandLine -match '(?i)(run-full-horizon-parent-drift|run-full-horizon-eval-arm|run-native)\.ps1')
            if (($knownNative -or $knownWrapper) -and -not $allowed.Contains([int]$row.ProcessId)) {
                "$($row.ProcessId):$name $commandLine"
            }
        }
    )
    if ($foreign.Count -ne 0) { throw "foreign trainer/evaluator processes are present: $($foreign -join '; ')" }
}

$phase = 'preflight'
try {
    Assert-ExclusiveWindow
    Assert-NoForeignTrainerEvalProcessesLocal
    Assert-NoForeignGpu1ComputeProcesses
    $git = Get-GitRecord -RepoRoot $script:RepoRoot
    $toolchain = Get-ToolchainRecord
    $cuda = Get-CudaRecord
    $prelaunch = Assert-PrelaunchResourceWindow
    $gpu0 = Assert-GpuIdentity -Ordinal 0
    $gpu1 = Assert-Gpu1Idle
    $design = Get-FileRecord -Path $DesignDocumentPath
    if ([string]$design.sha256 -ne '1f6ea9128e7d5b44f80c34c96c42ccd47325224fb11d22bc4aae0d43cef40b00') {
        throw 'scientific design document SHA-256 mismatch'
    }
    $trainingFile = Get-FileRecord -Path $TrainingManifestPath
    $training = Get-Content -LiteralPath $TrainingManifestPath -Raw | ConvertFrom-Json
    if ([string]$training.schema -ne 'regularized-continuation-full-horizon-training/v1' -or
        $training.passed -ne $true -or
        [string]$training.disposition -ne 'TRAINING-COMPLETE; DEVELOPMENT-EVALUATION-RELEASED' -or
        [string]$training.selected_beta -ne '0.1' -or
        [uint64]$training.updates_per_seed -ne 512 -or
        [uint64]$training.episodes_per_seed -ne 32768 -or
        $training.terminal_outcomes_read -ne $false) {
        throw 'full-horizon training prerequisite is not a valid passed manifest'
    }
    $candidates = @($training.candidates | Sort-Object { [uint64]$_.seed })
    $controls = @($training.controls | Sort-Object { [uint64]$_.seed })
    if ($candidates.Count -ne 3 -or $controls.Count -ne 3) {
        throw 'training manifest must bind exactly three candidate and three control Stores'
    }
    foreach ($index in 0..2) {
        $expectedSeed = [uint64](970001 + $index)
        foreach ($record in @($candidates[$index], $controls[$index])) {
            if ([uint64]$record.seed -ne $expectedSeed -or [uint64]$record.generation -ne 512 -or
                [uint64]$record.adam_step -ne 512 -or [uint64]$record.completed_episode_count -ne 32768) {
                throw "training Store record mismatch for seed $expectedSeed"
            }
            $actualTree = Get-StoreTreeHash -Path ([string]$record.store_root)
            $actualCount = [uint64]@((Get-StoreFileInventory -Path ([string]$record.store_root))).Count
            if ($actualTree -ne [string]$record.store_tree_sha256 -or
                $actualCount -ne [uint64]$record.store_file_count) {
                throw "training Store tree changed for seed $expectedSeed role $($record.role)"
            }
        }
    }

    if ([string]::IsNullOrWhiteSpace($Executable)) {
        $Executable = Get-ReleaseTestExecutable -RepoRoot $script:RepoRoot -EvidenceRoot $root -Label 'full-horizon-parent-drift'
    }
    $sourceExecutableFile = Get-FileRecord -Path $Executable
    $archivedExecutable = Join-Path $root "evaluator-$($sourceExecutableFile.sha256).exe"
    Copy-Item -LiteralPath $Executable -Destination $archivedExecutable -ErrorAction Stop
    $Executable = $archivedExecutable
    $executableFile = Get-FileRecord -Path $Executable
    $wrapperFile = Get-FileRecord -Path (Join-Path $PSScriptRoot 'run-full-horizon-parent-drift.ps1')
    $inputs = Get-InputRecord
    $pool = Get-Content -LiteralPath $script:PoolJson -Raw | ConvertFrom-Json
    if ([uint64]$pool.primary.generation -ne 384 -or
        [string]$pool.primary.source_run_sha256 -ne '2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae' -or
        [string]$pool.primary.checkpoint_sha256 -ne '4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8' -or
        [string]$pool.primary.state_sha256 -ne 'a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99') {
        throw 'Pool3 primary is not the exact promoted(2) generation-384 parent'
    }
    $arms = @(
        foreach ($record in $candidates) {
            [ordered]@{ beta = 0.1; store_root = [string]$record.store_root; generations = $script:Generations }
        }
        foreach ($record in $controls) {
            [ordered]@{ beta = 0.0; store_root = [string]$record.store_root; generations = $script:Generations }
        }
    )
    $request = [ordered]@{
        schema = 'regularized-continuation-full-horizon-parent-drift-request/v1'
        parent = [ordered]@{ store_root = $script:InitStore; generation = [uint64]384 }
        pool_json_path = $script:PoolJson
        evaluation_base_seed = [uint64]1941001
        pair_count = [uint64]512
        arms = $arms
    }
    $requestPath = Join-Path $root 'parent-drift-request.json'
    $reportPath = Join-Path $root 'parent-drift-report.json'
    Write-Utf8NoBomJsonFile -Value $request -Path $requestPath
    $plan = [ordered]@{
        schema = 'regularized-continuation-full-horizon-parent-drift-plan/v1'
        status = 'preflight complete; diagnostic evaluation not started'
        created_utc = [DateTimeOffset]::UtcNow.ToString('O')
        git = $git
        toolchain = $toolchain
        cuda = $cuda
        design = $design
        training = $trainingFile
        executable = $executableFile
        wrapper = $wrapperFile
        inputs = $inputs
        prelaunch = [ordered]@{ resources = $prelaunch; gpu0 = $gpu0; gpu1 = $gpu1 }
        request = Get-FileRecord -Path $requestPath
        terminal_outcomes_read = $false
    }
    $planPath = Join-Path $root 'parent-drift-plan.json'
    Write-JsonFile -Value $plan -Path $planPath

    $phase = 'diagnostic-evaluation'
    Write-JsonFile -Value ([ordered]@{
        schema = 'regularized-continuation-full-horizon-parent-drift-start/v1'
        utc = [DateTimeOffset]::UtcNow.ToString('O')
        plan_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $planPath).Hash.ToLowerInvariant()
    }) -Path (Join-Path $root 'parent-drift-start.json')
    $stdoutPath = Join-Path $root 'parent-drift-evaluator.stdout.log'
    $stderrPath = Join-Path $root 'parent-drift-evaluator.stderr.log'
    $completionPath = Join-Path $root 'parent-drift-evaluator.completion.json'
    $wrapperStdoutPath = Join-Path $root 'parent-drift-wrapper.stdout.log'
    $wrapperStderrPath = Join-Path $root 'parent-drift-wrapper.stderr.log'
    $wrapperPath = [string]$wrapperFile.path
    $hostCommand = Get-Command pwsh -ErrorAction SilentlyContinue
    if ($null -eq $hostCommand) {
        $hostCommand = Get-Command powershell -ErrorAction Stop
    }
    $childArgs = @('-NoProfile', '-WindowStyle', 'Hidden', '-File', $wrapperPath,
        '-Executable', $Executable, '-TestName', $script:EvaluatorTest,
        '-RequestPath', $requestPath, '-ReportPath', $reportPath,
        '-StdoutPath', $stdoutPath, '-StderrPath', $stderrPath,
        '-CompletionPath', $completionPath)
    $argText = ($childArgs | ForEach-Object { '"' + ([string]$_).Replace('"', '\"') + '"' }) -join ' '
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    $started = [DateTimeOffset]::UtcNow
    $process = Start-Process -FilePath $hostCommand.Source -ArgumentList $argText -WorkingDirectory $script:RepoRoot -PassThru -WindowStyle Hidden -RedirectStandardOutput $wrapperStdoutPath -RedirectStandardError $wrapperStderrPath
    $samples = New-Object System.Collections.Generic.List[object]
    try {
        while (-not $process.HasExited) {
            if ($clock.Elapsed.TotalHours -gt 2.0) {
                throw 'full-horizon parent-drift evaluator exceeded the two-hour watchdog'
            }
            Assert-NoForeignTrainerEvalProcessesLocal -RootProcessId ([int]$process.Id)
            Assert-NoForeignGpu1ComputeProcesses
            $samples.Add((Get-ResourceSample))
            Start-Sleep -Seconds 1
        }
        $process.WaitForExit()
        $samples.Add((Get-ResourceSample))
    }
    catch {
        Stop-ProcessTree -RootProcessId ([int]$process.Id) -SkipRoot:$process.HasExited
        throw
    }
    $clock.Stop()
    if (-not (Test-Path -LiteralPath $completionPath -PathType Leaf)) {
        throw 'full-horizon parent-drift evaluator exited without a completion record'
    }
    $completion = Get-Content -LiteralPath $completionPath -Raw | ConvertFrom-Json
    if ([string]$completion.schema -ne 'regularized-continuation-full-horizon-parent-drift-completion/v1' -or
        $completion.success -ne $true -or [int]$completion.native_exit_code -ne 0 -or
        [int]$completion.wrapper_process_id -ne [int]$process.Id -or
        [string]$completion.executable_sha256 -ne [string]$executableFile.sha256 -or
        [string]$completion.request_sha256 -ne (Get-FileHash -Algorithm SHA256 -LiteralPath $requestPath).Hash.ToLowerInvariant()) {
        throw 'full-horizon parent-drift completion record is not valid'
    }
    $process.Refresh()
    if ([int]$process.ExitCode -ne 0 -or
        (Get-Item -LiteralPath $wrapperStdoutPath).Length -ne 0 -or
        (Get-Item -LiteralPath $wrapperStderrPath).Length -ne 0) {
        throw 'full-horizon parent-drift wrapper exit or console binding is invalid'
    }
    if ((Get-Item -LiteralPath $stderrPath).Length -ne 0) {
        throw "full-horizon parent-drift evaluator wrote stderr; see $stderrPath"
    }
    $stdoutText = Get-Content -LiteralPath $stdoutPath -Raw
    if ($stdoutText -notmatch 'test result: ok\.') {
        throw 'full-horizon parent-drift native test-success marker is absent'
    }
    if (-not (Test-Path -LiteralPath $reportPath -PathType Leaf) -or
        [string]$completion.report_sha256 -ne (Get-FileHash -Algorithm SHA256 -LiteralPath $reportPath).Hash.ToLowerInvariant()) {
        throw 'full-horizon parent-drift evaluator did not publish its bound report'
    }
    if ([string]$completion.stdout_sha256 -ne (Get-FileHash -Algorithm SHA256 -LiteralPath $stdoutPath).Hash.ToLowerInvariant() -or
        [string]$completion.stderr_sha256 -ne (Get-FileHash -Algorithm SHA256 -LiteralPath $stderrPath).Hash.ToLowerInvariant()) {
        throw 'full-horizon parent-drift evaluator console hashes do not match completion'
    }
    foreach ($record in @($candidates + $controls)) {
        $afterTree = Get-StoreTreeHash -Path ([string]$record.store_root)
        $afterCount = [uint64]@((Get-StoreFileInventory -Path ([string]$record.store_root))).Count
        if ($afterTree -ne [string]$record.store_tree_sha256 -or $afterCount -ne [uint64]$record.store_file_count) {
            throw "training Store changed during parent-drift evaluation: $($record.role) seed-$($record.seed)"
        }
    }
    $resourceSummary = Get-ResourceSummary -Samples @($samples)

    $phase = 'classification'
    $report = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
    if ([string]$report.schema -ne 'regularized-continuation-full-horizon-parent-drift-report/v1' -or
        $report.terminal_outcomes_read -ne $false -or
        [uint64]$report.corpus.evaluation_base_seed -ne 1941001 -or
        [uint64]$report.corpus.pair_count -ne 512 -or
        [uint64]$report.corpus.episode_count -ne 1024 -or
        $report.corpus.all_natural -ne $true -or
        [string]$report.corpus.parent_identity.run_sha256 -ne '2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae' -or
        [string]$report.corpus.parent_identity.checkpoint_manifest_sha256 -ne '4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8' -or
        [string]$report.corpus.parent_identity.checkpoint_payload_sha256 -ne 'a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99' -or
        [string]$report.corpus.parent_identity.model_parameter_sha256 -ne 'db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d') {
        throw 'parent-drift report corpus or terminal-blind identity mismatch'
    }
    $reportArms = @($report.arms)
    if ($reportArms.Count -ne 6) {
        throw 'parent-drift report must contain exactly six arms'
    }
    $ratios = @(
        foreach ($index in 0..2) {
            $seed = [uint64](970001 + $index)
            $candidateArm = $reportArms[$index]
            $controlArm = $reportArms[$index + 3]
            if ([string]$candidateArm.store_root -ne [string]$candidates[$index].store_root -or
                [string]$controlArm.store_root -ne [string]$controls[$index].store_root -or
                [double]$candidateArm.beta -ne 0.1 -or [double]$controlArm.beta -ne 0.0 -or
                $candidateArm.complete -ne $true -or $controlArm.complete -ne $true -or
                $candidateArm.finite -ne $true -or $controlArm.finite -ne $true) {
                throw "parent-drift arm binding mismatch for seed $seed"
            }
            $generationRows = @(
                foreach ($generation in $script:Generations) {
                    $candidateCheckpoint = Get-CheckpointReport -Arm $candidateArm -Generation $generation -Label "candidate seed $seed"
                    $controlCheckpoint = Get-CheckpointReport -Arm $controlArm -Generation $generation -Label "control seed $seed"
                    $candidateKl = [double]$candidateCheckpoint.overall.mean_forward_kl
                    $controlKl = [double]$controlCheckpoint.overall.mean_forward_kl
                    if ($controlKl -le 0.0) {
                        throw "control seed $seed generation-$generation mean KL is zero, so R_g is undefined"
                    }
                    [ordered]@{
                        generation = [uint64]$generation
                        candidate_mean_parent_kl = $candidateKl
                        control_mean_parent_kl = $controlKl
                        R_g = $candidateKl / $controlKl
                        candidate_mean_row_tv = [double]$candidateCheckpoint.overall.mean_row_tv
                        control_mean_row_tv = [double]$controlCheckpoint.overall.mean_row_tv
                        candidate_checkpoint_manifest_sha256 = [string]$candidateCheckpoint.identity.checkpoint_manifest_sha256
                        control_checkpoint_manifest_sha256 = [string]$controlCheckpoint.identity.checkpoint_manifest_sha256
                    }
                }
            )
            $endpoint = @($generationRows | Where-Object { [uint64]$_.generation -eq 512 })[0]
            [ordered]@{
                seed = $seed
                generations = $generationRows
                R_512 = [double]$endpoint.R_g
                late_anchor_loss_trigger = ([double]$endpoint.R_g -ge 0.75)
            }
        }
    )
    $classification = [ordered]@{
        schema = 'regularized-continuation-full-horizon-parent-drift-classification/v1'
        complete = $true
        terminal_outcomes_read = $false
        evaluation_base_seed = [uint64]1941001
        pair_count = [uint64]512
        threshold = [ordered]@{ R_512_late_anchor_loss_minimum = 0.75 }
        seeds = $ratios
        R512 = [ordered]@{
            '970001' = [double]$ratios[0].R_512
            '970002' = [double]$ratios[1].R_512
            '970003' = [double]$ratios[2].R_512
        }
        trigger_seed_count = @($ratios | Where-Object { $_.late_anchor_loss_trigger }).Count
        any_late_anchor_loss_trigger = @($ratios | Where-Object { $_.late_anchor_loss_trigger }).Count -gt 0
        escalation_available = $false
        escalation_unavailable_reason = 'beta 0.1 was the only positive screen-eligible coefficient; no next-larger eligible beta exists'
        source_report = Get-FileRecord -Path $reportPath
        nonclaim = 'KL, TV, and R_g are diagnostics only. They do not measure playing strength or promote a policy.'
    }
    $classificationPath = Join-Path $root 'parent-drift-classification.json'
    Write-Utf8NoBomJsonFile -Value $classification -Path $classificationPath
    $phase = 'complete'
    $manifest = [ordered]@{
        schema = 'regularized-continuation-full-horizon-parent-drift/v1'
        passed = $true
        disposition = 'DIAGNOSTIC-COMPLETE'
        completed_utc = [DateTimeOffset]::UtcNow.ToString('O')
        wall_seconds = $clock.Elapsed.TotalSeconds
        git = $git
        toolchain = $toolchain
        cuda = $cuda
        design = $design
        training = $trainingFile
        executable = $executableFile
        wrapper = $wrapperFile
        request = Get-FileRecord -Path $requestPath
        report = Get-FileRecord -Path $reportPath
        classification = Get-FileRecord -Path $classificationPath
        evaluator_stdout = Get-FileRecord -Path $stdoutPath
        evaluator_stderr = Get-FileRecord -Path $stderrPath
        evaluator_completion = Get-FileRecord -Path $completionPath
        wrapper_stdout = Get-FileRecord -Path $wrapperStdoutPath
        wrapper_stderr = Get-FileRecord -Path $wrapperStderrPath
        resources = $resourceSummary
        started_utc = $started.ToString('O')
        result = $classification
        terminal_outcomes_read = $false
    }
    $manifestPath = Join-Path $root 'parent-drift-manifest.json'
    Write-JsonFile -Value $manifest -Path $manifestPath
    Write-Host "Full-horizon parent drift complete: $manifestPath"
}
catch {
    $line = "$([DateTimeOffset]::UtcNow.ToString('O')) phase=$phase VOID environmental_or_harness_interruption=$($_.Exception.Message)"
    $line | Set-Content -LiteralPath (Join-Path $root "void-$phase.log") -Encoding utf8
    throw
}
