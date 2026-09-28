param(
    [string]$Manifest,
    [ValidateSet('jack','haleyspc','runpod')][string]$HostName,
    [string]$WorkerRoot,
    [string]$ControlRoot,
    [string]$Python,
    [string]$RunConfig,
    [string]$CloudPackage,
    [string]$CloudControl
)
$ErrorActionPreference = 'Stop'

function Save-Json($Path, $Value) {
    $Value | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $Path -Encoding UTF8
}

function Assert-Hash($Path, $Expected) {
    if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Expected) {
        throw "Pinned file changed: $Path"
    }
}

if ($RunConfig) {
    # WMI owns this hidden process. Losing the dispatch client does not stop it.
    $config = Get-Content -LiteralPath $RunConfig -Raw | ConvertFrom-Json
    $receipt = [ordered]@{ complete=$false; started_utc=[DateTime]::UtcNow.ToString('o'); owner_pid=$PID; native_dispatched_directly=$false }
    try {
        Assert-Hash (Join-Path $PSScriptRoot 'g115_reserved_dispatch_v1.py') $config.reservation_adapter_sha256
        Assert-Hash (Join-Path $PSScriptRoot 'host_reservation_v1.py') $config.reservation_helper_sha256
        & $config.python (Join-Path $PSScriptRoot 'g115_reserved_dispatch_v1.py') --check-owner | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'Current host reservation required before native work' }
        Assert-Hash $PSCommandPath $config.transport_sha256
        Assert-Hash $config.manifest $config.manifest_sha256
        Assert-Hash $config.launcher $config.launcher_sha256
        if ($config.host_name -eq 'runpod') {
            Assert-Hash $config.cloud_controller $config.cloud_controller_sha256
            Assert-Hash $config.cloud_package $config.cloud_package_sha256
            Assert-Hash (Join-Path $config.cloud_control 'lease/lease.json') $config.cloud_lease_sha256
            foreach ($name in @('formal-manifest.json','completion.json','lease/created-pod.json','lease/create-pod-failure.json')) {
                if (Test-Path -LiteralPath (Join-Path $config.cloud_control $name)) { throw 'Cloud control already attempted; use fresh preparation' }
            }
        } elseif (Test-Path -LiteralPath $config.worker_root) { throw 'Worker root already exists; never resume or overwrite it' }
        $busy = @(Get-CimInstance Win32_Process | Where-Object { $_.Name -match '^public_feature_evaluation_v1.exe$|^trainer.exe$|^mtg_kernel.*exe$|^cargo.exe$|^rustc.exe$' })
        if ([IO.Path]::GetFileName($config.launcher) -in @('g115_d4_build_launch_v1.py','g115_r14_test_launch_v1.py','g115_d4_replay_timing_launch_v1.py','g115_d4_audit_pipeline_v1.py')) {
            $busy = @(Get-CimInstance Win32_Process | Where-Object { $_.Name -match '^(public_.*|native_.*|learned_sideboard_v1|trainer|mtg_kernel.*|cargo|rustc)\.exe$' })
        }
        if ($busy.Count -ne 0) { throw 'Competing native work present; preserve existing work' }
        $receipt.worker_started_utc = [DateTime]::UtcNow.ToString('o')
        Save-Json (Join-Path $config.control_root 'owner-start.json') $receipt
        # Windows PowerShell treats native stderr as an error record. Preserve it
        # without aborting observation of a still-running guarded child.
        $ErrorActionPreference = 'Continue'
        if ($config.host_name -eq 'runpod') {
            # The controller and its external lease guard inherit the WMI owner's
            # lifetime. Only the existing cloud controller may allocate or dispatch.
            & $config.python -u $config.cloud_controller --manifest $config.manifest --package $config.cloud_package --control $config.cloud_control --execute > (Join-Path $config.control_root 'worker.log') 2>&1
        } elseif ($config.observer) {
            Assert-Hash $config.observer $config.observer_sha256
            & $config.python -B -u $config.observer --manifest $config.manifest --host $config.host_name --root $config.worker_root --report-root (Join-Path $config.control_root 'telemetry') > (Join-Path $config.control_root 'worker.log') 2>&1
        } else {
            & $config.python -u $config.launcher --manifest $config.manifest --host $config.host_name --root $config.worker_root > (Join-Path $config.control_root 'worker.log') 2>&1
        }
        $ErrorActionPreference = 'Stop'
        $workerExit = $LASTEXITCODE
        if ($config.compiler_auxiliary) {
            $cleanup = & $config.python (Join-Path $PSScriptRoot 'g115_reserved_dispatch_v1.py') --config $RunConfig --cleanup-auxiliary
            if ($LASTEXITCODE -ne 0) { throw 'Compiler auxiliary cleanup refused' }
            $receipt.compiler_auxiliary_cleanup = @($cleanup | ConvertFrom-Json)
        }
        $receipt.exit_code = $workerExit
        $receipt.complete = ($workerExit -eq 0)
        # The launcher's original completion.json remains the authoritative shard result.
    } catch {
        $receipt.error = $_.Exception.Message
    } finally {
        $receipt.finished_utc = [DateTime]::UtcNow.ToString('o')
        Save-Json (Join-Path $config.control_root 'owner-completion.json') $receipt
    }
    if (-not $receipt.complete) { exit 1 }
    exit 0
}

foreach ($value in @($Manifest,$HostName,$ControlRoot,$Python)) {
    if (-not $value) { throw 'Manifest, HostName, ControlRoot and Python are required' }
}
$cloudMode = $HostName -eq 'runpod'
if ($cloudMode) {
    if (-not $CloudPackage -or -not $CloudControl -or $WorkerRoot) { throw 'Cloud mode requires CloudPackage and CloudControl, without WorkerRoot' }
    $modePaths = @($CloudPackage,$CloudControl)
} else {
    if (-not $WorkerRoot -or $CloudPackage -or $CloudControl) { throw 'Windows mode requires WorkerRoot without cloud arguments' }
    $modePaths = @($WorkerRoot)
}
foreach ($value in (@($Manifest,$ControlRoot,$Python,$PSCommandPath) + $modePaths)) {
    if (-not [IO.Path]::IsPathRooted($value) -or $value.Contains('"')) { throw 'Absolute paths without literal quotes required' }
}
if ((-not $cloudMode -and (Test-Path -LiteralPath $WorkerRoot)) -or (Test-Path -LiteralPath $ControlRoot)) {
    throw 'Fresh worker and control roots required; no automatic resume'
}
$plan = Get-Content -LiteralPath $Manifest -Raw | ConvertFrom-Json
$launcher = $plan.documents.launcher.path
Assert-Hash $launcher $plan.documents.launcher.sha256
Assert-Hash $PSCommandPath $plan.transport.dispatcher.sha256
$launcherName = [IO.Path]::GetFileName($launcher)
if ($launcherName -eq 'g115_d4_timing_launch_v1.py') {
    if ($cloudMode -or $plan.schema -ne 'g115-d4-bounded-timing/v1') { throw 'D4 admits bounded Windows timing only' }
} elseif ($launcherName -eq 'g115_d4_eval_timing_launch_v1.py') {
    if ($cloudMode -or $plan.schema -ne 'g115-d4-bounded-eval-timing/v1') { throw 'D4 evaluation admits bounded Windows timing only' }
} elseif ($launcherName -eq 'g115_r14_test_launch_v1.py') {
    if ($HostName -ne 'jack' -or $plan.schema -ne 'g115-r14-windows-tests/v1') { throw 'R14 tests require the named Jack test owner' }
} elseif ($launcherName -eq 'g115_d4_build_launch_v1.py') {
    if ($HostName -ne 'jack' -or $plan.schema -ne 'g115-d4-windows-build/v1') { throw 'D4 build mode admits the named Jack build launcher only' }
} elseif ($launcherName -eq 'g115_d4_replay_timing_launch_v1.py') {
    if ($cloudMode -or $plan.schema -ne 'g115-d4-replay-timing/v1') { throw 'D4 archived replay admits bounded Windows timing only' }
} elseif ($launcherName -eq 'g115_d4_audit_pipeline_v1.py') {
    if ($cloudMode -or $plan.schema -ne 'g115-d4-audit-pipeline-qualification/v1' -or $plan.mode -notin @('qualification','production')) { throw 'D4 combined audit path requires guarded Windows qualification or production' }
    if ([IO.Path]::GetFileName($plan.observer.path) -ne 'g115_d4_audit_observer_v1.py') { throw 'Audit qualification and production require the named utilization observer' }
    Assert-Hash $plan.observer.path $plan.observer.sha256
} elseif ($launcherName -eq 'g115_d4_archive_transfer_v1.py') {
    if ($HostName -ne 'jack' -or $plan.schema -ne 'g115-d4-archive-transfer/v1') { throw 'Archive transfer requires the named Jack WMI sender' }
} elseif ($launcherName -eq 'g115_d4_audit_verify_v1.py') {
    if ($cloudMode -or $plan.schema -ne 'g115-d4-audit-verification/v1') { throw 'Audit verification requires the bounded Windows verification owner' }
} elseif ($launcherName -eq 'g115_d4_header_audit_v1.py') {
    if ($HostName -ne 'jack' -or $plan.schema -ne 'g115-d4-header-audit/v1') { throw 'Header extraction requires the bounded read-only Jack owner' }
} elseif ($launcherName -eq 'g115_line_a_windows_launch_v1.py') {
    if ($cloudMode -or $plan.schema -ne 'g115-line-a-launch/v1') { throw 'Line (a) admits only the named guarded Windows launcher' }
} elseif ($launcherName -ne 'g115_d3_launch_v1.py') { throw 'Only named supported D3/D4/line (a) launchers are allowed' }
if (-not (Test-Path -LiteralPath $Python -PathType Leaf)) { throw 'Qualified Python executable missing' }
if ($cloudMode) {
    $cloudController = $plan.transport.cloud_controller.path
    if ([IO.Path]::GetFileName($cloudController) -ne 'g115_d3_cloud_v1.py') { throw 'Only the supported D3 cloud controller is allowed' }
    Assert-Hash $cloudController $plan.transport.cloud_controller.sha256
    Assert-Hash $CloudPackage $plan.transport.cloud_package.sha256
    if ($CloudControl -ne $plan.transport.cloud_control) { throw 'Cloud control differs from pinned manifest' }
    if (-not (Test-Path -LiteralPath (Join-Path $CloudControl 'lease/lease.json') -PathType Leaf)) { throw 'Fresh prepared cloud lease required' }
    foreach ($name in @('formal-manifest.json','completion.json','lease/created-pod.json','lease/create-pod-failure.json')) {
        if (Test-Path -LiteralPath (Join-Path $CloudControl $name)) { throw 'Cloud control already attempted; use fresh preparation' }
    }
}
New-Item -ItemType Directory -Path $ControlRoot | Out-Null
$config = [ordered]@{
    manifest=$Manifest; manifest_sha256=(Get-FileHash -LiteralPath $Manifest).Hash.ToLowerInvariant()
    launcher=$launcher; launcher_sha256=$plan.documents.launcher.sha256
    python=$Python; host_name=$HostName; worker_root=$WorkerRoot; control_root=$ControlRoot
    reservation_adapter_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'g115_reserved_dispatch_v1.py')).Hash.ToLowerInvariant()
    reservation_helper_sha256=(Get-FileHash -LiteralPath (Join-Path $PSScriptRoot 'host_reservation_v1.py')).Hash.ToLowerInvariant()
    transport_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()
}
if ($launcherName -eq 'g115_d4_audit_pipeline_v1.py' -and $plan.observer) {
    $config.observer = $plan.observer.path
    $config.observer_sha256 = $plan.observer.sha256
}
if ($cloudMode) {
    $config.cloud_controller = $cloudController
    $config.cloud_controller_sha256 = $plan.transport.cloud_controller.sha256
    $config.cloud_package = $CloudPackage
    $config.cloud_package_sha256 = $plan.transport.cloud_package.sha256
    $config.cloud_control = $CloudControl
    $config.cloud_lease_sha256 = (Get-FileHash -LiteralPath (Join-Path $CloudControl 'lease/lease.json')).Hash.ToLowerInvariant()
}
if ($launcherName -in @('g115_d4_build_launch_v1.py','g115_r14_test_launch_v1.py')) {
    $auxiliary = Join-Path (Split-Path $plan.tools.linker.path) 'VCTIP.EXE'
    if (Test-Path -LiteralPath $auxiliary) {
        $config.compiler_auxiliary = @{ path=$auxiliary; sha256=(Get-FileHash -LiteralPath $auxiliary).Hash.ToLowerInvariant() }
    }
}
$configPath = Join-Path $ControlRoot 'config.json'
Save-Json $configPath $config
# Full native admission is still performed by the unchanged launcher in the owner.
$ErrorActionPreference = 'Continue'
if ($cloudMode) {
    # Planning only: actual hardware, funding and resident-guard admission happen
    # in the unchanged controller before the unchanged remote launcher runs.
    & $Python $cloudController --manifest $Manifest > (Join-Path $ControlRoot 'check-only.log') 2>&1
} else {
    & $Python $launcher --manifest $Manifest --host $HostName --check-only > (Join-Path $ControlRoot 'check-only.log') 2>&1
}
$ErrorActionPreference = 'Stop'
if ($LASTEXITCODE -ne 0) { throw 'Supported launcher preflight refused; inspect retained check-only.log' }
# The helper acquires before WMI creation and contains the owner and descendants.
& $Python (Join-Path $PSScriptRoot 'g115_reserved_dispatch_v1.py') --config $configPath
if ($LASTEXITCODE -ne 0) { throw 'Reserved dispatch refused or unconfirmed; inspect dispatch.json, do not retry these roots' }
