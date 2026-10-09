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
free on C:. A four-core BelowNormal baseline build is running under Haley's
canonical `host_slots_v1` wrapper. An initial build-helper attempt stopped at
an interactive linker help command; that owned process tree was stopped and
the helper repaired before compilation. No training measurement was started
by that attempt. The local queued baseline build was cancelled before work.

Fresh RunPod REST and GraphQL inventory reads returned HTTP403. No new paid
allocation is authorized. This is unavailable inventory, not evidence that
pods are stopped. Existing storage reserves and other owners' work remain
binding. Raw benchmark artifacts stay outside Git; compact receipts and hashes
will accompany measured results.

## Verification and remaining work

Python dispatcher/storage tests and formatting passed in the implementation
lanes. Native parity tests, corruption regression checks, guarded qualification,
complete matched blocks, current-diff review and integration remain pending.
Review found and repaired two collector issues: a zero enclosing profiler
duration and source-pin verification across cached collection invocations.

Full canonical checkpoint encoding, real continuation-loader readback, final
byte reverification and durable recovery are preserved. Overlap or removal of
those stages is not counted as an implemented or measured speedup.
