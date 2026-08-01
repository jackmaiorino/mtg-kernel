param(
    [string]$EvidenceRoot = 'D:\mtg-kernel-macro-selfplay-envrand-v2-rung-v1'
)

. (Join-Path $PSScriptRoot 'common.ps1')

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
$preflightPath = Join-Path $EvidenceRoot 'preflight\preflight-summary.json'
if (-not (Test-Path -LiteralPath $preflightPath)) {
    throw 'passing preflight summary is required'
}
$preflight = Get-Content -LiteralPath $preflightPath -Raw | ConvertFrom-Json
if (-not $preflight.passed) {
    throw 'preflight did not pass'
}
$status = @(& git -c "safe.directory=$($repoRoot.Replace('\', '/'))" -C $repoRoot status --porcelain)
if ($LASTEXITCODE -ne 0 -or $status.Count -ne 0) {
    throw 'formal run requires a clean worktree'
}
$sourceCommit = (& git -c "safe.directory=$($repoRoot.Replace('\', '/'))" -C $repoRoot rev-parse HEAD).Trim()
if ($sourceCommit -ne $preflight.source_commit) {
    throw 'source commit changed after preflight'
}
$executable = [string]$preflight.executable
if (-not (Test-Path -LiteralPath $executable)) {
    throw 'preflight executable is missing'
}
$executableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $executable).Hash.ToLowerInvariant()
if ($executableHash -ne $preflight.executable_sha256) {
    throw 'preflight executable hash changed'
}

$runsRoot = Join-Path $EvidenceRoot 'runs'
if (Test-Path -LiteralPath $runsRoot) {
    throw "refusing to overwrite existing formal runs: $runsRoot"
}
New-Item -ItemType Directory -Force -Path $runsRoot | Out-Null
$toolchain = Get-ToolchainRecord
$results = @()
foreach ($seed in @(970001, 970002, 970003)) {
    $gpu = Assert-Gpu1Idle
    $seedRoot = Join-Path $runsRoot "seed-$seed"
    $logPath = Join-Path $runsRoot "seed-$seed.log"
    $started = [DateTimeOffset]::UtcNow
    Invoke-MacroTrainingRun -Executable $executable -Seed $seed -Updates 512 -StoreParent $seedRoot -LogPath $logPath
    $completed = [DateTimeOffset]::UtcNow
    $store = Join-Path $seedRoot 'run-0\store'
    $latest = Get-Content -LiteralPath (Join-Path $store 'latest.json') -Raw | ConvertFrom-Json
    if ([uint64]$latest.generation_index -ne 512) {
        throw "seed $seed did not reach generation 512"
    }
    $manifest = [ordered]@{
        schema = 'macro-selfplay-envrand-v2-run-manifest/v1'
        source_commit = $sourceCommit
        seed = $seed
        updates = 512
        episodes = 32768
        started_utc = $started.ToString('O')
        completed_utc = $completed.ToString('O')
        executable_sha256 = $executableHash
        gpu = $gpu
        toolchain = $toolchain
        pool_json_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $script:PoolJson).Hash.ToLowerInvariant()
        init_generation = $script:InitGeneration
        init_checkpoint_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $script:InitStore 'checkpoints\update-00000384.checkpoint.json')).Hash.ToLowerInvariant()
        init_sidecar_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $script:InitStore 'checkpoints\update-00000384.sidecar.json')).Hash.ToLowerInvariant()
        init_state_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $script:InitStore 'checkpoints\update-00000384.state.f32le')).Hash.ToLowerInvariant()
        store_tree_sha256 = Get-StoreTreeHash -Path $store
        latest_generation = [uint64]$latest.generation_index
        log = $logPath
    }
    $manifestPath = Join-Path $runsRoot "seed-$seed-manifest.json"
    $manifest | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $manifestPath -Encoding utf8
    $results += $manifest
}
$summary = [ordered]@{
    schema = 'macro-selfplay-envrand-v2-formal-summary/v1'
    source_commit = $sourceCommit
    valid_training = $true
    total_updates = 1536
    total_episodes = 98304
    runs = $results
}
$summary | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $EvidenceRoot 'formal-summary.json') -Encoding utf8
Write-Host 'FORMAL TRAINING COMPLETE 3 seeds, 98,304 episodes'
