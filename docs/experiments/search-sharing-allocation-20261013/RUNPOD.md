# One bounded CPU pod

Jack approved up to **$5 total**, **one CPU3c pod**, **32 vCPUs / 64 GB RAM**,
**100 GB container disk**, **US-MD-1**, and **four hours maximum**.
The parent lead owns creation and launches. No pod has been created by this
worker. Qualification and the existing 12 aggregate search/evaluation CPU-hour
ceiling still apply; this changes no frozen analysis.

The live read-only CPU catalog at 2026-10-13 23:24 UTC listed CPU3c at
$0.03/vCPU/hour and HIGH 32-vCPU stock in US-MD-1. Receipt:
D:/e-scratch/search-sharing-allocation-20261013/runpod_cpu_options_v2.json.
The documented [CPU catalog](https://docs.runpod.io/api-reference-v2/catalog/list-cpu-types)
uses https://api.runpod.io/v2/catalog/cpus with
include=AVAILABILITY&product=POD&vcpuCount=32.
[Disk pricing](https://docs.runpod.io/pods/pricing) is $0.10/GB/month for
running container disks. Conservatively using 720 hours/month, this is
$0.96 + $0.013889 = **$0.973889/hour**. Four hours plus $0.05 recovery reserve
and a 20 percent margin projects **$4.734667**. The guard permits an observed
compute rate of at most $1/hour, yielding a worst-case guarded projection of
**$4.926667** including disk and the same reserve/margin. No persistent/network volume
is created, so deleting this pod leaves no new storage charge.

The pinned existing Python image is
docker.io/library/python@sha256:9c47360a2a0355e2da18516d0b1c2126ec22c195d2185e97347c9d98398c5bef.
Docker Registry HTTPS verification at 23:31 UTC confirmed this digest,
Linux/amd64, and 45.4 MB compressed layers. Receipt:
D:/e-scratch/search-sharing-allocation-20261013/runpod_image_readonly.json.
The bootstrap explicitly installs Jack's existing
C:/Users/Jack/.ssh/id_ed25519.pub as this pod's authorized key; account
registered keys did not match that key. No account key update is needed.
The [CPU create schema](https://docs.runpod.io/api-reference/pods/POST/pods)
supports CPU3c, vcpuCount, containerDiskInGb, public IP, and SSH port exposure.

Use the new cloud_lease.py adapter with the existing phase1_cloud resident
and external guard implementations. The resident guard boots before package
setup, has the exact pod name/ID, $5 cap and four-hour deadline, and receives
the API key only in memory. Native/package subprocess environments remove
that key. The independent local guardian runs through Task Scheduler and
survives closing Codex. It covers failed boot and initiates bounded output
copying before exact-ID deletion. Both guards recheck ownership. A provider
API outage can delay verified deletion; neither guard claims absence from
an ambiguous DELETE. This is a guarded lease, not a provider-enforced TTL.

Run these from the parent checkout after integrating this helper. Preparation
must occur immediately before allocation, so the four-hour clock is fresh.

~~~powershell
$cloudScript = 'C:\Users\Jack\.codex\worktrees\search-sharing-allocation-20261013\mtg-kernel\tools\search_sharing_allocation\cloud_lease.py'
$cloudControl = 'D:\e-scratch\search-sharing-allocation-20261013\lease-001'
$cloudTaskName = 'Codex-SearchAllocation-20261013-lease-001'
python $cloudScript prepare --control $cloudControl --name search-sharing-allocation-20261013-lease-001
$cloudAction = New-ScheduledTaskAction -Execute 'C:\Python314\pythonw.exe' -Argument ('"'+$cloudScript+'" guardian --control "'+$cloudControl+'"')
$cloudPrincipal = New-ScheduledTaskPrincipal -UserId ([Security.Principal.WindowsIdentity]::GetCurrent().Name) -LogonType Interactive -RunLevel Limited
# No expiry: no execution limit, restart on failure, re-triggered every 5 minutes indefinitely.
$cloudTrigger = New-ScheduledTaskTrigger -Once -At (Get-Date) -RepetitionInterval (New-TimeSpan -Minutes 5)
$cloudTrigger.Repetition.Duration = ''
$cloudSettings = New-ScheduledTaskSettingsSet -Hidden -WakeToRun -StartWhenAvailable -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) -MultipleInstances IgnoreNew -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
Register-ScheduledTask -TaskName $cloudTaskName -Action $cloudAction -Trigger $cloudTrigger -Principal $cloudPrincipal -Settings $cloudSettings
# Independent watchdog: one guardian pass every 5 minutes.
$watchAction = New-ScheduledTaskAction -Execute 'C:\Python314\pythonw.exe' -Argument ('"'+$cloudScript+'" guardian --once --control "'+$cloudControl+'"')
$watchSettings = New-ScheduledTaskSettingsSet -Hidden -StartWhenAvailable -ExecutionTimeLimit (New-TimeSpan -Minutes 4) -MultipleInstances IgnoreNew -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
Register-ScheduledTask -TaskName ($cloudTaskName+'-watchdog') -Action $watchAction -Trigger $cloudTrigger -Principal $cloudPrincipal -Settings $watchSettings
Start-ScheduledTask -TaskName $cloudTaskName
Get-Content -LiteralPath ($cloudControl+'\guardian-ready.json')
python $cloudScript create --control $cloudControl --execute
python $cloudScript status --control $cloudControl
~~~

Create refuses an unarmed or dead guardian, any earlier create of the same
lease (one POST per prepared lease), any existing Pod with the lease's name,
and a missing, failed or over-120-second-old account funding read. After the
POST it adopts one same-name Pod and deletes the rest. An ambiguous POST with
no Pod seen stays guarded (the guardian deletes any late Pod with the name)
until an operator checks the RunPod console and runs `resolve-ambiguous
--checked-by "..."`. Existing unrelated pods are untouched. Before native work, observe the resident guard's fresh
/run/phase1/search-sharing-allocation-20261013-lease-001/guard.json with
provider_ok=true, verify the returned CPU/disk/rate, and qualify serial/parallel
completed work using the guarded Linux runtime. Register connection.json in
the control directory with pod_id, verified host IP, port, private_key_path,
and remote_output=/workspace/search-sharing-allocation/run. This enables the
guardian's bounded scp preservation. The guardian marks worker-started.json
itself once the Pod's resident state shows worker-started.json or
progress.json over SSH, so the startup timeout never ends productive work; the
resident progress.json tracks the actual worker through output transfer, and
must not mark finished until preservation completes.

The Linux launcher must use the actual cgroup quota/cpuset, /var/lib/mtg-node/
host-lock, and measured free disk after build/output allocation. An ephemeral
/workspace directory is not a separate mount. Preserve 60 GiB free, at most
16 GiB build scratch and 4 GiB new output. Pin Rust 1.94.1 and the actual linker.

Rough planning estimates, not measured cloud speed: 2 to 5 minutes to boot,
10 to 30 minutes for toolchain/native release build, and 1 to 3 minutes to
transfer the current roughly 170 MB source/input payload at 10 to 100 Mbps.
The runtime port is prepared locally before rent. If effective completed-work
speed is 8 to 16 local-core equivalents, the 12 CPU-hour ceiling corresponds
to roughly 45 to 90 minutes of parallel work, plus setup and qualification.
Use the imminent local smoke timings and measured cloud 1/2/4/... throughput
to choose allocation; 32 advertised vCPUs do not establish a 32-fold speedup.

On completion, cold-copy the finished records and hashes to D: before asking
the guards to release. On a requested release or startup timeout, output
copying runs once, for at most five minutes and only while 15 minutes of the
lease remain; it never delays the DELETE. The deadline releases without
copying.

~~~powershell
python $cloudScript release --control $cloudControl
Get-Content -LiteralPath ($cloudControl+'\preservation.json')
Get-Content -LiteralPath ($cloudControl+'\external-guard-completion.json')
# Exit 0 only after two absent observations 30 s apart and the guardian's own
# double-confirmed completion. Otherwise leave both tasks armed.
python $cloudScript verify-absent --control $cloudControl
if ($LASTEXITCODE -eq 0) {
    foreach ($t in @($cloudTaskName, ($cloudTaskName+'-watchdog'))) { Disable-ScheduledTask -TaskName $t; Stop-ScheduledTask -TaskName $t; Unregister-ScheduledTask -TaskName $t -Confirm:$false }
}
~~~

Retain the preparation, create receipt, failed attempts, copied outputs,
ledger and observed deletion evidence. Stop early when the completed result
is preserved, qualification fails, the aggregate CPU ceiling binds, or the
remaining lease cannot safely cover the next fixed unit of work.
