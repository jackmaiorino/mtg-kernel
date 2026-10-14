# Optional replacement allocation

Jack subsequently approved one replacement within the remaining $4 and original
deadline. Two lease-003 create attempts failed; the diagnostic retry reported
HTTP 500 because the requested instances were unavailable. A final exact-name
census at 00:38:49 UTC found no pod. The unused guardian was stopped and
unregistered, with its process confirmed absent. No paid replacement allocation
or native work occurred. Windows fallback preparation continues. Cloud work can
resume only after verified actual availability changes within the unchanged
authority; there is no recurring watcher or active lease.

Use one CPU3c pod in US-MD-1: 32 vCPUs, 64 GB RAM, 100 GB ephemeral container
disk, no persistent/network volume, unique name
`search-sharing-allocation-20261013-lease-003`. The read-only authenticated
32-vCPU catalog at 2026-10-14 00:30 UTC showed HIGH availability and
$0.03/vCPU/hour, or $0.96/hour. Receipt:
`D:/e-scratch/search-sharing-allocation-20261013/runpod-replacement-catalog.json`.
[Container disk pricing](https://docs.runpod.io/pods/pricing) remains
$0.10/GB/month, conservatively $0.013889/hour for 100 GB.

Keep the original absolute deadline **1791949383.235842**, October 14
**03:43:03 UTC**, and original **$5 cumulative cap**. Pod 002's independent
absence receipt is timestamped 1791937756.2024827. Applying its original guard
rate, disk and margin through that timestamp yields $0.997160023 conservative
spend; round upward to **$1.00 prior spend**, leaving at most **$4.00**.
This is conservative accounting, not a provider invoice. The prior receipt,
failed takeover and recovered evidence stay intact.

Offline proposal artifacts are in
`D:/e-scratch/search-sharing-allocation-20261013/lease-003-proposal`.
At preparation (00:30:39 UTC), 3 hours 12 minutes remained. The worst-case
guarded projection was **$3.961250 new / $4.961250 cumulative**, including
$0.05 recovery reserve and 20 percent margin. The helper rejects replacement
preparation without an explicit deadline and rejects a projection above the
remaining cap. Fresh preparation after authorization must preserve both values:

~~~powershell
python tools/search_sharing_allocation/cloud_lease.py prepare --control D:/e-scratch/search-sharing-allocation-20261013/lease-003 --name search-sharing-allocation-20261013-lease-003 --deadline-epoch 1791949383.235842 --prior-conservative-usd 1.00
~~~

Reuse the preserved Linux binary from source `5fdb10f37`, SHA-256
`a40e1c43a4fcf406a13d102e1f175e000148f03e1fa08ecd7ccda2af30e69b4a`.
No native rebuild is needed. Transfer the pinned binary and inputs, install the
fixed Python launcher and required runtime packages, observe actual cgroup
capacity/disk, then qualify representative serial and parallel completion with
the supported allocator before formal dispatch. The existing aggregate
12 CPU-hour experiment ledger carries forward unchanged.

Planning estimates are 2 to 5 minutes for boot and 1 to 3 minutes for transfer;
qualification time and native throughput are still unmeasured. Compare this
startup plus qualification with the ongoing Windows release build plus its
qualification. Advertised 32 vCPUs do not establish a speedup over eight local
cores. Select cloud only if observed total turnaround justifies it within the
remaining time and cap; no formal work is authorized by this proposal.

Before any allocation, arm the independent local guardian and embedded resident
guard with this unchanged absolute deadline and prior spend. Preserve outputs
off-pod, then use the existing guarded release command and verify exact-ID
absence. Both guards release early on completion or failure; their ten-minute
recovery window cannot extend the original deadline. Do not reuse pod 002's
latched guard, clear its STOP state, or renew the lease clock.
