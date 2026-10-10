# Training speedup implementation and comparison

Status: implementation verified and 16 short qualifications complete on two PCs.
The corrected full comparison remains pending; total end-to-end speedup
is not yet measured. Prior attempts and their exclusions are preserved below.

This follows the completed [throughput audit](../training_throughput_20261009/README.md).
Three GPT-6.1 Sol agents implemented learner arithmetic, collection/model reuse,
and artifact processing from baseline `5242a984c2bd8d878c9a5fbc4b3cacab9798d08b`.
The integration branch is `codex/training-speedups-20261009`.

## Comparison

Use the unchanged r5 first-block schedule from the completed nine-deck recipe:
162 updates, ten scheduled games/update, T1 checkpoint `88c0b997...`, CPU GAE,
the existing seeds and full Adam continuation. Compare baseline and combined
candidate through `native_expanded_dispatch_v1.py`, including fingerprinting,
storage checks, two recovery shards and full readback. Qualify one worker and
increasing useful counts independently for each binary before full blocks.
Use matched baseline/candidate blocks in balanced order and compare every
ordered trajectory and complete checkpoint fingerprint. Report execution,
dispatch, archive, exposure/retention and transport separately. Never add
overlapping stage timers or treat controller polling latency as training time.

Pinned Rust 1.94.1, the same MSVC linker and release profile apply to both builds.
Use default-disabled diagnostics for the primary speed comparison; run the
GAE profiler separately and check its output parity and overhead. CPU-only
work uses no GPU ordinal. Changed numerical backends are outside an exact
implementation speedup claim.

## Current placement

At 2026-10-09 21:58 UTC, Jack's PC held stage4a reservation generation412 and
all configured shared build cores were claimed. Haley had no reservation or
shared core claims, 16 logical CPUs, 22.35 GiB available memory and 69.68 GiB
free on C:. Four-core BelowNormal release builds of both variants passed under
Haley's canonical `host_slots_v1` wrapper. All 67 affected native tests passed.
An initial build-helper attempt stopped at
an interactive linker help command; that owned process tree was stopped and
the helper repaired before compilation. No training measurement was started
by that attempt. The local queued baseline build was cancelled before work.

Fresh RunPod REST and GraphQL inventory reads returned HTTP403. No new paid
allocation is authorized. This is unavailable inventory, not evidence that
pods are stopped. Existing storage reserves and other owners' work remain
binding. Raw benchmark artifacts stay outside Git; compact receipts and hashes
will accompany measured results.

## Verification and remaining work

Pinned Python 3.13.14 checks passed: 26 dispatcher tests, four archive tests,
and three storage tests; four Linux-only dispatcher tests were skipped.
Native parity tests passed, including the pinned two-update GAE state and
real serial/parallel games. Complete matched blocks, final current-head
checks and integration remain pending; the short qualifications are below.
Review found and repaired two collector issues: a zero enclosing profiler
duration and source-pin verification across cached collection invocations.

Full canonical checkpoint encoding, real continuation-loader readback, final
byte reverification and durable recovery are preserved. Overlap or removal of
those stages is not counted as an implemented or measured speedup.

## Reproduction and storage accounting

`prepare_requests.py` creates pinned requests and plans without execution.
Every executable comparison plan uses `case_driver.py`, which invokes the
supported reserved launcher and records the immediate logical preflight.
Run the baseline launcher with only the three-line child elapsed-time
instrumentation applied after its immutable binary build. Preserve its hash.
The shared formal native-output path is intentional: later trajectories pin
earlier checkpoint paths, so distinct native paths would alter output bytes.
Dispatch receipts and recovery roots remain unique to each case.

The previous r5 first block wrote 10,430,088,126 raw bytes and 3,498,824,608
recovery bytes. The full logical projection must cover at least their sum.
Both variants use the same NTFS-compressed parent directories. Any smaller
physical projection must come from measured allocated bytes and include
recovery, temporary writes and margin. The launcher dual-uses its projection
for physical reserve and logical accounting; the case driver additionally
enforces current logical bytes plus the full logical projection against the
192 GiB allowance. The 60 GiB physical reserve remains unchanged. First-block
physical growth must be reconciled before continuing. A storage admission
failure does not authorize lowering an unmeasured projection.

At 22:44:48 UTC the first qualification was refused by the canonical host
reservation guard before either native or cold output roots existed. The
22:45:14 inventory confirmed Haley generation 156 belongs to
`spellbench-xmage-native`, work `codex-learned-full-qualification-001`;
Jack generation 413 belongs to `claude-stage4a-20261009`. The failed controller
receipt is preserved. This preflight failure is not a timing sample. The next
attempt requires an observed reservation release and the same guarded launch.
Coordination and current state are recorded in collab PR127.

At 22:55:44 UTC, one bounded qualification waiter started on Haley as PID
132784 (creation time `134360601440142588`). Its observed state was
`waiting-reservation`, with generation 156 still held by live adopted work.
`resume_qualifications.py` waits on canonical lock-directory events, then
uses the existing supported dispatcher for atomic admission. It preserves
the first refused receipt, stops on a failed case and has a two-hour waiting
deadline. The waiter covers eight qualifications and allocation measurement;
it cannot launch full comparison blocks. Its job and current-state receipts
are under `C:/mtg-node/training-speedups-20261009/qualification-resume-*.json`.
The launched waiter is recorded in merged collab PR128.
The qualifications were pending at that October 9 observation.

## October 10 qualification result and placement

All eight guarded qualifications finished before the waiter stopped in the
allocation helper. Each completed one update and ten games. Every ordered
trajectory, complete optimizer/checkpoint and ledger fingerprint matches
across both variants and all worker counts. The canonical fingerprint digest
is `bbefb6d1d8f790812d95bcd22df0bd7c0e996043167ee544292c23cae659c9a8`.
The compact `qualification-analysis-20261010.json` contains the exact receipt
pins and timing table.

| Workers | Baseline dispatch seconds | Candidate dispatch seconds |
| --- | ---: | ---: |
| 1 | 18.945 | 14.291 |
| 2 | 15.943 | 11.716 |
| 4 | 15.385 | 12.276 |
| 8 | 15.290 | 12.899 |

The best short qualification is about 1.31x faster (baseline eight workers,
candidate two). This includes guarded dispatch and archive but excludes the
full-block maintenance and independent recovery transfer. It is not the
requested total speedup and does not establish a full-run estimate.

The allocation helper falsely rejected unchanged files because Windows
`DirEntry.stat()` returned inode zero while a subsequent `Path.lstat()`
returned the real file ID. Both observations now use fresh `lstat()`, and
measurement errors are reported before the empty-tree check. Mutation and
reparse rejection remain enforced. The preserved attempt-2 measurements
completed with zero errors: the full-block physical projection is
15,669,643,620 bytes, exceeding Haley's 6,820,810,752 bytes above the 60 GiB
reserve at that observation. Haley's later free-space observation is recorded
below; this exclusion is historical evidence, not its current eligibility.

At 12:07 UTC both host reservations were free; Jack's D: had 322,815,766,528
bytes free and RunPod still returned 403. Desktop qualification subsequently
completed using the same frozen binaries and unchanged input pins,
with owned D: output roots bound in new requests. All 23 CI
checks passed at `c4e75006a1524d8bcfddd4509d83fee24be8fa09`, including the
separate delivery review's repaired recovery/pruning checks.

All eight desktop cases completed at 12:22:06 UTC. Receipt chains, all 160
native inventory file hashes and complete training fingerprints verified;
the fingerprint digest is identical to Haley's. The compact
`desktop-qualification-analysis.json` records the source pins.

| Workers | Baseline dispatch seconds | Candidate dispatch seconds |
| --- | ---: | ---: |
| 1 | 12.053 | 15.896 |
| 2 | 10.114 | 7.654 |
| 4 | 9.533 | 7.181 |
| 8 | 9.312 | 7.265 |

The best desktop short qualification is 1.297x faster, using eight baseline
workers and four candidate workers on the same eight physical P cores
(logical CPUs 0,2,4,6,8,10,12,14). This remains preliminary dispatch timing,
not the full end-to-end speedup. The candidate's slow one-worker sample is
preserved without a causal explanation. Both variants' measured full-block
physical projections are below 15.67 GB and logical projections below
29.71 GB. D: has 256.26 GB above the 60 GiB reserve. Formal requests use
conservative 16 GiB physical and 32 GiB logical projections, a 160 GiB
desktop allowance and 32 GiB reserved for existing Haley artifacts.

The preserved formal-v2 attempt began at 12:35:51 UTC on October 10.
Its first native baseline completed all 162 updates and 1,620 games, with
800.104 seconds of guarded dispatch. That timing is excluded from the clean
comparison because two unpinned Cargo release builds overlapped its declared
timed P cores from 12:37:17 to approximately 12:38:40 UTC. The original
report, checkpoint fingerprint and recovery evidence remain preserved.
`desktop/formal-launch.json` identifies this historical attempt; it is not
the current clean launch receipt.

All four public `check-choice` calls passed. Eight affected recovery tests
passed on the desktop, including real local plan construction. Review also
repaired full-block reconciliation to use the allocation helper's raw inventory
API; its CLI intentionally accepts only the one-update qualification sample.
The raw API check completed across 1,050 files without errors. First-block
reconciliation is separate audit overhead. Results will report phase totals
and the actual coordinator wall envelope without adding nested timers twice.

## October 10 delivery observation

The authoritative coordinator stopped at 12:49:25 UTC with zero completed
cases: `block-01-baseline/inspect` failed because the exposure collector could
not find the registered nine 75s. Its loader derives the repository from the
runtime decks path, so staging only the runtime file and Python tools omitted
the required registration and card-map layout. The coordinator now validates
those dependencies before dispatch and pins them in its stability checks.
The failed attempt and its frozen source copies remain unchanged.

After the missing registration and card-map dependencies were staged, the
existing baseline's inspection, independent E: recovery copy and retention
completed. The continuation stopped on the next candidate's guarded dispatch
refusal before native execution. The compact
`desktop/contamination-disposition-20261010.json` records both attempts,
the complete recovery receipt and the decision to preserve the numerical
reference while excluding the contaminated timing from clean ABBA.

At 13:12:49 UTC Haley had 86,399,787,008 bytes free, enough for the conservative
16 GiB physical projection above the 60 GiB reserve. The new placement
comparison therefore includes desktop and Haley; RunPod remains unavailable
with HTTP403 and no paid authority. Existing eight-case qualifications per
host are reused with their unchanged runtime, schedule and full fingerprint
bindings. No rebenchmark is claimed.

`desktop/haley-qualification-mirror-20261010.json` records local report refs
and supported path mappings for all eight Haley cases. Requests, configs,
runtime, execution and archive receipts retain their captured bytes; native
payloads were recovered from the two pinned ZIP shards with exact member
inventory and SHA verification. The original scientific input pins remain
unchanged. This mirror supports full public output verification without
rewriting remote paths in captured artifacts.

The local desktop transport sample measured 27,226,168 bytes through D: SHA,
E: copy/fsync and full E: SHA in 0.7205 seconds. The analogous Haley sample
measured 27,225,149 bytes through actual SCP, D: SHA and independent E:
copy/fsync/readback in 2.0152 seconds. Their conservative 32 GiB linear
transport projections are 909.23 and 2,543.24 seconds respectively, with
cache and scaling limitations recorded in the sample receipts. Haley's
receipt is `desktop/haley-transport-sample-20261010.json`. These costs enter
the supported allocation comparison; actual complete recovery times must
still be measured for every formal case.

Formal-v3 began native execution at 13:39 UTC after all four public choice
checks passed. Its expanded timed reservation applied Windows job affinity
to the native child: live process readback was mask 65535, despite the
request's qualified mask 21845. The earlier statement that the child retained
the eight even CPUs was incorrect. This attempt is excluded from timing.
Only the identified native child was stopped; normal dispatcher failure and
reservation release completed at 13:44:29. Its 78 completed iterations and
partial output tree remain preserved, with no deletion. The compact stop
and move receipts record 1,439 files totaling 5,287,511,627 bytes.

Formal-v4 restores the original qualified timed guard, which binds both the
job and native process to CPUs 0,2,4,6,8,10,12,14. CI can still use 16-23.
A read-only observer now checks actual native process identity and affinity
immediately after launch, saving a pinned receipt before the completion wait.
Final analysis requires mask 21845 for every case as well as full output
parity and verified independent recovery.

An initial v4 preparation found a stale generated case label and stopped
before any native work. Its two choice files and failure logs are preserved.
The corrected launcher, PID 7360, started at 13:49:01 UTC, pinned in
`desktop/clean-v4-repaired-job.json`. The generated `clean4-*` labels were
checked against the coordinator and analyzer before this attempt. Frozen
binaries, scientific inputs and prior deployed sources remain unchanged.
The first v4 native baseline started at 13:50:18 UTC as PID 12416. Its live
identity and mask 21845 were verified and saved in
`desktop/formal-v4-initial-affinity.json`.

Current state is
`D:/training-speedups-20261009/desktop/formal-v4-launcher-repaired/state.json`,
then `desktop/formal-v4/coordinator/state.json`. The bounded local event
waiter handles reservation release, and the existing
`finish-training-throughput-comparison` heartbeat checks completion every
15 minutes. Queue time is separate from case timing and remains in the
coordinator wall. No complete ABBA result or final speedup is claimed yet.
