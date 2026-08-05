param(
    [string]$AttemptRoot = 'D:\mtg-kernel-regularized-continuation-retest-v1\development\seed-1940001\coefficient-screen\attempt-002',
    [string]$EvaluatorTest = 'native_gate3_terminal_blind_coefficient_screen_v1::gate3_terminal_blind_coefficient_screen_v1',
    [switch]$ValidateOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
. (Join-Path $PSScriptRoot 'coefficient-selector.ps1')
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path

function Assert-FileSha256 {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Expected,
        [Parameter(Mandatory = $true)][string]$Label
    )
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "$Label is missing: $Path"
    }
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
    if ($actual -ne $Expected) {
        throw "$Label SHA-256 mismatch"
    }
}

function Get-RecoveredLane {
    param(
        [Parameter(Mandatory = $true)][int]$WaveIndex,
        [Parameter(Mandatory = $true)][string]$Beta,
        [Parameter(Mandatory = $true)][int]$GpuOrdinal,
        [Parameter(Mandatory = $true)][string]$ExecutableSha256
    )
    $label = "wave-$('{0:d2}' -f $WaveIndex)-beta-$($Beta -replace '\.', '_')-gpu$GpuOrdinal"
    $storeParent = Join-Path $AttemptRoot $label
    $logPath = Join-Path $AttemptRoot "$label.log"
    $stdoutPath = Join-Path $AttemptRoot "$label.stdout.log"
    $stderrPath = Join-Path $AttemptRoot "$label.stderr.log"
    $completionPath = Join-Path $AttemptRoot "$label.completion.json"
    foreach ($path in @($storeParent, $logPath, $stdoutPath, $stderrPath, $completionPath)) {
        if (-not (Test-Path -LiteralPath $path)) {
            throw "recovered lane artifact is missing: $path"
        }
    }
    if ((Get-Item -LiteralPath $stderrPath).Length -ne 0) {
        throw "recovered beta=$Beta lane wrote child stderr"
    }
    $completion = Get-Content -LiteralPath $completionPath -Raw | ConvertFrom-Json
    if ([string]$completion.schema -ne 'regularized-continuation-native-lane-completion/v1' -or
        $completion.success -ne $true -or
        [uint64]$completion.seed -ne 1940001 -or
        [uint64]$completion.updates -ne 32 -or
        [int]$completion.gpu_ordinal -ne $GpuOrdinal -or
        [string]$completion.policy_anchor_beta -ne $Beta -or
        [string]$completion.store_parent -ne $storeParent -or
        [string]$completion.log_path -ne $logPath -or
        [string]$completion.executable_sha256 -ne $ExecutableSha256) {
        throw "recovered beta=$Beta lane completion does not match its frozen launch"
    }
    Assert-FileSha256 -Path $logPath -Expected ([string]$completion.log_sha256) -Label "beta=$Beta log"
    $logText = Get-Content -LiteralPath $logPath -Raw
    $escapedBeta = [regex]::Escape($Beta)
    if ($logText -notmatch 'test result: ok\.' -or
        $logText -notmatch 'MULTIRUN CONFIG .*envrand_v2=true' -or
        $logText -notmatch "policy_anchor_beta=$escapedBeta(?:\s|$)" -or
        $logText -notmatch 'MULTIRUN AGGREGATE runs=1 episodes=2048') {
        throw "recovered beta=$Beta lane has an invalid native completion log"
    }
    return [ordered]@{
        wave_index = $WaveIndex
        beta = $Beta
        gpu_ordinal = $GpuOrdinal
        store_parent = $storeParent
        log = [ordered]@{
            path = $logPath
            sha256 = [string]$completion.log_sha256
        }
        stdout = [ordered]@{
            path = $stdoutPath
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $stdoutPath).Hash.ToLowerInvariant()
        }
        stderr = [ordered]@{
            path = $stderrPath
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $stderrPath).Hash.ToLowerInvariant()
        }
        completion = [ordered]@{
            path = $completionPath
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $completionPath).Hash.ToLowerInvariant()
        }
        completed_utc = [string]$completion.completed_utc
        resource_summary = [ordered]@{
            status = 'unavailable'
            reason = 'attempt-002 completed training, then the in-memory OrderedDictionary resource summarizer failed before manifest serialization'
        }
    }
}

function Get-RecoveredArm {
    param([Parameter(Mandatory = $true)]$Lane)
    $beta = [string]$Lane.beta
    $store = Join-Path ([string]$Lane.store_parent) 'run-0\store'
    Assert-GenerationCheckpoint -Store $store -Generation 32
    $checkpoints = foreach ($generation in $script:CoefficientGenerations) {
        $prefix = Join-Path $store ('checkpoints\update-{0:d8}' -f $generation)
        foreach ($suffix in @('checkpoint.json', 'sidecar.json', 'state.f32le')) {
            if (-not (Test-Path -LiteralPath "$prefix.$suffix" -PathType Leaf)) {
                throw "beta=$beta generation=$generation artifact is missing: $prefix.$suffix"
            }
        }
        [ordered]@{
            generation = $generation
            checkpoint = [ordered]@{
                path = "$prefix.checkpoint.json"
                sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath "$prefix.checkpoint.json").Hash.ToLowerInvariant()
            }
            sidecar = [ordered]@{
                path = "$prefix.sidecar.json"
                sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath "$prefix.sidecar.json").Hash.ToLowerInvariant()
            }
            state = [ordered]@{
                path = "$prefix.state.f32le"
                sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath "$prefix.state.f32le").Hash.ToLowerInvariant()
            }
        }
    }
    $authorityPath = Join-Path ([string]$Lane.store_parent) $script:PolicyAnchorAuthorityFileName
    $authority = Get-Content -LiteralPath $authorityPath -Raw | ConvertFrom-Json
    if ([string]$authority.beta -ne $beta) {
        throw "recovered beta=$beta Store authority mismatch"
    }
    return [ordered]@{
        beta = $beta
        store_parent = [string]$Lane.store_parent
        store_root = $store
        store_tree_sha256 = Get-StoreTreeHash -Path $store
        store_file_count = @(Get-StoreFileInventory -Path $store).Count
        policy_anchor_authority = [ordered]@{
            path = $authorityPath
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $authorityPath).Hash.ToLowerInvariant()
        }
        checkpoints = @($checkpoints)
        lane = $Lane
    }
}

$phase = 'recovery-preflight'
try {
    $AttemptRoot = (Resolve-Path -LiteralPath $AttemptRoot).Path
    $planPath = Join-Path $AttemptRoot 'coefficient-plan.json'
    $formalStartPath = Join-Path $AttemptRoot 'formal-start.json'
    $stoppedPath = Join-Path $AttemptRoot 'stopped.log'
    foreach ($path in @($planPath, $formalStartPath, $stoppedPath)) {
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "required recovery input is missing: $path"
        }
    }
    foreach ($name in @('terminal-blind-request.json', 'terminal-blind-report.json', 'terminal-blind-evaluator.log', 'coefficient-manifest.json')) {
        if (Test-Path -LiteralPath (Join-Path $AttemptRoot $name)) {
            throw "recovery output is not create-new: $name"
        }
    }
    $plan = Get-Content -LiteralPath $planPath -Raw | ConvertFrom-Json
    $formalStart = Get-Content -LiteralPath $formalStartPath -Raw | ConvertFrom-Json
    $planSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $planPath).Hash.ToLowerInvariant()
    $stoppedSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $stoppedPath).Hash.ToLowerInvariant()
    $stoppedText = Get-Content -LiteralPath $stoppedPath -Raw
    if ([string]$plan.schema -ne 'regularized-continuation-coefficient-plan/v1' -or
        [string]$formalStart.schema -ne 'regularized-continuation-formal-start/v1' -or
        [string]$formalStart.plan_sha256 -ne $planSha256 -or
        $stoppedText -notmatch 'phase=formal-training stopped=Cannot process argument because the value of argument "Property" is not valid\.') {
        throw 'attempt is not the bound post-training OrderedDictionary summarizer failure'
    }
    if ([uint64]$plan.training_seed -ne 1940001 -or
        [uint64]$plan.validation_seed -ne 1941001 -or
        [uint64]$plan.validation_pairs -ne 512 -or
        [string]$plan.design_commit -ne 'e9bd7e5b4ef7b8320bb22edfc573ba50a8496ba7' -or
        [string]$plan.design_sha256 -ne '1f6ea9128e7d5b44f80c34c96c42ccd47325224fb11d22bc4aae0d43cef40b00' -or
        (@($plan.betas) -join ',') -ne ($script:CoefficientBetas -join ',') -or
        (@($plan.generations | ForEach-Object { [uint64]$_ }) -join ',') -ne ($script:CoefficientGenerations -join ',')) {
        throw 'frozen coefficient plan contract mismatch'
    }
    Assert-ExclusiveWindow
    Assert-Gpu1Idle | Out-Null
    Assert-NoForeignGpu1ComputeProcesses
    $recoveryGit = Get-GitRecord -RepoRoot $script:RepoRoot
    $executable = [string]$plan.executable.path
    Assert-FileSha256 -Path $executable -Expected ([string]$plan.executable.sha256) -Label 'frozen evaluator executable'
    Assert-FileSha256 -Path ([string]$plan.prerequisite_identity.manifest_path) -Expected ([string]$plan.prerequisite_identity.manifest_sha256) -Label 'identity prerequisite manifest'
    Assert-FileSha256 -Path ([string]$plan.prerequisite_throughput.manifest_path) -Expected ([string]$plan.prerequisite_throughput.manifest_sha256) -Label 'throughput prerequisite manifest'
    foreach ($binding in @(
        @([string]$plan.inputs.pool_json_path, [string]$plan.inputs.pool_json_sha256, 'pool JSON'),
        @([string]$plan.inputs.init_checkpoint_path, [string]$plan.inputs.init_checkpoint_sha256, 'parent checkpoint'),
        @([string]$plan.inputs.init_sidecar_path, [string]$plan.inputs.init_sidecar_sha256, 'parent sidecar'),
        @([string]$plan.inputs.init_state_path, [string]$plan.inputs.init_state_sha256, 'parent state')
    )) {
        Assert-FileSha256 -Path $binding[0] -Expected $binding[1] -Label $binding[2]
    }

    $lanes = New-Object System.Collections.Generic.List[object]
    $waveIndex = 0
    foreach ($wave in @($plan.waves)) {
        foreach ($member in @($wave.members)) {
            $lanes.Add((Get-RecoveredLane -WaveIndex $waveIndex -Beta ([string]$member.beta) -GpuOrdinal ([int]$member.gpu) -ExecutableSha256 ([string]$plan.executable.sha256)))
        }
        $waveIndex++
    }
    if ($lanes.Count -ne 5) {
        throw "recovery expected five completed lanes, found $($lanes.Count)"
    }
    $arms = New-Object System.Collections.Generic.List[object]
    foreach ($beta in $script:CoefficientBetas) {
        $matches = @($lanes | Where-Object { [string]$_.beta -eq $beta })
        if ($matches.Count -ne 1) {
            throw "recovery expected exactly one beta=$beta lane"
        }
        $arms.Add((Get-RecoveredArm -Lane $matches[0]))
    }
    if ($ValidateOnly) {
        Write-Host "COEFFICIENT SCREEN RECOVERY VALIDATED stores=$($arms.Count) evidence=$AttemptRoot"
        return
    }

    $parentStore = Split-Path -Parent (Split-Path -Parent ([string]$plan.inputs.init_checkpoint_path))
    $request = [ordered]@{
        schema = 'regularized-continuation-terminal-blind-request/v1'
        parent = [ordered]@{ store_root = $parentStore; generation = [uint64]384 }
        pool_json_path = [string]$plan.inputs.pool_json_path
        evaluation_base_seed = [uint64]$plan.validation_seed
        pair_count = [uint64]$plan.validation_pairs
        arms = @($arms | ForEach-Object {
            [ordered]@{
                beta = $_.beta
                store_root = $_.store_root
                generations = $script:CoefficientGenerations
            }
        })
    }
    $requestPath = Join-Path $AttemptRoot 'terminal-blind-request.json'
    $reportPath = Join-Path $AttemptRoot 'terminal-blind-report.json'
    $evaluationLog = Join-Path $AttemptRoot 'terminal-blind-evaluator.log'
    Write-JsonFile -Value $request -Path $requestPath

    $phase = 'formal-terminal-blind-evaluation'
    $savedInput = [Environment]::GetEnvironmentVariable('REGCONT_SCREEN_INPUT_JSON', 'Process')
    $savedOutput = [Environment]::GetEnvironmentVariable('REGCONT_SCREEN_OUTPUT_JSON', 'Process')
    [Environment]::SetEnvironmentVariable('REGCONT_SCREEN_INPUT_JSON', $requestPath, 'Process')
    [Environment]::SetEnvironmentVariable('REGCONT_SCREEN_OUTPUT_JSON', $reportPath, 'Process')
    try {
        $previous = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try {
            & $executable $EvaluatorTest --ignored --exact --nocapture --test-threads=1 2>&1 |
                Tee-Object -FilePath $evaluationLog |
                Out-Null
            $evaluationExit = $LASTEXITCODE
        }
        finally {
            $ErrorActionPreference = $previous
        }
        Assert-LastExitCode $evaluationExit "terminal-blind evaluator; see $evaluationLog"
    }
    finally {
        [Environment]::SetEnvironmentVariable('REGCONT_SCREEN_INPUT_JSON', $savedInput, 'Process')
        [Environment]::SetEnvironmentVariable('REGCONT_SCREEN_OUTPUT_JSON', $savedOutput, 'Process')
    }
    if (-not (Test-Path -LiteralPath $reportPath -PathType Leaf)) {
        throw 'terminal-blind evaluator did not create its report'
    }
    $report = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
    Assert-CoefficientReport -Report $report
    foreach ($arm in $arms) {
        $reportedArm = Get-ExactArm -Report $report -Beta $arm.beta
        if ([string]$reportedArm.store_root -ne [string]$arm.store_root) {
            throw "beta=$($arm.beta) evaluator Store binding mismatch"
        }
    }
    $selection = Get-CoefficientSelection -Report $report
    $phase = 'complete'
    $manifest = [ordered]@{
        schema = 'regularized-continuation-coefficient-screen/v1'
        passed = $null -ne $selection.selected_beta
        disposition = $selection.disposition
        selected_beta = $selection.selected_beta
        training_seed = [uint64]$plan.training_seed
        validation_seed = [uint64]$plan.validation_seed
        validation_pairs = [uint64]$plan.validation_pairs
        terminal_outcomes_read = $false
        plan = [ordered]@{ path = $planPath; sha256 = $planSha256 }
        request = [ordered]@{ path = $requestPath; sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $requestPath).Hash.ToLowerInvariant() }
        terminal_blind_report = [ordered]@{ path = $reportPath; sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $reportPath).Hash.ToLowerInvariant() }
        evaluator_log = [ordered]@{ path = $evaluationLog; sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $evaluationLog).Hash.ToLowerInvariant() }
        executable = $plan.executable
        git = $plan.git
        prerequisite_identity = $plan.prerequisite_identity
        prerequisite_throughput = $plan.prerequisite_throughput
        recovery = [ordered]@{
            reason = 'post-training OrderedDictionary resource-summary failure before evaluator launch'
            original_stopped_log = [ordered]@{ path = $stoppedPath; sha256 = $stoppedSha256 }
            recovery_git = $recoveryGit
            recovery_script = [ordered]@{
                path = $PSCommandPath
                sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()
            }
            missing_evidence = @('per-second in-memory resource samples and exact wave wall seconds')
            scientific_effect = 'none; all five Stores and launch completions are hash-bound, and the frozen terminal-blind evaluator had not launched'
        }
        training_waves = @(
            for ($index = 0; $index -lt @($plan.waves).Count; $index++) {
                [ordered]@{
                    wave_index = $index
                    wall_seconds = $null
                    lanes = @($lanes | Where-Object { [int]$_.wave_index -eq $index } | ForEach-Object {
                        [ordered]@{
                            beta = $_.beta
                            gpu_ordinal = $_.gpu_ordinal
                            store_parent = $_.store_parent
                            completed_utc = $_.completed_utc
                            completion = $_.completion
                            resource_summary = $_.resource_summary
                        }
                    })
                }
            }
        )
        arms = @($arms | ForEach-Object { $_ })
        selection = $selection
    }
    $manifestPath = Join-Path $AttemptRoot 'coefficient-manifest.json'
    Write-JsonFile -Value $manifest -Path $manifestPath
    $manifestHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $manifestPath).Hash.ToLowerInvariant()
    if ($null -eq $selection.selected_beta) {
        Write-Host "COEFFICIENT SCREEN STOP no eligible beta evidence=$AttemptRoot manifest_sha256=$manifestHash"
    }
    else {
        Write-Host "COEFFICIENT SCREEN PASS beta=$($selection.selected_beta) evidence=$AttemptRoot manifest_sha256=$manifestHash"
    }
}
catch {
    $message = $_.Exception.Message -replace "[\r\n]+", ' '
    "$( [DateTimeOffset]::UtcNow.ToString('O') ) phase=$phase stopped=$message" |
        Set-Content -LiteralPath (Join-Path $AttemptRoot 'resume-stopped.log') -Encoding utf8
    throw
}
