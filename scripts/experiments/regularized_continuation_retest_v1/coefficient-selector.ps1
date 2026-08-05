Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:CoefficientBetas = @('0', '0.01', '0.03', '0.1', '0.3')
$script:CoefficientGenerations = @([uint64]0, [uint64]8, [uint64]16, [uint64]24, [uint64]32)
$script:CoefficientScopes = @('overall', 'P0', 'P1')

function Test-FiniteNumber {
    param([Parameter(Mandatory = $true)]$Value)
    try {
        $number = [double]$Value
        return -not [double]::IsNaN($number) -and -not [double]::IsInfinity($number)
    }
    catch {
        return $false
    }
}

function Get-ExactArm {
    param([Parameter(Mandatory = $true)]$Report, [Parameter(Mandatory = $true)][string]$Beta)
    $target = [double]::Parse($Beta, [Globalization.CultureInfo]::InvariantCulture)
    $targetBits = [BitConverter]::DoubleToInt64Bits($target)
    $matches = @($Report.arms | Where-Object {
        [BitConverter]::DoubleToInt64Bits([double]$_.beta) -eq $targetBits
    })
    if ($matches.Count -ne 1) {
        throw "expected exactly one beta=$Beta arm, found $($matches.Count)"
    }
    return $matches[0]
}

function Get-ExactCheckpoint {
    param([Parameter(Mandatory = $true)]$Arm, [Parameter(Mandatory = $true)][uint64]$Generation)
    $matches = @($Arm.checkpoints | Where-Object { [uint64]$_.generation -eq $Generation })
    if ($matches.Count -ne 1) {
        throw "beta=$($Arm.beta) expected exactly one generation=$Generation checkpoint, found $($matches.Count)"
    }
    return $matches[0]
}

function Get-ScopeMetrics {
    param(
        [Parameter(Mandatory = $true)]$Checkpoint,
        [Parameter(Mandatory = $true)][ValidateSet('overall', 'P0', 'P1')][string]$Scope
    )
    if ($Scope -eq 'overall') {
        return $Checkpoint.overall
    }
    $matches = @($Checkpoint.by_learner_seat | Where-Object { [string]$_.learner_seat -eq $Scope })
    if ($matches.Count -ne 1) {
        throw "generation=$($Checkpoint.generation) expected exactly one learner-seat=$Scope metric, found $($matches.Count)"
    }
    return $matches[0].metrics
}

function Assert-MetricRecord {
    param(
        [Parameter(Mandatory = $true)]$Metrics,
        [Parameter(Mandatory = $true)][string]$Label
    )
    if ($Metrics.finite -ne $true) {
        throw "$Label is not finite"
    }
    $countFields = @(
        'episode_count', 'physical_group_count', 'row_count',
        'choice_row_count', 'singleton_row_count', 'action_count'
    )
    foreach ($field in $countFields) {
        if ([uint64]$Metrics.$field -lt 0) {
            throw "$Label has an invalid $field"
        }
    }
    if ([uint64]$Metrics.episode_count -eq 0 -or
        [uint64]$Metrics.physical_group_count -eq 0 -or
        [uint64]$Metrics.row_count -eq 0 -or
        [uint64]$Metrics.choice_row_count -eq 0 -or
        [uint64]$Metrics.action_count -lt [uint64]$Metrics.row_count -or
        [uint64]$Metrics.choice_row_count + [uint64]$Metrics.singleton_row_count -ne [uint64]$Metrics.row_count) {
        throw "$Label has an invalid inventory"
    }
    $numericFields = @(
        'mean_forward_kl', 'mean_row_tv', 'p90_row_tv', 'p99_row_tv',
        'mean_choice_entropy', 'mean_choice_max_action_probability',
        'maximum_absolute_selected_group_log_ratio'
    )
    foreach ($field in $numericFields) {
        if (-not (Test-FiniteNumber $Metrics.$field)) {
            throw "$Label has a nonfinite $field"
        }
    }
    if ([double]$Metrics.mean_forward_kl -lt 0.0 -or
        [double]$Metrics.mean_row_tv -lt 0.0 -or [double]$Metrics.mean_row_tv -gt 1.0 -or
        [double]$Metrics.p90_row_tv -lt 0.0 -or [double]$Metrics.p90_row_tv -gt 1.0 -or
        [double]$Metrics.p99_row_tv -lt [double]$Metrics.p90_row_tv -or [double]$Metrics.p99_row_tv -gt 1.0 -or
        [double]$Metrics.mean_choice_entropy -lt 0.0 -or
        [double]$Metrics.mean_choice_max_action_probability -lt 0.0 -or
        [double]$Metrics.mean_choice_max_action_probability -gt 1.0 -or
        [double]$Metrics.maximum_absolute_selected_group_log_ratio -lt 0.0) {
        throw "$Label has an out-of-range metric"
    }
}

function Assert-NoOutcomeProperties {
    param([Parameter(Mandatory = $true)]$Value, [string]$Path = '$')
    $forbidden = @('winner', 'outcome', 'terminal', 'learner_return', 'wins', 'losses', 'draws', 'w', 'l', 'd')
    if ($null -eq $Value) {
        return
    }
    if ($Value -is [string] -or $Value -is [ValueType]) {
        return
    }
    if ($Value -is [System.Collections.IDictionary]) {
        foreach ($key in $Value.Keys) {
            if ([string]$key -in $forbidden) {
                throw "terminal outcome property is forbidden in coefficient report: $Path.$key"
            }
            Assert-NoOutcomeProperties -Value $Value[$key] -Path "$Path.$key"
        }
        return
    }
    if ($Value -is [System.Collections.IEnumerable]) {
        $index = 0
        foreach ($item in $Value) {
            Assert-NoOutcomeProperties -Value $item -Path "$Path[$index]"
            $index++
        }
        return
    }
    foreach ($property in $Value.PSObject.Properties) {
        if ($property.Name -in $forbidden) {
            throw "terminal outcome property is forbidden in coefficient report: $Path.$($property.Name)"
        }
        Assert-NoOutcomeProperties -Value $property.Value -Path "$Path.$($property.Name)"
    }
}

function Assert-CoefficientReport {
    param([Parameter(Mandatory = $true)]$Report)
    Assert-NoOutcomeProperties -Value $Report
    if ([string]$Report.schema -ne 'regularized-continuation-terminal-blind-report/v1' -or
        $Report.terminal_outcomes_read -ne $false -or
        $Report.corpus.all_natural -ne $true -or
        [uint64]$Report.corpus.evaluation_base_seed -ne 1941001 -or
        [uint64]$Report.corpus.pair_count -ne 512 -or
        [uint64]$Report.corpus.episode_count -ne 1024 -or
        [string]$Report.corpus.sha256 -notmatch '^[0-9a-f]{64}$') {
        throw 'terminal-blind coefficient report has an invalid corpus contract'
    }
    if ([uint64]$Report.corpus.inventory.episode_count -ne 1024 -or
        [uint64]$Report.corpus.inventory.physical_group_count -eq 0 -or
        [uint64]$Report.corpus.inventory.substep_count -eq 0 -or
        [uint64]$Report.corpus.inventory.row_count -ne [uint64]$Report.corpus.inventory.substep_count -or
        [uint64]$Report.corpus.inventory.action_count -lt [uint64]$Report.corpus.inventory.row_count) {
        throw 'terminal-blind coefficient report has an invalid corpus inventory'
    }
    if (@($Report.arms).Count -ne $script:CoefficientBetas.Count) {
        throw 'terminal-blind coefficient report does not contain five arms'
    }
    foreach ($beta in $script:CoefficientBetas) {
        $arm = Get-ExactArm -Report $Report -Beta $beta
        if ($arm.complete -ne $true -or $arm.finite -ne $true -or @($arm.checkpoints).Count -ne $script:CoefficientGenerations.Count) {
            throw "beta=$beta is incomplete or nonfinite"
        }
        foreach ($generation in $script:CoefficientGenerations) {
            $checkpoint = Get-ExactCheckpoint -Arm $arm -Generation $generation
            if (-not (Test-FiniteNumber $checkpoint.parameter_l2_from_parent) -or [double]$checkpoint.parameter_l2_from_parent -lt 0.0) {
                throw "beta=$beta generation=$generation has invalid parameter L2"
            }
            foreach ($scope in $script:CoefficientScopes) {
                Assert-MetricRecord -Metrics (Get-ScopeMetrics -Checkpoint $checkpoint -Scope $scope) -Label "beta=$beta generation=$generation scope=$scope"
            }
            $overall = Get-ScopeMetrics -Checkpoint $checkpoint -Scope 'overall'
            $p0 = Get-ScopeMetrics -Checkpoint $checkpoint -Scope 'P0'
            $p1 = Get-ScopeMetrics -Checkpoint $checkpoint -Scope 'P1'
            foreach ($field in @('episode_count', 'physical_group_count', 'row_count', 'choice_row_count', 'singleton_row_count', 'action_count')) {
                if ([uint64]$overall.$field -ne [uint64]$p0.$field + [uint64]$p1.$field) {
                    throw "beta=$beta generation=$generation $field does not reconcile across seats"
                }
            }
            if ([uint64]$overall.episode_count -ne [uint64]$Report.corpus.inventory.episode_count -or
                [uint64]$overall.physical_group_count -ne [uint64]$Report.corpus.inventory.physical_group_count -or
                [uint64]$overall.row_count -ne [uint64]$Report.corpus.inventory.row_count -or
                [uint64]$overall.action_count -ne [uint64]$Report.corpus.inventory.action_count) {
                throw "beta=$beta generation=$generation metric inventory does not match the fixed corpus"
            }
        }
    }
}

function Get-CoefficientSelection {
    param([Parameter(Mandatory = $true)]$Report)
    Assert-CoefficientReport -Report $Report
    $zero = Get-ExactArm -Report $Report -Beta '0'
    $armReads = New-Object System.Collections.Generic.List[object]
    foreach ($beta in $script:CoefficientBetas | Select-Object -Skip 1) {
        $arm = Get-ExactArm -Report $Report -Beta $beta
        $checks = New-Object System.Collections.Generic.List[object]
        foreach ($scope in $script:CoefficientScopes) {
            foreach ($generation in @([uint64]16, [uint64]24, [uint64]32)) {
                $candidateMetrics = Get-ScopeMetrics -Checkpoint (Get-ExactCheckpoint -Arm $arm -Generation $generation) -Scope $scope
                $zeroMetrics = Get-ScopeMetrics -Checkpoint (Get-ExactCheckpoint -Arm $zero -Generation $generation) -Scope $scope
                $limit = 0.75 * [double]$zeroMetrics.mean_forward_kl
                $checks.Add([ordered]@{
                    criterion = 'mean_forward_kl_contraction'
                    scope = $scope
                    generation = $generation
                    value = [double]$candidateMetrics.mean_forward_kl
                    limit = $limit
                    passed = [double]$candidateMetrics.mean_forward_kl -le $limit
                })
            }
            $candidate32 = Get-ScopeMetrics -Checkpoint (Get-ExactCheckpoint -Arm $arm -Generation 32) -Scope $scope
            $zero32 = Get-ScopeMetrics -Checkpoint (Get-ExactCheckpoint -Arm $zero -Generation 32) -Scope $scope
            $movementFloor = [math]::Max(0.005, 0.25 * [double]$zero32.mean_row_tv)
            $p99Cap = [math]::Max(0.150, 0.60 * [double]$zero32.p99_row_tv)
            $groupCap = [math]::Max(1.0, 0.75 * [double]$zero32.maximum_absolute_selected_group_log_ratio)
            $checks.Add([ordered]@{
                criterion = 'mean_row_tv_movement_floor'; scope = $scope; generation = [uint64]32
                value = [double]$candidate32.mean_row_tv; limit = $movementFloor
                passed = [double]$candidate32.mean_row_tv -ge $movementFloor
            })
            $checks.Add([ordered]@{
                criterion = 'p99_row_tv_cap'; scope = $scope; generation = [uint64]32
                value = [double]$candidate32.p99_row_tv; limit = $p99Cap
                passed = [double]$candidate32.p99_row_tv -le $p99Cap
            })
            $checks.Add([ordered]@{
                criterion = 'maximum_absolute_selected_group_log_ratio_cap'; scope = $scope; generation = [uint64]32
                value = [double]$candidate32.maximum_absolute_selected_group_log_ratio; limit = $groupCap
                passed = [double]$candidate32.maximum_absolute_selected_group_log_ratio -le $groupCap
            })
        }
        $failed = @($checks | Where-Object { $_.passed -ne $true })
        $armReads.Add([ordered]@{
            beta = $beta
            eligible = $failed.Count -eq 0
            checks = @($checks | ForEach-Object { $_ })
            failed_check_count = $failed.Count
        })
    }
    $eligible = @($armReads | Where-Object { $_.eligible -eq $true })
    $selectedBeta = if ($eligible.Count -eq 0) { $null } else { [string]$eligible[0].beta }
    return [ordered]@{
        selected_beta = $selectedBeta
        positive_arm_reads = @($armReads | ForEach-Object { $_ })
        disposition = if ($null -eq $selectedBeta) { 'STOP-NO-ELIGIBLE-COEFFICIENT' } else { 'SELECTED' }
    }
}
