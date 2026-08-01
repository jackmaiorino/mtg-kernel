param(
    [string]$EvidenceRoot = 'D:\mtg-kernel-macro-selfplay-envrand-v2-rung-v1'
)

. (Join-Path $PSScriptRoot 'common.ps1')

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..')).Path
$preflightRoot = Join-Path $EvidenceRoot 'preflight'
if (Test-Path -LiteralPath $preflightRoot) {
    throw "refusing to overwrite existing preflight root: $preflightRoot"
}
New-Item -ItemType Directory -Force -Path $preflightRoot | Out-Null

$status = @(& git -c "safe.directory=$($repoRoot.Replace('\', '/'))" -C $repoRoot status --porcelain)
if ($LASTEXITCODE -ne 0 -or $status.Count -ne 0) {
    throw 'formal preflight requires a clean worktree'
}
$sourceCommit = (& git -c "safe.directory=$($repoRoot.Replace('\', '/'))" -C $repoRoot rev-parse HEAD).Trim()
$gpu = Assert-Gpu1Idle
$executable = Get-ReleaseTestExecutable -RepoRoot $repoRoot -EvidenceRoot $preflightRoot
$executableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $executable).Hash.ToLowerInvariant()

$parentA = Join-Path $preflightRoot 'repeat-a'
$parentB = Join-Path $preflightRoot 'repeat-b'
Invoke-MacroTrainingRun -Executable $executable -Seed 969999 -Updates 1 -StoreParent $parentA -LogPath (Join-Path $preflightRoot 'repeat-a.log')
Assert-Gpu1Idle | Out-Null
Invoke-MacroTrainingRun -Executable $executable -Seed 969999 -Updates 1 -StoreParent $parentB -LogPath (Join-Path $preflightRoot 'repeat-b.log')

$storeA = Join-Path $parentA 'run-0\store'
$storeB = Join-Path $parentB 'run-0\store'
$hashA = Get-StoreTreeHash -Path $storeA
$hashB = Get-StoreTreeHash -Path $storeB
$passed = $hashA -eq $hashB
$summary = [ordered]@{
    schema = 'macro-selfplay-envrand-v2-preflight/v1'
    passed = $passed
    source_commit = $sourceCommit
    executable = $executable
    executable_sha256 = $executableHash
    gpu = $gpu
    pool_json_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $script:PoolJson).Hash.ToLowerInvariant()
    seed = 969999
    updates = 1
    episodes_per_repeat = 64
    store_tree_sha256_a = $hashA
    store_tree_sha256_b = $hashB
    toolchain = Get-ToolchainRecord
}
$summary | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $preflightRoot 'preflight-summary.json') -Encoding utf8
if (-not $passed) {
    throw 'repeat stores are not bit-identical'
}
Write-Host "PREFLIGHT PASS store_tree_sha256=$hashA"
