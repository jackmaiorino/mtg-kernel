param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$OutDir,
    [int]$Updates = 16,
    [int]$Device = 0,
    [string]$Seed = "5500001",
    [switch]$Prefix
)
# Small ticket-free timing check (<= 64 updates) of the PR #107 round-2 pilot
# workload: one run, topology 2x32, broker target 16. Records wall, process
# CPU time and the harness output.
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force $OutDir | Out-Null
$env:MULTIRUN_RUNS = "1"
$env:MULTIRUN_UPDATES = "$Updates"
$env:MULTIRUN_WORKERS = "2"
$env:MULTIRUN_SESSIONS = "32"
$env:MULTIRUN_BROKER_TARGET = "16"
$env:MULTIRUN_BASE_SEED = $Seed
$env:MULTIRUN_SEED_OFFSET = "0"
$env:MULTIRUN_STORE_PARENT = (Join-Path $OutDir "parent")
$env:MTG_KERNEL_PILOT_CUDA_ORDINAL = "$Device"
Remove-Item Env:CUDA_VISIBLE_DEVICES -ErrorAction SilentlyContinue
if ($Prefix) { $env:MTG_KERNEL_TENSORIZE_PROFILE_PREFIX = "1" } else { Remove-Item Env:MTG_KERNEL_TENSORIZE_PROFILE_PREFIX -ErrorAction SilentlyContinue }
$log = Join-Path $OutDir "process.log"
$start = Get-Date
$p = Start-Process -FilePath $Exe -ArgumentList @("--exact", "native_science_loop_v1::windows_science_loop_tests::multirun_pilot_v1", "--ignored", "--nocapture", "--test-threads=1") -WorkingDirectory $OutDir -RedirectStandardOutput $log -RedirectStandardError (Join-Path $OutDir "process.err") -PassThru -NoNewWindow
$p.WaitForExit()
$wall = ((Get-Date) - $start).TotalSeconds
$cpu = $p.TotalProcessorTime.TotalSeconds
$summary = [ordered]@{ exe = $Exe; exe_sha256 = (Get-FileHash $Exe -Algorithm SHA256).Hash.ToLower(); updates = $Updates; device = $Device; seed = $Seed; prefix_stat = [bool]$Prefix; exit_code = $p.ExitCode; wall_s = $wall; process_cpu_s = $cpu; user_cpu_s = $p.UserProcessorTime.TotalSeconds }
$summary | ConvertTo-Json | Out-File -Encoding utf8 (Join-Path $OutDir "summary.json")
Get-Content $log | Select-String "MULTIRUN|test result"
$summary | ConvertTo-Json
