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
projected work plus transport and any measured remaining host queue. A training
qualification runs the first update. Its projection pays once what a run pays
once: the launcher's setup, the native child's initialization and exit, the
storage checks and the report. The child times the update itself in the
report's pinned `native_stdout`, and the report's `artifact_timing` times the
fingerprint and archive. Every update then repeats the native update and its
archive. The trial fingerprints its one update in one process, while a run
spreads its updates over `fingerprint_workers(placement)` processes, so the
projection spreads the fingerprint the same way. A run's first update is
slower than its later ones, so projections stay somewhat high: for 162-update
runs on the desktop they were 1.14x (pack of 8), 1.15x (w2) and 1.18x (w8) the
measured wall time. Reports from before `native_stdout` project the whole trial
once per update, so they count the start-up once per update; the
qualifications of one choice must share one projection basis. Optional
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

## Packs: side-by-side runs of one family

A run is mostly serial, so one run on many held cores leaves most of them idle.
A study of several runs of one workload family (replicates that differ only in
IDs and seeds) can instead run them side by side, each on its own slot, inside
the study's held reservation. Nothing a run writes changes: each member is an
ordinary production dispatch with its own config, seeds, outputs and receipt.

A slot is a fixed CPU set plus its worker counts and memory limit:

```json
{"cpu_affinity": [6], "workers": 1, "preparation_workers": 1, "memory_bytes": 8589934592}
```

Slots of a layout are disjoint and share one worker count: a member's config
implements its slot's worker counts, so in a layout of mixed counts fewer
members than slots could run at once. A slot's kind is its core type
(efficiency class from `host_slots_v1.py topology`), physical cores and logical
CPUs, e.g. one thread of a P core, both threads of a P core, or one E core. A
slot may not span core types.

The pack tool runs inside the study's held reservation under a
`host_slots_v1.py timed` declaration. The declared cores must hold every slot
CPU and every SMT sibling of one, so no work outside the slots shares a slot's
physical cores, plus any post CPUs (below). The slots' memory limits together
must fit the host's free memory. On Linux, set `HOST_SLOTS_ROOT` to the
reservation's root (`$MTG_HOST_LOCK_ROOT`, else `/var/lib/mtg-node/host-lock`).
Both plans may name `compute_host_name`, which every nested dispatch gets as
`--compute-host-name`: WMI-created supervisors inherit no `COMPUTE_HOST_NAME`.

**Pack qualification.** Write a plan beside a training qualification request
of the family (one without `choice`):

```json
{"schema": "native-expanded-pack-qualify/v1",
 "base_request": {"path": "ABS/qualification-w1.request.json", "sha256": "SHA256"},
 "slots": [SLOT, SLOT, ...], "root": "ABS/pack-qualification", "timeout_seconds": 3600}
```

```text
python -B python/tools/host_slots_v1.py timed --cores CPUS -- python -B python/tools/native_expanded_pack_v1.py qualify ABS_PLAN.json
```

The tool runs the base request's one-update payload as nested `qualify`
dispatches, unchanged apart from placement and roots (the base request's roots
with the plan's SHA256 prefix and the trial's name appended): alone on the
first slot of each kind, one after another, then on every slot at once. The
packed trials write at once, so their storage blocks must fit together (one
storage check with every projection added) before the first of them starts.
Each packed trial carries a `start_barrier`. It waits after all of its checks
until every packed trial waits, and then they all start; the wait counts in
neither its seconds nor its wall-time limit, and is limited to the trial
timeout plus an hour of busy retries per packed trial, since each later trial
may retry busy refusals before it starts. That limit is only a backstop: the
barrier names the pack tool by pid and creation time, and a waiting trial fails
once the tool has ended without releasing it, so a tool that is killed or
crashes cannot leave its trials holding the host. A trial that ends before the
release, or fails before the barrier, fails the others. The tool writes `root/pack.json`
(`native-expanded-cpu-pack/v1`: workload, slots, kinds, the host topology, pins
of each trial's report and start record, and the host load before the solo
trials, at the packed trials' release and after them: the reservation's timed
declaration, as its token digest and cores, and every live host_slots claim).
It prints each trial's seconds, whether every output matched, the packed
trials' start spread and the mean packed over solo seconds of each kind, and
exits nonzero if any output differed.

**Choosing a pack.** Add to the choice JSON the pack indexes and the study's
size per family:

```json
"packs": [{"path": "ABS/pack-qualification/pack.json", "sha256": "SHA256"}],
"study_runs": {"WORKLOAD_DIGEST": 16}
```

A pack counts only if every trial passes the qualification checks above, each
trial ran on its slot, every trial's sample and full output fingerprint equals
those of the family's serial/parallel qualifications (so each packed output is
byte-equal to the serial run's), the packed trials started within a tenth of
the shortest one's native seconds of each other, no solo trial overlapped
another trial (by the start each trial's receipt binds: a qualification's
execution record carries its native child's start, which must equal its
start record's), there is one solo trial per kind and one packed trial per slot,
the slots share one worker count and lie inside the host's eligible CPU set,
and all three load records name one timed declaration that holds every slot
CPU and its SMT siblings with no other running claim on its cores. Kinds come
from the topology recorded in the pack, which must still be the host's own
when the pack is on the admitting host. A packed trial (one with a
`start_barrier`) never counts as a solo allocation's qualification. For a
study of N runs the launcher compares, per option, N x (projected seconds +
transport) + queue for each solo allocation with ceil(N / slots) x (the
slowest packed slot's projected seconds + transport) + queue for each pack,
and requires `selected` to name the fastest and the production placement to
be one of its slots. A pack pays transport once per wave because a wave's
members move their outputs at the same time; a solo allocation pays it once
per run. Every family a choice selects for must run in one layout (the same
hosts, CPUs and worker counts), so a study's arms stay comparable. The
verified snapshot then records the pack, the study's projected seconds and
size, the number of slots, each slot's packed projected seconds
(`slot_seconds`), the slowdown of each kind and how many other claims ran
elsewhere on the host while the pack qualified (`background_claims`). Choices
without `packs` behave as before.

**Running a pack.** List the members, each a complete production request with
`choice` and `choice_verification` whose verified choice selects a pack. All
members' packs must have the same slots, each member's placement is one of
them, members have their own roots and output directories, a family's members
cite one `study_runs`, and no family has more members than it:

```json
{"schema": "native-expanded-pack-run/v1", "root": "ABS/pack-run", "stop_files": ["ABS/STOP"],
 "members": [{"name": "r1-a", "request": {"path": "ABS/r1-a.request.json", "sha256": "SHA256"}}],
 "post": ["python", "-B", "ABS/trace.py", "{report}"], "post_cpus": [15], "post_timeout_seconds": 1800}
```

```text
python -B python/tools/host_slots_v1.py timed --cores CPUS -- python -B python/tools/native_expanded_pack_v1.py run ABS_PLAN.json
```

Members start in list order, each on the first free slot. The copy of the
request under `root/requests` differs from the member's only in its placement,
which is that slot. Each goes through the
nested `dispatch` action, so the launcher admits it against its choice as it
does any production request; a busy refusal is retried every minute for an
hour. Before a member starts, the launcher's storage check runs with the
projections of the members already running added to its own; the member waits
for room, and fails if it does not fit alone. A slot is free again once the
member's nested supervisor records that its whole job has exited; work that
ends without a receipt, or a supervisor that ends without that record, fails
the member. A failed member or a STOP file stops new starts, and running
members finish. The optional post command runs after each member on one of
the post CPUs at below-normal priority, so a trace overlaps the next member.
On Linux it runs in the tool's process group, so a signal to that group (or a
host_slots kill or suspension of the tool) reaches it too, and its wrapper is
the subreaper of every process it starts. On Windows it runs in its own
one-CPU job nested in the declaration's job, whose affinity is read back
before the command runs. Post CPUs are declared and share no physical core
with a slot. In the command, `{name}`, `{report}` and `{request_root}` are
replaced and other text, braces included, stays. A post command that fails,
cannot start or outlives `post_timeout_seconds` also stops new starts. The
tool writes `root/log.jsonl` and `root/summary.json` (every failure, the
members not started, and each member's own and rewritten request pins) and
exits nonzero unless every member completed.

**What a stopped post leaves running.** A member's own records are complete
before its post command starts, and stopping the post never touches them; only
the trace can be cut short.

- When the command exits or outlives its timeout, every process it started is
  killed, on Linux even one in a session of its own.
- On Linux, SIGHUP, SIGINT or SIGTERM to the wrapper kills the command and
  every process it started in the same way, however many signals arrive, and
  the wrapper exits 128 plus the number of the signal it stopped for. The same
  signal to the tool's process group also reaches the command, but the wrapper
  kills what is left at once, with no grace period, so a trace that finishes
  its output on SIGTERM can still be cut off mid-write.
- On Linux, a SIGKILL of the wrapper alone stops nothing: the command and
  everything it started keep running. A SIGKILL of the tool's process group (a
  host_slots kill) ends the group but not a process the command started in a
  session of its own, and a SIGSTOP of the group (a host_slots suspension)
  does not pause that process either.
- On Windows the post's job is closed when its wrapper ends, however it ends,
  which kills every process in it.

So use a trace only when its member's `post-end` record in `log.jsonl` shows
exit code 0 (a run stopped during a post leaves a `post-start` with no
`post-end`), or when the trace writes its output atomically, to a temporary
name renamed into place once complete.

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

The workstation side of a lease (packaging, planning, the Pod, staging,
recovery and release) is `nine_deck_cloud_v1.py`; see
[nine_deck_cloud_v1.md](nine_deck_cloud_v1.md).
