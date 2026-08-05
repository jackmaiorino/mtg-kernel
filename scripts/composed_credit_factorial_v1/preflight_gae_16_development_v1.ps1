param(
    [Parameter(Mandatory = $true)]
    [string]$ManifestPath
)

$ErrorActionPreference = 'Stop'

function Require-Condition {
    param(
        [bool]$Condition,
        [string]$Message
    )
    if (-not $Condition) {
        throw $Message
    }
}

function Require-Equal {
    param(
        $Actual,
        $Expected,
        [string]$Field
    )
    if ($Actual -ne $Expected) {
        throw "$Field mismatch: actual=$Actual expected=$Expected"
    }
}

function Tool-Field {
    param(
        [string[]]$Lines,
        [string]$Prefix
    )
    $line = $Lines | Where-Object { $_.StartsWith($Prefix) } | Select-Object -First 1
    Require-Condition ($null -ne $line) "missing toolchain field $Prefix"
    return $line.Substring($Prefix.Length).Trim()
}

$manifestResolved = (Resolve-Path -LiteralPath $ManifestPath).Path
$manifest = Get-Content -Raw -LiteralPath $manifestResolved | ConvertFrom-Json
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path

Require-Equal $manifest.schema 'mtg-kernel-current-net8-gae-16-update-preflight/v1' 'schema'
Require-Equal $manifest.status 'ready-not-launched' 'status'

$gitStatus = @(git -C $repoRoot status --porcelain 2>&1)
Require-Equal $LASTEXITCODE 0 'git status exit code'
Require-Equal $gitStatus.Count 0 'worktree dirty-line count'
$gitHead = (git -C $repoRoot rev-parse HEAD 2>&1).Trim()
Require-Equal $LASTEXITCODE 0 'git rev-parse exit code'
Require-Equal $manifest.git_commit $gitHead 'git_commit'

$rustcLines = @(& rustc -Vv 2>&1 | ForEach-Object { $_.ToString() })
Require-Equal $LASTEXITCODE 0 'rustc -Vv exit code'
$cargoLines = @(& cargo -Vv 2>&1 | ForEach-Object { $_.ToString() })
Require-Equal $LASTEXITCODE 0 'cargo -Vv exit code'
Require-Equal $manifest.toolchain.rustc_release (Tool-Field $rustcLines 'release:') 'toolchain.rustc_release'
Require-Equal $manifest.toolchain.rustc_commit_hash (Tool-Field $rustcLines 'commit-hash:') 'toolchain.rustc_commit_hash'
Require-Equal $manifest.toolchain.host (Tool-Field $rustcLines 'host:') 'toolchain.host'
Require-Equal $manifest.toolchain.llvm_version (Tool-Field $rustcLines 'LLVM version:') 'toolchain.llvm_version'
Require-Equal $manifest.toolchain.cargo_release (Tool-Field $cargoLines 'release:') 'toolchain.cargo_release'
Require-Equal $manifest.toolchain.cargo_commit_hash (Tool-Field $cargoLines 'commit-hash:') 'toolchain.cargo_commit_hash'

$linkerPath = (Get-Command link.exe -ErrorAction Stop).Source
$linkerItem = Get-Item -LiteralPath $linkerPath
$linkerHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $linkerPath).Hash.ToLowerInvariant()
Require-Condition ([StringComparer]::OrdinalIgnoreCase.Equals($manifest.toolchain.linker.path, $linkerPath)) 'toolchain.linker.path mismatch'
Require-Equal $manifest.toolchain.linker.file_version $linkerItem.VersionInfo.FileVersion 'toolchain.linker.file_version'
Require-Equal $manifest.toolchain.linker.sha256 $linkerHash 'toolchain.linker.sha256'

$executablePath = (Resolve-Path -LiteralPath $manifest.executable.path).Path
$executableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $executablePath).Hash.ToLowerInvariant()
Require-Equal $manifest.executable.sha256 $executableHash 'executable.sha256'
Require-Equal $manifest.executable.byte_count (Get-Item -LiteralPath $executablePath).Length 'executable.byte_count'

$gpuLines = @(nvidia-smi --query-gpu=index,uuid,name,utilization.gpu,memory.used --format=csv,noheader,nounits 2>&1)
Require-Equal $LASTEXITCODE 0 'nvidia-smi exit code'
$gpuFields = $null
foreach ($gpuLine in $gpuLines) {
    $fields = @($gpuLine -split ',' | ForEach-Object { $_.Trim() })
    if ($fields[0] -eq '1') {
        $gpuFields = $fields
        break
    }
}
Require-Condition ($null -ne $gpuFields) 'GPU ordinal 1 missing'
Require-Equal $manifest.gpu.ordinal ([int]$gpuFields[0]) 'gpu.ordinal'
Require-Equal $manifest.gpu.uuid $gpuFields[1] 'gpu.uuid'
Require-Equal $manifest.gpu.name $gpuFields[2] 'gpu.name'
Require-Condition ([int]$gpuFields[3] -le [int]$manifest.gpu.maximum_prelaunch_utilization_percent) 'GPU 1 utilization exceeds prelaunch limit'
Require-Condition ([int]$gpuFields[4] -le [int]$manifest.gpu.maximum_prelaunch_memory_mib) 'GPU 1 memory exceeds prelaunch limit'

Require-Equal $manifest.inputs.source_checkpoint_state_sha256 '00333d987584d5cf7f9a37f1ba2b558cfd22a60388f2487c1bf1623fcc6686a0' 'inputs.source_checkpoint_state_sha256'
Require-Equal $manifest.inputs.source_run_sha256 '2307caf5a0093bf3f6f9d3673788eac1d73bcd248bfb6fcb3af785a596304cab' 'inputs.source_run_sha256'
Require-Equal $manifest.inputs.pool_sha256 '6c3c8ff09ab519dc9f462b41cbf898da902d230656d14e64d79fc66a19f3bc71' 'inputs.pool_sha256'
Require-Equal $manifest.inputs.critic_composite_model_parameter_sha256 '6329233bcc22f7941e8085ef0235107eb75293fe74c727434c0474da15354f22' 'inputs.critic_composite_model_parameter_sha256'
Require-Condition (Test-Path -LiteralPath $manifest.inputs.critic_root -PathType Container) 'inputs.critic_root missing'

Require-Equal $manifest.training.base_seed 970001 'training.base_seed'
Require-Equal $manifest.training.first_episode_index 32768 'training.first_episode_index'
Require-Equal $manifest.training.last_episode_index 33791 'training.last_episode_index'
Require-Equal $manifest.training.update_count 16 'training.update_count'
Require-Equal $manifest.training.episodes_per_update 64 'training.episodes_per_update'
Require-Equal $manifest.evaluation.first_episode_index 98304 'evaluation.first_episode_index'
Require-Equal $manifest.evaluation.last_episode_index 99327 'evaluation.last_episode_index'
Require-Equal $manifest.evaluation.episode_count_per_arm 1024 'evaluation.episode_count_per_arm'
Require-Equal $manifest.evaluation.cluster_count 512 'evaluation.cluster_count'
Require-Equal $manifest.evaluation.canonical_schedule_sha256 'b8177bbfec80ec1f57a2d9672d94d3ffd0f02cacccc68f293bb1e151c0958441' 'evaluation.canonical_schedule_sha256'
Require-Equal $manifest.topology.worker_count 4 'topology.worker_count'
Require-Equal $manifest.topology.sessions_per_worker 16 'topology.sessions_per_worker'
Require-Equal $manifest.topology.logical_actor_count 64 'topology.logical_actor_count'

$expectedOutputRoot = 'D:\mtg-kernel-composed-factorial-v1\gae-16-update-development-v1'
Require-Condition ([StringComparer]::OrdinalIgnoreCase.Equals($manifest.output.root, $expectedOutputRoot)) 'output.root mismatch'
Require-Condition (-not (Test-Path -LiteralPath (Join-Path $expectedOutputRoot 'fresh-eval-v1'))) 'fresh evaluation output already exists'

$manifestHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $manifestResolved).Hash.ToLowerInvariant()
[pscustomobject]@{
    schema = 'mtg-kernel-current-net8-gae-16-update-preflight-result/v1'
    status = 'pass'
    git_commit = $gitHead
    executable_sha256 = $executableHash
    manifest_sha256 = $manifestHash
    gpu_uuid = $gpuFields[1]
    evaluation_schedule_sha256 = $manifest.evaluation.canonical_schedule_sha256
} | ConvertTo-Json -Compress
