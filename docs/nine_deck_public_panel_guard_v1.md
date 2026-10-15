# Pinned public game-one panels

The legacy public-panel build and reservation launcher remain the default.
Pinned explicit panels use the same `case_match`, native evaluator and recovery
semantics through a host_slots-backed resource guard. No training command or
native model behavior changes in this launcher.

`build_from_cases(panel, candidate, opponent, decks, out, shards, cases_pin,
metadata, game_one_only=True, native_root=None)` reads and hashes the existing
case document. Its `decks` must equal the baseline deck-name order. Each case
contains integer `own`, `other`, `family`, `seat`, `starting_player`, a u64
`seed`, and `reference`. Metadata contains `lineage`, `endpoint`, `reference`.
Only reference filtering and seat sharding occur; seeds are never generated.
The existing scorer screen SCHEDULE can be passed directly. Its declared
count and complete deck/family/seat/reference coverage are checked.

One reference has 972 cases in that schedule. Two references, two lineages
and two endpoints produce eight plans and 7,776 physical game-one cases.
Plans, requests and row exports use create-only publication. Every use
rechecks the original case pin, exact shard coverage/order and native match
requests. Explicit rows retain family and all three metadata fields.

Optional `native_root` separates prepared plans from the fresh execution
tree. For a shared attempt use `attempt/native/<plan-id>` for each plan's
native root, then run all plans inside one guard rooted at `attempt/native`.
All preparation, logs and outputs must also be inside the declared storage
accounting roots. Existing native roots or logs are never overwritten by the
guarded path. Recovery verifies original/continuation pins and native start
identity, retains missing cases, and uses the same attempt budget.

## Resource guard

Run through `host_slots_v1.py run` with the requested BelowNormal core claim.
The new `qualify` and `run` options are `--guard-plan FILE --guard-root FRESH`.
The guard plan has this shape (replace the illustrative values and pins):

```json
{
  "schema": "public-panel-guard/v1",
  "evaluator": {"path": "C:/pinned/public_feature_evaluation_v1.exe", "sha256": "..."},
  "cpu_affinity": [1, 3],
  "qualification_limits": {"cpu_seconds": 120, "wall_seconds": 180},
  "run_limits": {"cpu_seconds": 600, "wall_seconds": 900},
  "storage": {
    "accounting_roots": ["D:/e-scratch/owned-panel"],
    "max_physical_bytes": 1073741824,
    "projected_additional_bytes": 67108864,
    "reserve_bytes": 64424509440
  },
  "stop_files": ["D:/e-scratch/owned-panel/STOP"]
}
```

Only the selected action's limits are needed. The existing archive `Guard`
provides suspended child admission, CPU affinity, process-tree CPU including
exited descendants, wall/STOP/storage/reserve checks and drain verification.
It also checks its fresh root's STOP file. Every evaluator launch checks the
evaluator pin, command source pins and output containment. The guard stays
active through recovery and final publication. Failed attempts remain.
The public wrapper opts into reservation pause/resume under a live
BelowNormal host_slots claim. It pauses only its contained tree while a
reservation conflicts, retaining CPU, wall, storage and STOP checks. Wall
time includes pauses. Qualification binds the pause policy and exact archive
Guard bytes; final guard records include observed pause transitions. Existing
archive training dispatch keeps its default immediate conflict behavior.

`with_guard(guard_plan, root, qualifying, operation)` exposes the same scope
to Python callers. `operation(guard)` may qualify or run several panels; the
budget is not reset per panel. Batches run sequentially within that callback,
and every batch verifies descendant drain before the next worker trial or
panel begins. Qualification receipt pins are recorded in the
successful drained `guard-result.json`. Caps across separate host attempts
must be allocated from the campaign's remaining budget by its existing
manifest and placement decision; this helper does not allocate other hosts.

## Qualification and evidence reuse

Explicit qualification requires `--qualification-cases FILE`, another pinned
document with the same case fields and deck order. Use a predeclared
qualification-only namespace. All selected cases run; an optional `--sample`
must equal their count. Both learner seats and all nine own/opponent deck
indices must occur. Formal schedule seeds cannot occur in this document.
Sources remain seat-routed through the plan's candidate and opponent, so
T1/H and each lineage need their corresponding plans. The worker list must
start at one and include a larger count within the claimed cores. The
qualifier compares exact ordered native match hashes at every worker count
and selects the fastest complete batch. Guarded timing includes request
publication, native startup/execution/output publication, and output hash/pin
verification; the native execution portion is also reported separately.

Run admission checks evaluator, mode, schedule, opponent and model layout,
plus host, affinity and launcher bytes. It verifies the measured request and
completion pins, actual native output hashes, timing arithmetic, case/source
coverage (including each saved native match against its requested case and
game-one result shape), and successful drained guard publication before any production
spawn. Layout reuse permits new candidate weights; it is throughput evidence,
not a claim that different weights have identical outputs. Pin the actual
runtime containing the required model loader and game-one implementation.

BO3 versus game-one first-game parity is a separate small correctness check.
A caller can use `with_guard` and `run_requests(..., guard=guard)` for pinned
requests differing only in mode/output path. Compare first-game records,
first policy resets and captured `game_index == 1` decision rows. Whole BO3
and game-one result hashes intentionally differ. Actual T1/H admission and
this parity check remain required workload evidence; synthetic Python-child
tests do not establish model admission or playing strength.

Explicit row export verifies each completed segment's native completion mode,
case count and ordered output hashes, and compares every saved match with its
exact requested configuration and registered decks. A failed segment retains
only its contiguous completed prefix after checking the native start, saved
model identities, case binding and original failure. Recovery errors must
match that failure record. Every explicit row carries its original segment
request pin, including technical terminations; missing rows are not invented.
