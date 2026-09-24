param(
    [string]$Manifest,
    [ValidateSet('jack','haleyspc')][string]$HostName,
    [string]$WorkerRoot,
    [string]$ControlRoot,
    [string]$Python,
    [string]$RunConfig
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
        Assert-Hash $PSCommandPath $config.transport_sha256
        Assert-Hash $config.manifest $config.manifest_sha256
        Assert-Hash $config.launcher $config.launcher_sha256
        if (Test-Path -LiteralPath $config.worker_root) { throw 'Worker root already exists; never resume or overwrite it' }
        $busy = @(Get-CimInstance Win32_Process | Where-Object { $_.Name -match '^public_feature_evaluation_v1.exe$|^trainer.exe$|^mtg_kernel.*exe$|^cargo.exe$|^rustc.exe$' })
        if ($busy.Count -ne 0) { throw 'Competing native work present; preserve existing work' }
        $receipt.worker_started_utc = [DateTime]::UtcNow.ToString('o')
        Save-Json (Join-Path $config.control_root 'owner-start.json') $receipt
        # Windows PowerShell treats native stderr as an error record. Preserve it
        # without aborting observation of a still-running guarded child.
        $ErrorActionPreference = 'Continue'
        & $config.python -u $config.launcher --manifest $config.manifest --host $config.host_name --root $config.worker_root > (Join-Path $config.control_root 'worker.log') 2>&1
        $ErrorActionPreference = 'Stop'
        $receipt.exit_code = $LASTEXITCODE
        $receipt.complete = ($LASTEXITCODE -eq 0)
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

foreach ($value in @($Manifest,$HostName,$WorkerRoot,$ControlRoot,$Python)) {
    if (-not $value) { throw 'Manifest, HostName, WorkerRoot, ControlRoot and Python are required' }
}
foreach ($value in @($Manifest,$WorkerRoot,$ControlRoot,$Python,$PSCommandPath)) {
    if (-not [IO.Path]::IsPathRooted($value) -or $value.Contains('"')) { throw 'Absolute paths without literal quotes required' }
}
if ((Test-Path -LiteralPath $WorkerRoot) -or (Test-Path -LiteralPath $ControlRoot)) {
    throw 'Fresh worker and control roots required; no automatic resume'
}
$plan = Get-Content -LiteralPath $Manifest -Raw | ConvertFrom-Json
$launcher = $plan.documents.launcher.path
Assert-Hash $launcher $plan.documents.launcher.sha256
if ([IO.Path]::GetFileName($launcher) -ne 'g115_d3_launch_v1.py') { throw 'Only the supported D3 launcher is allowed' }
if (-not (Test-Path -LiteralPath $Python -PathType Leaf)) { throw 'Qualified Python executable missing' }
New-Item -ItemType Directory -Path $ControlRoot | Out-Null
$config = [ordered]@{
    manifest=$Manifest; manifest_sha256=(Get-FileHash -LiteralPath $Manifest).Hash.ToLowerInvariant()
    launcher=$launcher; launcher_sha256=$plan.documents.launcher.sha256
    python=$Python; host_name=$HostName; worker_root=$WorkerRoot; control_root=$ControlRoot
    transport_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()
}
$configPath = Join-Path $ControlRoot 'config.json'
Save-Json $configPath $config
# Full native admission is still performed by the unchanged launcher in the owner.
$ErrorActionPreference = 'Continue'
& $Python $launcher --manifest $Manifest --host $HostName --check-only > (Join-Path $ControlRoot 'check-only.log') 2>&1
$ErrorActionPreference = 'Stop'
if ($LASTEXITCODE -ne 0) { throw 'Supported launcher preflight refused; inspect retained check-only.log' }
$powershell = Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'
$command = '"' + $powershell + '" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + $PSCommandPath + '" -RunConfig "' + $configPath + '"'
$startup = New-CimInstance -ClassName Win32_ProcessStartup -ClientOnly -Property @{ShowWindow=[uint16]0}
$created = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{
    CommandLine=$command; CurrentDirectory=$ControlRoot; ProcessStartupInformation=$startup
}
$dispatch = [ordered]@{ utc=[DateTime]::UtcNow.ToString('o'); return_value=$created.ReturnValue; owner_pid=$created.ProcessId; command=$command; hidden=$true; native_dispatched_directly=$false }
if ($created.ReturnValue -eq 0) {
    $owner = Get-CimInstance Win32_Process -Filter "ProcessId = $($created.ProcessId)"
    if ($owner) { $dispatch.owner_created_utc=$owner.CreationDate.ToUniversalTime().ToString('o') }
}
Save-Json (Join-Path $ControlRoot 'dispatch.json') $dispatch
$dispatch | ConvertTo-Json
if ($created.ReturnValue -ne 0) { throw 'WMI dispatch refused; no retry into these roots' }
# A receipt is not proof of continued life. Recheck actual PID/creation time and worker progress.
