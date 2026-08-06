param(
    [string]$AttemptRoot = 'D:\mtg-kernel-regularized-continuation-retest-v1\development\seed-1941001\full-horizon-parent-drift\attempt-001'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')

$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
$script:Generations = @([uint64]64, [uint64]128, [uint64]256, [uint64]384, [uint64]512)
$script:ExpectedVoidText = 'phase=diagnostic-evaluation VOID environmental_or_harness_interruption=Argument types do not match'

function Get-FileRecord {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "required file is missing: $Path" }
    $item = Get-Item -LiteralPath $Path
    [ordered]@{
        path = $item.FullName
        bytes = [uint64]$item.Length
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $item.FullName).Hash.ToLowerInvariant()
    }
}

function Assert-FileRecordUnchanged {
    param($Record, [Parameter(Mandatory = $true)][string]$Label)
    $actual = Get-FileRecord -Path ([string]$Record.path)
    if ([string]$actual.sha256 -ne [string]$Record.sha256 -or [uint64]$actual.bytes -ne [uint64]$Record.bytes) {
        throw "$Label file binding changed"
    }
    $actual
}

function Assert-FiniteNonnegative {
    param($Value, [Parameter(Mandatory = $true)][string]$Label)
    $number = [double]$Value
    if ([double]::IsNaN($number) -or [double]::IsInfinity($number) -or $number -lt 0.0) {
        throw "$Label must be finite and nonnegative"
    }
    $number
}

function Get-CheckpointReport {
    param(
        [Parameter(Mandatory = $true)]$Arm,
        [Parameter(Mandatory = $true)][uint64]$Generation,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $matches = @($Arm.checkpoints | Where-Object { [uint64]$_.generation -eq $Generation })
    if ($matches.Count -ne 1) { throw "$Label must contain exactly one generation-$Generation checkpoint" }
    $checkpoint = $matches[0]
    if ($checkpoint.overall.finite -ne $true) { throw "$Label generation-$Generation overall metrics are nonfinite" }
    Assert-FiniteNonnegative $checkpoint.overall.mean_forward_kl "$Label generation-$Generation mean KL" | Out-Null
    Assert-FiniteNonnegative $checkpoint.overall.mean_row_tv "$Label generation-$Generation mean TV" | Out-Null
    foreach ($seat in @('P0', 'P1')) {
        $rows = @($checkpoint.by_learner_seat | Where-Object { [string]$_.learner_seat -eq $seat })
        if ($rows.Count -ne 1 -or $rows[0].metrics.finite -ne $true) {
            throw "$Label generation-$Generation $seat metrics are missing or nonfinite"
        }
    }
    $checkpoint
}

Assert-ExclusiveWindow
$git = Get-GitRecord -RepoRoot $script:RepoRoot
$planPath = Join-Path $AttemptRoot 'parent-drift-plan.json'
$startPath = Join-Path $AttemptRoot 'parent-drift-start.json'
$requestPath = Join-Path $AttemptRoot 'parent-drift-request.json'
$reportPath = Join-Path $AttemptRoot 'parent-drift-report.json'
$stdoutPath = Join-Path $AttemptRoot 'parent-drift-evaluator.stdout.log'
$stderrPath = Join-Path $AttemptRoot 'parent-drift-evaluator.stderr.log'
$completionPath = Join-Path $AttemptRoot 'parent-drift-evaluator.completion.json'
$wrapperStdoutPath = Join-Path $AttemptRoot 'parent-drift-wrapper.stdout.log'
$wrapperStderrPath = Join-Path $AttemptRoot 'parent-drift-wrapper.stderr.log'
$voidPath = Join-Path $AttemptRoot 'void-diagnostic-evaluation.log'
$classificationPath = Join-Path $AttemptRoot 'parent-drift-classification.json'
$manifestPath = Join-Path $AttemptRoot 'parent-drift-manifest.json'
foreach ($path in @($classificationPath, $manifestPath)) {
    if (Test-Path -LiteralPath $path) { throw "recovery output already exists: $path" }
}

$planFile = Get-FileRecord -Path $planPath
$startFile = Get-FileRecord -Path $startPath
$voidFile = Get-FileRecord -Path $voidPath
$voidText = Get-Content -LiteralPath $voidPath -Raw
if ($voidText -notlike "*$($script:ExpectedVoidText)*") {
    throw 'recovery is authorized only for the exact resource-summary collection exception'
}
$plan = Get-Content -LiteralPath $planPath -Raw | ConvertFrom-Json
if ([string]$plan.schema -ne 'regularized-continuation-full-horizon-parent-drift-plan/v1' -or
    $plan.terminal_outcomes_read -ne $false -or
    [string]$plan.design.sha256 -ne '1f6ea9128e7d5b44f80c34c96c42ccd47325224fb11d22bc4aae0d43cef40b00' -or
    [string]$plan.executable.sha256 -ne '4165c7f676907bba8902957d0f8c3d0955eaa2fa41b0a654e08850b9baba2e26') {
    throw 'parent-drift plan identity mismatch'
}
foreach ($binding in @(
    [ordered]@{ record = $plan.design; label = 'design' },
    [ordered]@{ record = $plan.training; label = 'training manifest' },
    [ordered]@{ record = $plan.executable; label = 'evaluator executable' },
    [ordered]@{ record = $plan.wrapper; label = 'evaluator wrapper' },
    [ordered]@{ record = $plan.request; label = 'request' }
)) {
    Assert-FileRecordUnchanged -Record $binding.record -Label $binding.label | Out-Null
}
foreach ($prefix in @('pool_json', 'init_checkpoint', 'init_sidecar', 'init_state')) {
    $path = [string]$plan.inputs."${prefix}_path"
    $sha = [string]$plan.inputs."${prefix}_sha256"
    if ((Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant() -ne $sha) {
        throw "parent-drift input changed: $prefix"
    }
}

$completion = Get-Content -LiteralPath $completionPath -Raw | ConvertFrom-Json
if ([string]$completion.schema -ne 'regularized-continuation-full-horizon-parent-drift-completion/v1' -or
    $completion.success -ne $true -or [int]$completion.native_exit_code -ne 0 -or
    [string]$completion.executable_sha256 -ne [string]$plan.executable.sha256 -or
    [string]$completion.request_sha256 -ne [string]$plan.request.sha256 -or
    $completion.report_created -ne $true -or
    [string]$completion.report_sha256 -ne (Get-FileHash -Algorithm SHA256 -LiteralPath $reportPath).Hash.ToLowerInvariant() -or
    [string]$completion.stdout_sha256 -ne (Get-FileHash -Algorithm SHA256 -LiteralPath $stdoutPath).Hash.ToLowerInvariant() -or
    [string]$completion.stderr_sha256 -ne (Get-FileHash -Algorithm SHA256 -LiteralPath $stderrPath).Hash.ToLowerInvariant()) {
    throw 'parent-drift completion binding mismatch'
}
if ((Get-Process -Id ([int]$completion.wrapper_process_id) -ErrorAction SilentlyContinue) -or
    (Get-Item -LiteralPath $stderrPath).Length -ne 0 -or
    (Get-Item -LiteralPath $wrapperStdoutPath).Length -ne 0 -or
    (Get-Item -LiteralPath $wrapperStderrPath).Length -ne 0 -or
    (Get-Content -LiteralPath $stdoutPath -Raw) -notmatch 'test result: ok\.') {
    throw 'parent-drift process or console postconditions failed'
}

$training = Get-Content -LiteralPath ([string]$plan.training.path) -Raw | ConvertFrom-Json
if ([string]$training.schema -ne 'regularized-continuation-full-horizon-training/v1' -or
    $training.passed -ne $true -or [string]$training.disposition -ne 'TRAINING-COMPLETE; DEVELOPMENT-EVALUATION-RELEASED' -or
    $training.terminal_outcomes_read -ne $false) {
    throw 'bound training manifest is not released'
}
$candidates = @($training.candidates | Sort-Object { [uint64]$_.seed })
$controls = @($training.controls | Sort-Object { [uint64]$_.seed })
if ($candidates.Count -ne 3 -or $controls.Count -ne 3) { throw 'training manifest must bind three candidates and controls' }
foreach ($record in @($candidates + $controls)) {
    $tree = Get-StoreTreeHash -Path ([string]$record.store_root)
    $count = [uint64]@((Get-StoreFileInventory -Path ([string]$record.store_root))).Count
    if ($tree -ne [string]$record.store_tree_sha256 -or $count -ne [uint64]$record.store_file_count) {
        throw "training Store changed: $($record.role) seed-$($record.seed)"
    }
}

$report = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
if ([string]$report.schema -ne 'regularized-continuation-full-horizon-parent-drift-report/v1' -or
    $report.terminal_outcomes_read -ne $false -or
    [uint64]$report.corpus.evaluation_base_seed -ne 1941001 -or
    [uint64]$report.corpus.pair_count -ne 512 -or [uint64]$report.corpus.episode_count -ne 1024 -or
    $report.corpus.all_natural -ne $true -or
    [string]$report.corpus.parent_identity.run_sha256 -ne '2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae' -or
    [string]$report.corpus.parent_identity.checkpoint_manifest_sha256 -ne '4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8' -or
    [string]$report.corpus.parent_identity.checkpoint_payload_sha256 -ne 'a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99' -or
    [string]$report.corpus.parent_identity.model_parameter_sha256 -ne 'db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d') {
    throw 'parent-drift report corpus or parent identity mismatch'
}
$arms = @($report.arms)
if ($arms.Count -ne 6) { throw 'parent-drift report must contain six arms' }
$ratios = @(
    foreach ($index in 0..2) {
        $seed = [uint64](970001 + $index)
        $candidateArm = $arms[$index]
        $controlArm = $arms[$index + 3]
        if ([string]$candidateArm.store_root -ne [string]$candidates[$index].store_root -or
            [string]$controlArm.store_root -ne [string]$controls[$index].store_root -or
            [double]$candidateArm.beta -ne 0.1 -or [double]$controlArm.beta -ne 0.0 -or
            $candidateArm.complete -ne $true -or $controlArm.complete -ne $true -or
            $candidateArm.finite -ne $true -or $controlArm.finite -ne $true) {
            throw "parent-drift arm binding mismatch for seed $seed"
        }
        $rows = @(
            foreach ($generation in $script:Generations) {
                $candidateCheckpoint = Get-CheckpointReport -Arm $candidateArm -Generation $generation -Label "candidate seed $seed"
                $controlCheckpoint = Get-CheckpointReport -Arm $controlArm -Generation $generation -Label "control seed $seed"
                $candidateKl = [double]$candidateCheckpoint.overall.mean_forward_kl
                $controlKl = [double]$controlCheckpoint.overall.mean_forward_kl
                if ($controlKl -le 0.0) { throw "control seed $seed generation-$generation KL is zero" }
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
        $endpoint = @($rows | Where-Object { [uint64]$_.generation -eq 512 })[0]
        [ordered]@{
            seed = $seed
            generations = $rows
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
Write-Utf8NoBomJsonFile -Value $classification -Path $classificationPath
$started = Get-Content -LiteralPath $startPath -Raw | ConvertFrom-Json
$completedUtc = [DateTimeOffset]::Parse([string]$completion.completed_utc)
$startedUtc = [DateTimeOffset]::Parse([string]$started.utc)
$manifest = [ordered]@{
    schema = 'regularized-continuation-full-horizon-parent-drift/v1'
    passed = $true
    disposition = 'DIAGNOSTIC-COMPLETE'
    completed_utc = $completedUtc.ToString('O')
    wall_seconds = ($completedUtc - $startedUtc).TotalSeconds
    git = $plan.git
    recovery_git = $git
    toolchain = $plan.toolchain
    cuda = $plan.cuda
    design = $plan.design
    training = $plan.training
    executable = $plan.executable
    wrapper = $plan.wrapper
    request = Get-FileRecord -Path $requestPath
    report = Get-FileRecord -Path $reportPath
    classification = Get-FileRecord -Path $classificationPath
    evaluator_stdout = Get-FileRecord -Path $stdoutPath
    evaluator_stderr = Get-FileRecord -Path $stderrPath
    evaluator_completion = Get-FileRecord -Path $completionPath
    wrapper_stdout = Get-FileRecord -Path $wrapperStdoutPath
    wrapper_stderr = Get-FileRecord -Path $wrapperStderrPath
    resources = [ordered]@{
        available = $false
        reason = 'live samples were held only in controller memory and were lost in the postprocessing exception'
    }
    started_utc = $startedUtc.ToString('O')
    result = $classification
    recovery = [ordered]@{
        kind = 'postprocessing-only'
        source_void = $voidFile
        source_plan = $planFile
        source_start = $startFile
        exact_exception = 'Argument types do not match'
        diagnostic_replayed = $false
        terminal_outcomes_read = $false
        lost_non_scientific_field = 'per-second live resource samples'
    }
    terminal_outcomes_read = $false
}
Write-JsonFile -Value $manifest -Path $manifestPath
Write-Host "Full-horizon parent-drift recovery PASS: $manifestPath"
