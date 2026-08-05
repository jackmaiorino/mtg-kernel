param(
    [Parameter(Mandatory = $true)]
    [string]$ManifestPath
)

$ErrorActionPreference = 'Stop'

function Require-Condition {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) {
        throw $Message
    }
}

function Require-Equal {
    param($Actual, $Expected, [string]$Field)
    if ($Actual -ne $Expected) {
        throw "$Field mismatch: actual=$Actual expected=$Expected"
    }
}

function Tool-Field {
    param([string[]]$Lines, [string]$Prefix)
    $line = $Lines | Where-Object { $_.StartsWith($Prefix) } | Select-Object -First 1
    Require-Condition ($null -ne $line) "missing toolchain field $Prefix"
    return $line.Substring($Prefix.Length).Trim()
}

$manifestResolved = (Resolve-Path -LiteralPath $ManifestPath).Path
$manifest = Get-Content -Raw -LiteralPath $manifestResolved | ConvertFrom-Json
$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path

Require-Equal $manifest.schema 'mtg-kernel-current-net8-population-response-preflight/v1' 'schema'
Require-Equal $manifest.status 'ready-not-launched' 'status'

$gitStatus = @(git -C $repoRoot status --porcelain 2>&1)
Require-Equal $LASTEXITCODE 0 'git status exit code'
Require-Equal $gitStatus.Count 0 'worktree dirty-line count'
$gitHead = (git -C $repoRoot rev-parse HEAD 2>&1).Trim()
Require-Equal $LASTEXITCODE 0 'git rev-parse exit code'
Require-Equal $manifest.git_commit $gitHead 'git_commit'

$designPath = (Resolve-Path -LiteralPath (Join-Path $repoRoot $manifest.design.path)).Path
$designHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $designPath).Hash.ToLowerInvariant()
Require-Equal $manifest.design.sha256 $designHash 'design.sha256'

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

$linkerPath = (Resolve-Path -LiteralPath $manifest.toolchain.linker.path).Path
$linkerItem = Get-Item -LiteralPath $linkerPath
$linkerHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $linkerPath).Hash.ToLowerInvariant()
Require-Equal $manifest.toolchain.linker.path $linkerPath 'toolchain.linker.path'
Require-Equal $manifest.toolchain.linker.file_version $linkerItem.VersionInfo.FileVersion 'toolchain.linker.file_version'
Require-Equal $manifest.toolchain.linker.sha256 $linkerHash 'toolchain.linker.sha256'

$executablePath = (Resolve-Path -LiteralPath $manifest.executable.path).Path
$executableItem = Get-Item -LiteralPath $executablePath
$executableHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $executablePath).Hash.ToLowerInvariant()
Require-Equal $manifest.executable.sha256 $executableHash 'executable.sha256'
Require-Equal $manifest.executable.byte_count $executableItem.Length 'executable.byte_count'

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

$initializerPath = (Resolve-Path -LiteralPath $manifest.inputs.initializer.path).Path
$initializerHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $initializerPath).Hash.ToLowerInvariant()
Require-Equal $manifest.inputs.initializer.payload_sha256 $initializerHash 'inputs.initializer.payload_sha256'
Require-Equal $manifest.inputs.initializer.byte_count (Get-Item -LiteralPath $initializerPath).Length 'inputs.initializer.byte_count'
Require-Equal $manifest.inputs.initializer.native_state_sha256 'ab7dd25ca6619a4a613ca089e1eb8e75981f8e5cfc0bae8535b78cddd7efa952' 'inputs.initializer.native_state_sha256'
Require-Equal $manifest.inputs.initializer.model_parameter_sha256 '5efe2f167045bde379da3be8af6c480b6702f5d7a849ff8435d8ac6b1d91daa8' 'inputs.initializer.model_parameter_sha256'
Require-Equal $manifest.inputs.historical_parent_native_state_sha256 '00333d987584d5cf7f9a37f1ba2b558cfd22a60388f2487c1bf1623fcc6686a0' 'inputs.historical_parent_native_state_sha256'
Require-Equal $manifest.inputs.retained_pool3_primary_native_state_sha256 'a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99' 'inputs.retained_pool3_primary_native_state_sha256'
Require-Equal $manifest.inputs.pool_sha256 '6c3c8ff09ab519dc9f462b41cbf898da902d230656d14e64d79fc66a19f3bc71' 'inputs.pool_sha256'
Require-Equal $manifest.inputs.critic_composite_model_parameter_sha256 '6329233bcc22f7941e8085ef0235107eb75293fe74c727434c0474da15354f22' 'inputs.critic_composite_model_parameter_sha256'
Require-Condition (Test-Path -LiteralPath $manifest.inputs.source_store_root -PathType Container) 'inputs.source_store_root missing'
Require-Condition (Test-Path -LiteralPath $manifest.inputs.pool_root -PathType Container) 'inputs.pool_root missing'
Require-Condition (Test-Path -LiteralPath $manifest.inputs.critic_root -PathType Container) 'inputs.critic_root missing'

Require-Equal $manifest.topology.base_seed 980000 'topology.base_seed'
Require-Equal $manifest.topology.first_episode_index 33280 'topology.first_episode_index'
Require-Equal $manifest.topology.episode_count_per_candidate 64 'topology.episode_count_per_candidate'
Require-Equal ($manifest.topology.candidates -join ',') '1x32,2x32,4x16' 'topology.candidates'
Require-Equal $manifest.training.base_seed 980001 'training.base_seed'
Require-Equal $manifest.training.first_episode_index 33280 'training.first_episode_index'
Require-Equal $manifest.training.last_episode_index 33791 'training.last_episode_index'
Require-Equal $manifest.training.update_count 8 'training.update_count'
Require-Equal $manifest.training.episodes_per_update 64 'training.episodes_per_update'
Require-Equal $manifest.evaluation.base_seed 980001 'evaluation.base_seed'
Require-Equal $manifest.evaluation.original_pool_first_episode_index 65536 'evaluation.original_pool_first_episode_index'
Require-Equal $manifest.evaluation.original_pool_last_episode_index 66559 'evaluation.original_pool_last_episode_index'
Require-Equal $manifest.evaluation.pure_gae8_first_episode_index 66560 'evaluation.pure_gae8_first_episode_index'
Require-Equal $manifest.evaluation.pure_gae8_last_episode_index 67583 'evaluation.pure_gae8_last_episode_index'
Require-Equal $manifest.evaluation.episode_count_per_arm 1024 'evaluation.episode_count_per_arm'
Require-Equal $manifest.evaluation.arm_count_per_panel 3 'evaluation.arm_count_per_panel'

$expectedOutputRoot = 'D:\mtg-kernel-composed-factorial-v1\population-response-cycle-v1'
Require-Equal $manifest.output.root $expectedOutputRoot 'output.root'
Require-Condition (-not (Test-Path -LiteralPath (Join-Path $expectedOutputRoot 'throughput-screen.json'))) 'throughput output already exists'
Require-Condition (-not (Test-Path -LiteralPath (Join-Path $expectedOutputRoot 'development-v1'))) 'development output already exists'

$manifestHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $manifestResolved).Hash.ToLowerInvariant()
[pscustomobject]@{
    schema = 'mtg-kernel-current-net8-population-response-preflight-result/v1'
    status = 'pass'
    git_commit = $gitHead
    design_sha256 = $designHash
    executable_sha256 = $executableHash
    initializer_payload_sha256 = $initializerHash
    manifest_sha256 = $manifestHash
    gpu_uuid = $gpuFields[1]
} | ConvertTo-Json -Compress
