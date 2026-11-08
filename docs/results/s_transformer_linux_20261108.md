# S learner Linux qualification

Exact parallel GAE reached **1.377x the baseline training throughput** on
Haley's owned Debian 12 WSL2 guest. The fixed four-update CS prefix had
identical parameter and full Adam state bytes. Mean native elapsed time fell
27.38%.

| Four updates / 40 games | Baseline mean | Candidate mean | Speedup |
|---|---:|---:|---:|
| Complete native training | 17.6245 s | 12.7996 s | 1.377x |
| Training and on-host recovery | 21.3583 s | 16.4195 s | 1.301x |
| Learner update time | 8.6258 s | 4.0030 s | 2.155x |

Each runtime first completed one update at collector/preparation counts 1/1,
2/2, 3/3, 4/4 and 4/2. The supported dispatch gate selected the fastest measured
allocation for each runtime. Baseline used three collectors, three preparation
workers and a serial learner. Candidate used four collectors and four
preparation and learner workers. Both used guest vCPUs 0–3 under an exclusive
Windows reservation. The two observed paired native speedups in AB and BA
order were 1.3649x and 1.3892x.

All ten qualification fingerprints matched, including the ordered trajectory
hashes for the first update. All four ABBA runs matched at every update on the
full parameter and Adam digest and the checkpoint container byte hash. Those
four state digests and four container hashes also match the [Windows prefix
receipt](s_transformer_parallel_20261108.json). The raw trajectory byte check
covers the first update. Later trajectory files carry run-specific checkpoint
paths.

Baseline `483c13669` and candidate `e6f91a9f` used Rust/Cargo 1.94.1, GCC 12.2.0
and GNU ld 2.40 with identical release settings. The initial 10 GiB, two-job
build preflight became unresponsive and the guardian stopped the owned guest.
The retry reused the retained toolchain and caches with one Cargo job, a 12 GiB
guest cap and swap disabled. Six focused Linux dispatcher tests passed. The
measurement used guardian `1234ae4`. Follow-up `9946f70` hardens future cleanup
and changes orchestration only. Hosted CI was queued at reporting.

The on-host recovery timer includes native dispatch and its recovery archive.
Build, evidence return and custody verification were timed separately. Return
took 10.238 s, and return plus copying and verification took 19.090 s. The
577,814,432-byte archive has the same SHA-256 on the E: and independent D:
custody disks. The owned VM drained, the Windows reservation was released, and
`.wslconfig` returned to its original absent state. Windows C: had 63.0 GiB free
at verification, above the 60 GiB floor.

This qualifies the ordinary S learner on the measured WSL placement for this
initial-state prefix. Guest vCPU affinity is scoped to the guest. Native fleet
deployment, later checkpoints, larger allocations, auxiliary updates and
playing strength each require their own evidence. Claude's next native run
requires a representative qualification on its full layout. Acknowledgement
is pending in the [handoff PR](https://github.com/jackmaiorino/mtg-kernel-collab/pull/381).
Runtime, seeds, output hashes and custody records are in the
[qualification manifest](s_transformer_linux_20261108.json).
