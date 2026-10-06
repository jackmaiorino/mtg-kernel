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
  "root": "ABS/qualification-jack-ordinary-w1",
  "cold_root": "OTHER/qualification-jack-ordinary-w1",
  "placement": {
    "host": "jack", "cpu_affinity": [0, 1, 2, 3],
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
    "jack": {"eligible": true, "reason": "CURRENT EVIDENCE", "checked_at": "ISO UTC",
      "evidence": {"path": "ABS/census.json", "sha256": "SHA256"},
      "cpu_affinity": [0, 1, 2, 3], "transport_seconds": 0},
    "haleyspc": {"eligible": false, "reason": "CURRENT EVIDENCE", "checked_at": "ISO UTC",
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
analysis remain in the accepted research lane. No paid launch path is provided.
