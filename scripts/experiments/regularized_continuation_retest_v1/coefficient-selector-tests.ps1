$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'coefficient-selector.ps1')

function New-Metrics {
    param([double]$Kl, [double]$Tv, [double]$P99, [double]$Group, [int]$Scale = 1)
    [pscustomobject]@{
        finite = $true
        episode_count = 512 * $Scale
        physical_group_count = 10 * $Scale
        row_count = 12 * $Scale
        choice_row_count = 10 * $Scale
        singleton_row_count = 2 * $Scale
        action_count = 22 * $Scale
        mean_forward_kl = $Kl
        mean_row_tv = $Tv
        p90_row_tv = [math]::Min($P99, 0.1)
        p99_row_tv = $P99
        mean_choice_entropy = 0.5
        mean_choice_max_action_probability = 0.7
        maximum_absolute_selected_group_log_ratio = $Group
    }
}

function New-SyntheticReport {
    $arms = foreach ($beta in $script:CoefficientBetas) {
        $isZero = $beta -eq '0'
        $kl = if ($isZero) { 0.20 } elseif ($beta -eq '0.01') { 0.16 } else { 0.10 }
        $tv = if ($isZero) { 0.20 } else { 0.06 }
        [pscustomobject]@{
            beta = $beta
            store_root = "D:\synthetic\$beta"
            complete = $true
            finite = $true
            checkpoints = @(
                foreach ($generation in $script:CoefficientGenerations) {
                    $p0Metrics = New-Metrics -Kl $kl -Tv $tv -P99 0.10 -Group 0.8
                    $p1Metrics = New-Metrics -Kl $kl -Tv $tv -P99 0.10 -Group 0.8
                    $overallMetrics = New-Metrics -Kl $kl -Tv $tv -P99 0.10 -Group 0.8 -Scale 2
                    [pscustomobject]@{
                        generation = $generation
                        identity = [pscustomobject]@{ checkpoint_sha256 = ('a' * 64) }
                        parameter_l2_from_parent = 0.1
                        overall = $overallMetrics
                        by_learner_seat = @(
                            [pscustomobject]@{ learner_seat = 'P0'; metrics = $p0Metrics },
                            [pscustomobject]@{ learner_seat = 'P1'; metrics = $p1Metrics }
                        )
                    }
                }
            )
        }
    }
    [pscustomobject]@{
        schema = 'regularized-continuation-terminal-blind-report/v1'
        terminal_outcomes_read = $false
        corpus = [pscustomobject]@{
            evaluation_base_seed = 1941001
            pair_count = 512
            episode_count = 1024
            all_natural = $true
            sha256 = ('b' * 64)
            inventory = [pscustomobject]@{
                episode_count = 1024
                physical_group_count = 20
                substep_count = 24
                row_count = 24
                action_count = 44
            }
        }
        arms = @($arms)
    }
}

$report = New-SyntheticReport
$selection = Get-CoefficientSelection -Report $report
if ($selection.selected_beta -ne '0.03') {
    throw "smallest eligible beta test failed: $($selection.selected_beta)"
}

$numericReport = New-SyntheticReport
foreach ($arm in $numericReport.arms) {
    $arm.beta = [double]::Parse([string]$arm.beta, [Globalization.CultureInfo]::InvariantCulture)
}
$numericReport = $numericReport | ConvertTo-Json -Depth 12 | ConvertFrom-Json
$numericSelection = Get-CoefficientSelection -Report $numericReport
if ($numericSelection.selected_beta -ne '0.03') {
    throw "numeric JSON beta matching test failed: $($numericSelection.selected_beta)"
}

$beta03 = Get-ExactArm -Report $report -Beta '0.03'
(Get-ScopeMetrics -Checkpoint (Get-ExactCheckpoint -Arm $beta03 -Generation 32) -Scope 'P1').p99_row_tv = 0.151
$selection = Get-CoefficientSelection -Report $report
if ($selection.selected_beta -ne '0.1') {
    throw "per-seat cap test failed: $($selection.selected_beta)"
}

foreach ($beta in $script:CoefficientBetas | Select-Object -Skip 1) {
    $arm = Get-ExactArm -Report $report -Beta $beta
    foreach ($scope in $script:CoefficientScopes) {
        (Get-ScopeMetrics -Checkpoint (Get-ExactCheckpoint -Arm $arm -Generation 32) -Scope $scope).mean_row_tv = 0.001
    }
}
$selection = Get-CoefficientSelection -Report $report
if ($null -ne $selection.selected_beta -or $selection.disposition -ne 'STOP-NO-ELIGIBLE-COEFFICIENT') {
    throw 'no-eligible-beta test failed'
}

$forbidden = New-SyntheticReport
$forbidden | Add-Member -NotePropertyName winner -NotePropertyValue 'P0'
$blocked = $false
try {
    Get-CoefficientSelection -Report $forbidden | Out-Null
}
catch {
    $blocked = $_.Exception.Message -match 'terminal outcome property is forbidden'
}
if (-not $blocked) {
    throw 'terminal-outcome property rejection test failed'
}

Write-Host 'COEFFICIENT SELECTOR TESTS PASS'
