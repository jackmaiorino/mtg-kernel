# WSL CI host-core qualification

Jack assigned host-core isolation qualification and Linux CI activation only if
it passes, preserving active research. Current result: **not qualified**. No
guest workload, VM modification, WSL restart or Linux runner activation occurred.

On October 13, 2026, Jack's PC reports Windows 11 Home build 26200 and WSL
2.4.13.0. The latest Hyper-V-Hypervisor event 2 reports scheduler type `0x4`
(Root). Research retains logical CPUs 0-7; the recorder build retains 16-23.
Linux listeners remain offline behind the existing reservation gate.
At the 21:45 EDT availability check, another ordinary claim also occupied
CPUs 8-15. Successful administrative access would still require fresh spare
capacity before any guest workload could start.

Native `HcsGetServiceProperties` queries for `CpuGroup`, `ProcessorTopology`
and `QoSCapabilities`, and `HcsOpenComputeSystem` for the existing WSL VM,
returned `0x8037011b` (`HCS_E_ACCESS_DENIED`). The narrow process control handle
for `vmmemWSL` also returned Windows error 5. The current token is medium
integrity; Administrators membership is deny-only and Hyper-V Administrators
membership is absent. Existing runner tasks use limited tokens, and local SSH
batch authentication is unavailable. These observations block live control
qualification from this session.

The read-only `tools/windows/wsl_host_preflight.ps1` discovers the existing WSL
VM and process, queries HCS directly, checks narrow process handle access and
records the scheduler event. Run it from an elevated PowerShell to resolve the
access question. Exit 0 means a complete query/control access path is available;
it never means isolation passed. `qualified` stays false, and the helper has no
VM setters, guest workload or runner activation path.

Qualification still needs a reversible host-side affinity control on the exact
WSL VM, verified host Idle scheduling under the actual Root scheduler, and one
short bounded workload whose host scheduling trace shows execution stays on
claimed spare CPUs. Recheck reservations at admission, preserve original VM
settings and verify restore. Check research progress telemetry for interference.
VM recreation, reservation changes and ordinary-work preemption must close the
gate or suspend all contained CI before CPUs can be reused. The whole-VM scope
must be checked for other active WSL work; guest CPU IDs alone do not establish
Windows host placement. CPU pinning also leaves shared memory and storage load.

Microsoft documents [HCS access requirements](https://learn.microsoft.com/en-us/virtualization/api/hcs/reference/hcshresult),
[CPU groups](https://learn.microsoft.com/en-us/windows-server/virtualization/hyper-v/manage/manage-hyper-v-cpugroups),
and [scheduler types](https://learn.microsoft.com/en-us/windows-server/virtualization/hyper-v/manage/manage-hyper-v-scheduler-types).
Root is the supported Windows client scheduler; ordinary Hyper-V per-VM CPU
caps, weights and reserves are unsupported there. Do not change the host boot
scheduler or use a VM weight as evidence of lowest priority.
