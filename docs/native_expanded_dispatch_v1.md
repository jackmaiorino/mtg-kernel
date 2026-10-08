# Supported native-expanded CPU dispatch

Implements the launcher prerequisite for issue148. No Rust or scientific
configuration changes. The actual native binary remains pinned independently
from the Python launcher's Git revision. Both training and evaluation enter
through `python/tools/native_expanded_dispatch_v1.py`, which acquires the target
host's canonical `host_reservation_v1` reservation before spawning work.

The operator supplies one create-only request per execution. Example shape
(paths, hashes, byte projections, affinity, inventory and limits must come from
the actual admitted allocation):

```json
{
  "schema": "native-expanded-cpu-dispatch/v1",
  "kind": "training",
  "lane": "codex-terminal-credit-20261005",
  "runtime": {"path": "ABS/runtime.json", "sha256": "SHA256"},
  "config": {"path": "ABS/ordinary-2026100601.json", "sha256": "SHA256"},
  "root": "ABS/qualification-desktop-ordinary-w1",
  "cold_root": "OTHER/qualification-desktop-ordinary-w1",
  "placement": {
    "host": "desktop", "cpu_affinity": [0, 1, 2, 3],
    "workers": 1, "preparation_workers": 1, "memory_bytes": 8589934592
  },
  "storage": {
    "accounting_roots": ["ABS/campaign", "OTHER/campaign"],
    "max_logical_bytes": 206158430208,
    "reserve_bytes": 64424509440,
    "projected_additional_bytes": 1073741824,
    "projected_volume_bytes": {"D:": 536870912, "E:": 536870912}
  },
  "wall_seconds": 3600
}
```

The example numbers are syntax illustrations, not authority, projections or
resource claims. `runtime.json` contains `engine_commit`,
`tracked_tree_sha256`, and `binary` (absolute path/SHA256). Runtime compatibility
compares engine, tree and executable SHA, not machine-specific executable paths.
Stage identical canonical source pins on each host. Launch on that host using
its regular Python executable, with no raw native bypass:

```text
python -B python/tools/native_expanded_dispatch_v1.py qualify ABS_REQUEST.json
```

Run workers1, then increasing useful counts independently for each loss setting
on each eligible host. The bounded training payload retains the full frozen
configuration and uses the existing `--max-new-iterations 1` option. Only its
output path and collection/preparation worker counts change. It completes at
most32 games in one full synchronous update. Failed outputs are retained.

For `kind: evaluation`, supply a genuine `collect_parallel` configuration plus
`episode_indices`, sorted distinct indices of2..20 representative cases. Serial
qualification uses the existing `collect` mode; parallel uses `collect_parallel`.
All candidates must use the same exact sample. Actual production keeps every
episode and the actual checkpoint pin. Same import, feature/card contract and
parameter layout allow compatible evaluation qualification to cover the final
trained weights. Monitor observed game lengths and throughput; a material
workload change needs a revised placement, not automatic reuse without scrutiny.

Each successful `root/report.json` records the request, actual configuration,
immutable runtime, process exit, completed-work counts, timings, full parameter
and Adam comparison fingerprint, ordered trajectory hashes, and checked recovery
archive. Evaluation adds an explicit `collection` pin; training adds `run`.
Qualification compares byte-identical trajectories and all checkpoint contents,
excluding only output paths in trajectory pins. Treatment and ordinary settings
are separate comparisons. Native scheduler stage timings remain in stdout.

Recovery uses the existing two-shard level1 archive and full checksum readback.
Raw native artifacts remain. The measured seconds include initialization,
collection, update, verification and this local recovery. Target transport is a
separately measured cost in inventory. The operator must recover and seal the
required independent copy before using the receipt. Output directories and
archives count in the same campaign allowance. Account for other hosts and
retained outputs by reducing each launch's local logical allowance;192GiB is a
whole-campaign ceiling, never a separate allowance per host. Per-volume projected
bytes must leave60GiB free. Actual usage and owned-process CPU/memory are checked
while native work runs. Symlinks and junctions in accounting trees are refused.

Build a choice JSON:

```json
{
  "schema": "native-expanded-cpu-allocation/v1",
  "inventory": {
    "desktop": {"eligible": true, "reason": "CURRENT EVIDENCE", "checked_at": "ISO UTC",
      "evidence": {"path": "ABS/census.json", "sha256": "SHA256"},
      "cpu_affinity": [0, 1, 2, 3], "transport_seconds": 0},
    "computehost": {"eligible": false, "reason": "CURRENT EVIDENCE", "checked_at": "ISO UTC",
      "evidence": {"path": "ABS/census.json", "sha256": "SHA256"}},
    "runpod": {"eligible": false, "reason": "No paid compute authority", "checked_at": "ISO UTC",
      "evidence": {"path": "ABS/budget-census.json", "sha256": "SHA256"}}
  },
  "qualifications": [{"path": "ABS/w1/report.json", "sha256": "SHA256"},
                     {"path": "ABS/w2/report.json", "sha256": "SHA256"}],
  "selected": {"WORKLOAD_DIGEST": {"path": "ABS/w2/report.json", "sha256": "SHA256"}}
}
```

Get each key with `workload(config, kind)`. Training replicas differ only in
IDs/seeds; decks, seat order, opponents, complete schedule shape, initial state,
loss and optimizer must match. The fastest eligible candidate minimizes complete
projected work plus transport and any measured remaining host queue. Optional
`queue_seconds` requires a `queue_evidence` pin; this allows an otherwise slower
free PC to finish an independent job sooner while the faster PC is occupied.
The research lead owns the total campaign queue and shared storage accounting.

Two opt-in request fields serve chained training blocks; requests without them
behave exactly as before.

- `"schedule_family": "permuted-units-v1"` (training only, in qualification and
  production requests alike). The family key is then
  `workload(config, "training", family="permuted-units-v1")`: blocks may order
  one balanced schedule differently and start from different admitted weights
  of the same import, feature contract and parameter layout. The multiset of
  scheduled episodes (without IDs/seeds), episodes per update, opponents, loss,
  optimizer and tolerance stay bound. One serial/parallel qualification of the
  first block's first update then covers every block of the schedule.
- `"non_natural_tolerance": F` (training only) must restate a nonzero
  `max_non_natural_episode_fraction` of the config. Kept trajectories must still
  be natural; each update's non-natural ledger SHA256 joins the parity
  fingerprint, so serial and parallel trials must discard identical attempts.
  Evaluation still refuses any tolerance.

For recovered remote evidence, an entry may instead be
`{"report": PIN, "path_mappings": [{"source_root": "C:/captured/root", "local_root": "D:/recovered/root"}]}`.
Only file location changes; captured bytes and all SHA256s remain fixed. Include
mappings for request/config/runtime/native/archive roots as needed. A copied
binary can likewise resolve through a mapping. No receipt is rewritten.

Add `choice` (choice JSON pin) and `choice_verification_output` (new file path)
to a production request whose full configuration implements the selected worker
counts. Once per workload family and choice, verify the saved outputs:

```text
python -B python/tools/native_expanded_dispatch_v1.py check-choice ABS_REQUEST.json
```

Pin the resulting file as `choice_verification` in the request. Production now
reuses that compatible checked snapshot and verifies small receipts, current
inventory, exact runtime and full requested workload. It does not reparse every
large qualification trajectory for each formal run. Changing the choice requires
a fresh verification snapshot, not another timing run when evidence remains
compatible. Finally:

```text
python -B python/tools/native_expanded_dispatch_v1.py dispatch ABS_REQUEST.json
```

The public and reservation-owned private entry both enforce these guards.
Fresh output roots are mandatory; this launcher never silently retries formal
native execution. Completion is a process result and durable receipt, not a
claim of playing strength. Source-dependent endpoint validation and statistical
analysis remain in the accepted research lane. No paid launch path is provided;
a `runpod` placement only runs on an already leased pod under its guard (below).

## Host reservation on Linux

`host_reservation_v1.py` also runs on Linux (for example a rented CPU pod)
with the same CLI, lock and event files, exit codes and reclaim rules; Windows
behavior is unchanged. The root is `$MTG_HOST_LOCK_ROOT`, else
`/var/lib/mtg-node/host-lock`, on a local filesystem with hard links and
`flock`.

Guaranteed as on Windows: exactly one acquirer wins (the record is hard-linked
into place, which fails if a lock exists); state changes are serialized; a
process is its pid plus start time (`/proc/<pid>/stat` field 22), so a dead,
exited or reused pid counts as absent and anything unreadable stays unknown; a
changed `/proc/sys/kernel/random/boot_id` is reboot evidence. The supervisor is
spawned detached in a new session. It becomes a child subreaper and runs the
work in a session of its own. It releases only when that session, its process
group and every descendant of the supervisor have ended, and SIGTERM, SIGINT or
SIGHUP to the supervisor kills all of them. The busy refusal scans `/proc`.

Weaker than Windows: a job object dies with its supervisor, but here SIGKILL of
the supervisor (or the OOM killer) kills nothing. Processes still in the work
session keep the reservation held, but a descendant that called `setsid` itself
is no longer seen once the supervisor is gone. Absence of the recorded
processes is therefore weaker evidence for reclaim. Lock exclusion binds only
processes that use this module, since `flock` is advisory where Windows share
modes are enforced.

## Dispatch on Linux and the `runpod` placement

Windows behavior and records are unchanged. On Linux the dispatcher binds the
placement's CPUs with `sched_setaffinity`, starts the native child at nice 10
(set before exec, so every native thread inherits it) and samples `/proc`.
Telemetry keeps the Windows fields: `rss_bytes` is VmRSS (the resident set,
the working-set counterpart; 0 once the child has exited), `cpu_seconds` is
utime+stime of all threads. The busy pattern adds `^trainer$` and the pinned
binary's file name (Linux image names have no `.exe`). Linux paths have no
drive, so their `projected_volume_bytes` key is `""`.

A Linux `runtime.json` adds `"platform": "linux-x86_64"` and `"libraries"`,
`{path, sha256}` for `ld-linux-x86-64.so.2`, `libc.so.6`, `libm.so.6` and
`libgcc_s.so.1` (`f32::tanh` goes through libm). Before launch every pin is
verified and a non-empty `LD_*` or `GLIBC_TUNABLES` is refused; once the loader
has mapped the four libraries, `/proc/<child>/maps` is read once and each must
resolve (realpath) to its pinned file, else the child is killed. The runtime
identity adds the platform and library SHA256s, so Linux and Windows
qualifications never match; a Windows identity is exactly as before, and a
runtime runs only on its own platform.

`runpod` is eligible only on Linux with `RUNPOD_POD_ID` set and
`MTG_LEASE_GUARD_DIR` naming the pod's guard state directory
(`phase1_cloud/lease_guard.py --state`), where the operator also places the
guard's exact lease as `lease.json` (it must pass `lease_guard.validate`).
`guard.json` must name this pod and lease, be at most three poll intervals
plus 30 s old (two provider timeouts), have no `latched`, `release_epoch` or
`stop-request.json`, report `provider_ok` and `allow_new_dispatch`, and leave
the work (`wall_seconds`, or a smaller `lease_work_seconds`) before
`deadline_epoch - recovery_seconds`, when the guard latches `deadline`. The
public entry and the reserved launch both check. desktop and computehost are
refused on Linux, `runpod` on Windows. The dispatcher never rents compute and
does not write `progress.json`; its caller keeps the guard fed.

`nine_deck_campaign_v1.py` on `runpod` checks before every block attempt that
the guard leaves 1.25x the longest block measured on runpod (else
`lease_block_seconds`, else `wall_seconds`), plus 600 s for collection and
retention, plus `recovery_seconds`, and passes the estimate as
`lease_work_seconds`. Otherwise the run stops at that boundary with
`stop_reason` `lease_time_exhausted` (or `lease_guard_refused: ...`) and the
next lease resumes there; a running block is never stopped for this. The driver
atomically rewrites `<guard>/progress.json` every min(15 s,
`work_idle_seconds`/3) and after each block: `last_productive_epoch` is the
last block completion (else driver start), `last_activity_epoch` advances while
native CPU time grows or the driver itself dispatches, archives or finishes a
block, `queued_work` equals `native_alive`, and the final record is
`finished: true`, so the guard releases the pod after `recovery_seconds` (or
after 90 s as `controller_lost` if the driver dies). Keep campaign state,
checkpoints, cold and retained roots on the network volume.
