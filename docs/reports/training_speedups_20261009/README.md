# Training speedup implementation and comparison

Status: implementation and verification in progress. No achieved speedup yet.

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
real serial/parallel games. Guarded qualification, complete matched blocks,
current-head CI and integration remain pending.
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
