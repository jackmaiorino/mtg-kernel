# Exact parallel S learner

The ordinary S learner spends most of its update in sequential forward and
backward work. Prepared ordinary GAE updates now use their existing
`preparation_workers` count for bounded independent substeps. Workers retain the
operands of parameter-gradient contributions. Replay partitions independent
parameter rows and applies their summands in original substep and reduction
order, then the existing Adam update runs. Worker-local gradient reductions are
not used. One-worker APIs and auxiliary EX1/W5 updates retain serial execution.

The correctness requirement is exact gradient, loss, parameter and full Adam
state equality, including continuation after checkpoint reload. Tests also cover
multi-substep groups, repeated embedding entries, original error precedence and
the real prepared update route. Each arithmetic worker normalizes MXCSR.

Qualification uses a copied first-four-update prefix of the current CS workload,
including its original ten-game batches, initial S0, opponents and seeds. The
production chain ledger is omitted from this independent engineering replay;
input content and path rebasing are recorded in the small run manifest.

The supported native CPU dispatcher first compares one complete update at
1/2/3/4 workers and the production four-collector/two-learner setting for each
pinned baseline/candidate binary, verifies equivalent
outputs, and selects its fastest measured eligible placement. It then runs the
complete prefix in ABBA order, retaining every output and failed attempt.
Report actual child wall time, recovery-inclusive time, learner timing and
every full optimizer-state container hash. `scripts/experiments/s_transformer_parallel_v1/qualify.py`
composes this existing reservation and throughput gate. Its plan pins runtime,
inputs, host inventory and the CPU allocation. `s_transformer_v1 gae-bench` is a
bounded synthetic diagnostic only, never end-to-end qualification evidence.

The active Linux training chains and Jack's live human-play session are excluded
from placement. Haley's soak ended before qualification staging, leaving that
PC available. Its two-core/four-thread allocation uses a 6 GiB retained-output
cap and preserves 60 GiB free disk space. The short prefix fits this capacity;
the original 16-update fixture is retained. No active executable, reservation,
checkpoint, seed, statistical rule or campaign is modified. Speed measurements
on this host do not establish speed on the Linux nodes or playing strength.

Thirty native checks passed, including full gradient bits, warmed Adam state,
real-game training and resume. The matched qualification passed with 1.337x
native throughput and identical full Adam states. See the
[result and scope](../results/s_transformer_parallel_20261108.md).
