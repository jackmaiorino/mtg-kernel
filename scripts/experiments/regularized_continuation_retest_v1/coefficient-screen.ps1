param(
    [string]$EvidenceRoot = 'D:\mtg-kernel-regularized-continuation-retest-v1\development\seed-1940001',
    [string]$PrerequisiteRoot = 'D:\mtg-kernel-regularized-continuation-retest-v1\preflight\seed-969999',
    [string]$EvaluatorTest = 'native_gate3_terminal_blind_coefficient_screen_v1::gate3_terminal_blind_coefficient_screen_v1'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
. (Join-Path $PSScriptRoot 'coefficient-selector.ps1')
$script:RepoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
$root = New-UniqueAttemptRoot -EvidenceRoot $EvidenceRoot -GateName 'coefficient-screen'

function Get-LaneManifestRecord {
    param([Parameter(Mandatory = $true)]$Lane)
    $samples = @($Lane.resource_samples)
    $gpuSummaries = foreach ($ordinal in @(0, 1)) {
        $rows = @($samples | ForEach-Object { $_.gpus } | Where-Object { $_.ordinal -eq $ordinal })
        if ($rows.Count -gt 0) {
            [ordered]@{
                ordinal = $ordinal
                sample_count = $rows.Count
                utilization_mean_percent = ($rows | Measure-Object -Property utilization_percent -Average).Average
                utilization_peak_percent = ($rows | Measure-Object -Property utilization_percent -Maximum).Maximum
                memory_peak_mib = ($rows | Measure-Object -Property memory_used_mib -Maximum).Maximum
            }
        }
    }
    $memoryFractions = @($samples | Where-Object { $_.host_memory_total_mib -gt 0 } | ForEach-Object {
        $_.host_memory_used_mib / [double]$_.host_memory_total_mib
    })
    return [ordered]@{
        gpu_ordinal = $Lane.gpu_ordinal
        store_parent = $Lane.store_parent
        log = [ordered]@{
            path = $Lane.log
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Lane.log).Hash.ToLowerInvariant()
        }
        stdout = [ordered]@{
            path = $Lane.stdout
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Lane.stdout).Hash.ToLowerInvariant()
        }
        stderr = [ordered]@{
            path = $Lane.stderr
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $Lane.stderr).Hash.ToLowerInvariant()
        }
        completion = $Lane.completion
        started_utc = $Lane.started_utc
        completed_utc = $Lane.completed_utc
        wall_seconds = $Lane.wall_seconds
        exit_code = $Lane.exit_code
        resource_summary = [ordered]@{
            sample_count = $samples.Count
            cpu_mean_percent = if ($samples.Count -eq 0) { $null } else { ($samples | Measure-Object -Property cpu_total_percent -Average).Average }
            cpu_peak_percent = if ($samples.Count -eq 0) { $null } else { ($samples | Measure-Object -Property cpu_total_percent -Maximum).Maximum }
            host_memory_peak_fraction = if ($memoryFractions.Count -eq 0) { $null } else { ($memoryFractions | Measure-Object -Maximum).Maximum }
            gpus = @($gpuSummaries)
        }
    }
}

function Get-ArmStoreRecord {
    param(
        [Parameter(Mandatory = $true)][string]$Beta,
        [Parameter(Mandatory = $true)][string]$StoreParent,
        [Parameter(Mandatory = $true)]$Lane
    )
    $store = Join-Path $StoreParent 'run-0\store'
    Assert-GenerationCheckpoint -Store $store -Generation 32
    $checkpoints = foreach ($generation in $script:CoefficientGenerations) {
        $prefix = Join-Path $store ('checkpoints\update-{0:d8}' -f $generation)
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
    $authorityPath = Join-Path $StoreParent $script:PolicyAnchorAuthorityFileName
    $authority = Get-Content -LiteralPath $authorityPath -Raw | ConvertFrom-Json
    if ([string]$authority.beta -ne $Beta) {
        throw "beta=$Beta Store authority mismatch"
    }
    return [ordered]@{
        beta = $Beta
        store_parent = $StoreParent
        store_root = $store
        store_tree_sha256 = Get-StoreTreeHash -Path $store
        store_file_count = @(Get-StoreFileInventory -Path $store).Count
        policy_anchor_authority = [ordered]@{
            path = $authorityPath
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $authorityPath).Hash.ToLowerInvariant()
        }
        checkpoints = @($checkpoints)
        lane = Get-LaneManifestRecord -Lane $Lane
    }
}

function Invoke-TrainingWave {
    param(
        [Parameter(Mandatory = $true)][object[]]$Members,
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][int]$WaveIndex
    )
    $clock = [System.Diagnostics.Stopwatch]::StartNew()
    $lanes = New-Object System.Collections.Generic.List[object]
    try {
        foreach ($member in $Members) {
            $label = "wave-$('{0:d2}' -f $WaveIndex)-beta-$([string]$member.beta -replace '\.', '_')-gpu$($member.gpu)"
            $storeParent = Join-Path $root $label
            $lane = Start-NativeLane -Executable $Executable -Seed 1940001 -Updates 32 -StoreParent $storeParent -GpuOrdinal $member.gpu -PolicyAnchorBeta $member.beta -LogPath (Join-Path $root "$label.log") -EvidenceRoot $root
            $lanes.Add($lane)
        }
        $results = @($lanes | ForEach-Object { Wait-NativeLane -Lane $_ })
        $clock.Stop()
        return [ordered]@{
            wave_index = $WaveIndex
            wall_seconds = $clock.Elapsed.TotalSeconds
            lanes = @($results)
        }
    }
    catch {
        foreach ($lane in $lanes) {
            Stop-NativeLane -Lane $lane
        }
        throw
    }
}

$phase = 'preflight'
try {
    Assert-ExclusiveWindow
    $git = Get-GitRecord -RepoRoot $script:RepoRoot
    $toolchain = Get-ToolchainRecord
    $cuda = Get-CudaRecord
    $inputs = Get-InputRecord
    $gpu1 = Assert-Gpu1Idle
    $gpu0 = Assert-GpuIdentity -Ordinal 0
    $prelaunchResources = Assert-PrelaunchResourceWindow
    Assert-NoForeignGpu1ComputeProcesses
    $executable = Get-ReleaseTestExecutable -RepoRoot $script:RepoRoot -EvidenceRoot $root -Label 'coefficient-screen'
    $executableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $executable).Hash.ToLowerInvariant()
    $identity = Get-PassedIdentityPrerequisite -EvidenceRoot $PrerequisiteRoot -CandidateCommit $git.commit -CandidateExecutableSha256 $executableHash -RepoRoot $script:RepoRoot
    $throughput = Get-PassedThroughputPrerequisite -EvidenceRoot $PrerequisiteRoot -CandidateCommit $git.commit -IdentityManifestSha256 $identity.manifest_sha256

    $armPlan = @(
        [ordered]@{ beta = '0'; ordinal = 0 },
        [ordered]@{ beta = '0.01'; ordinal = 1 },
        [ordered]@{ beta = '0.03'; ordinal = 2 },
        [ordered]@{ beta = '0.1'; ordinal = 3 },
        [ordered]@{ beta = '0.3'; ordinal = 4 }
    )
    $waves = New-Object System.Collections.Generic.List[object]
    if ($throughput.selected_topology -eq 'gpu0+gpu1') {
        $waves.Add([pscustomobject]@{ members = @(
            [pscustomobject]@{ beta = '0'; gpu = 0 },
            [pscustomobject]@{ beta = '0.01'; gpu = 1 }
        ) })
        $waves.Add([pscustomobject]@{ members = @(
            [pscustomobject]@{ beta = '0.03'; gpu = 0 },
            [pscustomobject]@{ beta = '0.1'; gpu = 1 }
        ) })
        $waves.Add([pscustomobject]@{ members = @(
            [pscustomobject]@{ beta = '0.3'; gpu = 1 }
        ) })
    }
    else {
        foreach ($arm in $armPlan) {
            $waves.Add([pscustomobject]@{ members = @([pscustomobject]@{ beta = $arm.beta; gpu = 1 }) })
        }
    }
    $plan = [ordered]@{
        schema = 'regularized-continuation-coefficient-plan/v1'
        status = 'preflight-complete; formal measurement not yet started'
        design_commit = 'e9bd7e5b4ef7b8320bb22edfc573ba50a8496ba7'
        design_sha256 = '1f6ea9128e7d5b44f80c34c96c42ccd47325224fb11d22bc4aae0d43cef40b00'
        git = $git
        prerequisite_identity = $identity
        prerequisite_throughput = $throughput
        toolchain = $toolchain
        cuda = $cuda
        executable = [ordered]@{ path = $executable; sha256 = $executableHash }
        inputs = $inputs
        prelaunch_gpus = @($gpu0, $gpu1)
        prelaunch_resources = $prelaunchResources
        training_seed = 1940001
        validation_seed = 1941001
        validation_pairs = 512
        generations = $script:CoefficientGenerations
        betas = $script:CoefficientBetas
        topology = $throughput.selected_topology
        waves = @($waves | ForEach-Object { $_ })
        selector = [ordered]@{
            scopes = $script:CoefficientScopes
            kl_generations = @(16, 24, 32)
            mean_forward_kl_ratio_cap = 0.75
            update32_mean_tv_relative_floor = 0.25
            update32_mean_tv_absolute_floor = 0.005
            update32_p99_tv_absolute_or_relative_cap = @(0.150, 0.60)
            update32_max_group_log_ratio_absolute_or_relative_cap = @(1.0, 0.75)
            rule = 'select the smallest eligible positive beta after every arm and checkpoint is complete; no gameplay outcome is read'
        }
    }
    $planPath = Join-Path $root 'coefficient-plan.json'
    Write-JsonFile -Value $plan -Path $planPath

    $phase = 'formal-training'
    $formalStarted = [ordered]@{
        schema = 'regularized-continuation-formal-start/v1'
        utc = [DateTimeOffset]::UtcNow.ToString('O')
        plan_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $planPath).Hash.ToLowerInvariant()
    }
    Write-JsonFile -Value $formalStarted -Path (Join-Path $root 'formal-start.json')
    $trainingWaves = New-Object System.Collections.Generic.List[object]
    $waveIndex = 0
    foreach ($wave in $waves) {
        $trainingWaves.Add((Invoke-TrainingWave -Members @($wave.members) -Executable $executable -WaveIndex $waveIndex))
        $waveIndex++
    }
    Assert-Gpu1Idle | Out-Null
    Assert-NoForeignGpu1ComputeProcesses

    $armRecords = New-Object System.Collections.Generic.List[object]
    foreach ($beta in $script:CoefficientBetas) {
        $lane = @($trainingWaves | ForEach-Object { $_.lanes } | Where-Object {
            [string]$_.store_parent -like "*beta-$($beta -replace '\.', '_')-*"
        })
        if ($lane.Count -ne 1) {
            throw "beta=$beta expected exactly one completed training lane"
        }
        $armRecords.Add((Get-ArmStoreRecord -Beta $beta -StoreParent $lane[0].store_parent -Lane $lane[0]))
    }

    $request = [ordered]@{
        schema = 'regularized-continuation-terminal-blind-request/v1'
        parent = [ordered]@{ store_root = $script:InitStore; generation = $script:InitGeneration }
        pool_json_path = $script:PoolJson
        evaluation_base_seed = 1941001
        pair_count = 512
        arms = @($armRecords | ForEach-Object {
            [ordered]@{
                beta = $_.beta
                store_root = $_.store_root
                generations = $script:CoefficientGenerations
            }
        })
    }
    $requestPath = Join-Path $root 'terminal-blind-request.json'
    $reportPath = Join-Path $root 'terminal-blind-report.json'
    Write-JsonFile -Value $request -Path $requestPath
    if (Test-Path -LiteralPath $reportPath) {
        throw 'terminal-blind evaluator output path is not create-new'
    }
    $phase = 'formal-terminal-blind-evaluation'
    $savedInput = [Environment]::GetEnvironmentVariable('REGCONT_SCREEN_INPUT_JSON', 'Process')
    $savedOutput = [Environment]::GetEnvironmentVariable('REGCONT_SCREEN_OUTPUT_JSON', 'Process')
    [Environment]::SetEnvironmentVariable('REGCONT_SCREEN_INPUT_JSON', $requestPath, 'Process')
    [Environment]::SetEnvironmentVariable('REGCONT_SCREEN_OUTPUT_JSON', $reportPath, 'Process')
    $evaluationLog = Join-Path $root 'terminal-blind-evaluator.log'
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
    foreach ($armRecord in $armRecords) {
        $reportedArm = Get-ExactArm -Report $report -Beta $armRecord.beta
        if ([string]$reportedArm.store_root -ne [string]$armRecord.store_root) {
            throw "beta=$($armRecord.beta) evaluator Store binding mismatch"
        }
    }
    $selection = Get-CoefficientSelection -Report $report
    $phase = 'complete'
    $manifest = [ordered]@{
        schema = 'regularized-continuation-coefficient-screen/v1'
        passed = $null -ne $selection.selected_beta
        disposition = $selection.disposition
        selected_beta = $selection.selected_beta
        training_seed = 1940001
        validation_seed = 1941001
        validation_pairs = 512
        terminal_outcomes_read = $false
        plan = [ordered]@{
            path = $planPath
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $planPath).Hash.ToLowerInvariant()
        }
        request = [ordered]@{
            path = $requestPath
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $requestPath).Hash.ToLowerInvariant()
        }
        terminal_blind_report = [ordered]@{
            path = $reportPath
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $reportPath).Hash.ToLowerInvariant()
        }
        evaluator_log = [ordered]@{
            path = $evaluationLog
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $evaluationLog).Hash.ToLowerInvariant()
        }
        executable = [ordered]@{ path = $executable; sha256 = $executableHash }
        git = $git
        prerequisite_identity = $identity
        prerequisite_throughput = $throughput
        training_waves = @($trainingWaves | ForEach-Object {
            [ordered]@{
                wave_index = $_.wave_index
                wall_seconds = $_.wall_seconds
                lanes = @($_.lanes | ForEach-Object {
                    [ordered]@{
                        gpu_ordinal = $_.gpu_ordinal
                        store_parent = $_.store_parent
                        wall_seconds = $_.wall_seconds
                        completion = $_.completion
                    }
                })
            }
        })
        arms = @($armRecords | ForEach-Object { $_ })
        selection = $selection
    }
    $manifestPath = Join-Path $root 'coefficient-manifest.json'
    Write-JsonFile -Value $manifest -Path $manifestPath
    $manifestHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $manifestPath).Hash.ToLowerInvariant()
    if ($null -eq $selection.selected_beta) {
        Write-Host "COEFFICIENT SCREEN STOP no eligible beta evidence=$root manifest_sha256=$manifestHash"
    }
    else {
        Write-Host "COEFFICIENT SCREEN PASS beta=$($selection.selected_beta) evidence=$root manifest_sha256=$manifestHash"
    }
}
catch {
    $message = $_.Exception.Message -replace "[\r\n]+", ' '
    "$( [DateTimeOffset]::UtcNow.ToString('O') ) phase=$phase stopped=$message" |
        Set-Content -LiteralPath (Join-Path $root 'stopped.log') -Encoding utf8
    throw
}
