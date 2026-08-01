param(
    [string]$EvidenceRoot = 'D:\mtg-kernel-macro-selfplay-envrand-v2-rung-v1'
)

. (Join-Path $PSScriptRoot 'common.ps1')

$formalPath = Join-Path $EvidenceRoot 'formal-summary.json'
$preflightPath = Join-Path $EvidenceRoot 'preflight\preflight-summary.json'
if (-not (Test-Path -LiteralPath $formalPath)) {
    throw 'formal training summary is required'
}
$formal = Get-Content -LiteralPath $formalPath -Raw | ConvertFrom-Json
$preflight = Get-Content -LiteralPath $preflightPath -Raw | ConvertFrom-Json
if (-not $formal.valid_training) {
    throw 'formal training is not valid'
}
$evaluationRoot = Join-Path $EvidenceRoot 'native-eval'
if (Test-Path -LiteralPath $evaluationRoot) {
    throw "refusing to overwrite existing native evaluation: $evaluationRoot"
}
New-Item -ItemType Directory -Force -Path $evaluationRoot | Out-Null

$results = @()
foreach ($seed in @(970001, 970002, 970003)) {
    $store = Join-Path $EvidenceRoot "runs\seed-$seed\run-0\store"
    $log = Join-Path $evaluationRoot "seed-$seed-vs-promoted2-g384.log"
    $result = Invoke-MacroH2hEvaluation -Executable $preflight.executable -CandidateStore $store -CandidateSeed $seed -EvaluationSeed 977001 -LogPath $log
    $results += $result
}
$passCount = @($results | Where-Object { $_.passes_55_percent }).Count
$floorCount = @($results | Where-Object { $_.wins -ge 1024 }).Count
$macroPass = $passCount -ge 2 -and $floorCount -eq 3
$selected = $results | Sort-Object @{ Expression = 'wins'; Descending = $true }, @{ Expression = 'seed'; Descending = $false } | Select-Object -First 1
$summary = [ordered]@{
    schema = 'macro-selfplay-envrand-v2-native-eval/v1'
    evaluation_seed = 977001
    games_per_seed = 2048
    pass_threshold_wins = 1127
    results = $results
    pass_count = $passCount
    floor_count = $floorCount
    macro_pass = $macroPass
    selected_seed = $selected.seed
    selected_wins = $selected.wins
}
$summary | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $evaluationRoot 'native-eval-summary.json') -Encoding utf8
Write-Host "NATIVE EVAL macro_pass=$macroPass selected_seed=$($selected.seed) wins=$($selected.wins)"
