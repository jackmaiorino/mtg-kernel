# D3 cloud controller transport

Status: prepared, not dispatched; transport delta requires Kimi verification.

The CLI turn ends its tool processes. The previous Windows WMI dispatcher only
supervised Windows shards, leaving the workstation cloud controller and its
external lease guard exposed to that lifetime. The same dispatcher now accepts
`-HostName runpod -CloudPackage <path> -CloudControl <prepared lease root>`.
`-ControlRoot` is a separate fresh WMI receipt directory. Cloud mode rejects
`-WorkerRoot`; Windows mode rejects cloud arguments.

The new mode runs the existing `g115_d3_cloud_v1.py` planning validation before
creating the hidden WMI owner. The owner rechecks manifest, dispatcher, launcher,
cloud controller, package and lease hashes, refuses already attempted cloud
control roots, and invokes that controller with `--execute`. The controller
retains every existing funding, lease, qualification, actual hardware, resident
guard, recovery and release check. Native work still goes through the pinned
`g115_d3_launch_v1.py` on the allocated host. The cloud controller and its external
guard descend from the WMI owner. No native executable is dispatched directly.

Windows shard execution and the WMI creation sequence are unchanged. The
dispatcher additionally checks its own hash against the supplied manifest.
New manifests must pin `transport.cloud_controller`, `transport.cloud_package`
and `transport.cloud_control` for cloud mode. The science, native executable,
models, panel, seeds, time bounds, statistics and retained-byte collector are
unchanged. The cloud preparation retains the KIMI034 snapshot and relocates
transport/index references without changing their bytes.

Evidence: `E:/mtg-g115-lineage-20260923/d3-attempt004-cloud-transport-001/`.
The original dispatcher is preserved as `verified-windows-dispatch.ps1`, SHA256
`8d69eaba4d4bd2f913d4d46c739ce68afa0177a2acb3a2e3dc8a85dabde5dd68`.
The new dispatcher SHA256 is
`156b054920b3dde34fa3546eecdc619040f505d8d9a1520a12894625b0e3a22d`.
PowerShell parsing passed. The actual cloud-mode preflight returned1 with
`Design review has unresolved launch conditions` for the deliberately incomplete
preparation. No WMI dispatch/owner receipt, allocated pod or formal manifest was
created. `receipt.json` records those checks and unchanged logical manifest
fields. This is a negative preflight check, not live cloud admission or a new
forced-backend-teardown test.

Kimi should verify the transport diff and preservation of W1-W4 under the
existing audit. W3 demonstrated backend-teardown survival of the same WMI
creation mechanism for the prior Windows worker. Its evidence does not by
itself assert a completed cloud-controller teardown test, reboot survival,
resident-guard success before boot, or provider TTL. The Pod-resident guard is
independent of the workstation after boot; failed boot, host failure and an
unreachable provider remain documented guard limitations. Any live allocation
still needs current compatible hardware, fresh inventory/lease, final support,
verified placement and actual guarded launch receipts.
