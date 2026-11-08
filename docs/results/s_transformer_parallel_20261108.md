# S learner parallel qualification

Exact parallel GAE improved completed training throughput by **1.337x** on a
Windows allocation with two physical cores and four hardware threads. Mean native elapsed time fell
25.23%. This qualifies the measured ordinary S path on this allocation; it does
not establish Linux fleet performance or playing strength.

| Four updates / 40 games | Baseline mean | Candidate mean | Speedup |
|---|---:|---:|---:|
| Complete native training | 16.7185 s | 12.5000 s | 1.337x |
| Training plus on-host recovery | 19.4690 s | 15.1715 s | 1.283x |
| Learner portion | 9.2320 s | 4.9068 s | 1.881x |

The ABBA comparison used the same first four CS batches, original episode seeds,
S0 state and net-8 opponents. Both runtimes selected four collectors and four
preparation/learner workers on logical CPUs 8-11 of a Ryzen 9 5900XT. Before ABBA,
each runtime completed one update at collector/preparation counts 1/1, 2/2,
3/3, 4/4 and 4/2. Every observation was retained; the supported dispatch gate
received each collector count's fastest preparation setting.

The two paired native speedups were 1.3404x and 1.3346x. These are two observed
pairs, not a statistical confidence interval. All ten qualification fingerprints
matched, including their ten ordered trajectory hashes. All four ABBA runs
matched at every update on both the full parameter/Adam digest and the state
container's byte hash. Later raw trajectory documents contain different
checkpoint-path provenance; their raw bytes were not compared across arms.

Baseline `483c13669acee8e514b34874e60e7c1ce04ec59b` and candidate
`e6f91a9f255cd575612fcdf16dc5fec062635795` were built with Rust/Cargo 1.94.1,
MSVC linker 14.50.35725.0, release optimization, LTO disabled and 16 codegen
units. Subsequent delivery commits change orchestration and documentation only.
Thirty native checks and four focused dispatcher tests passed; the ignored
native test is an existing optional timing receipt. Hosted CI was queued at delivery.

The active Linux chains and the desktop human-play session were excluded.
Haley's previous soak had ended before launch. The canonical reservation was
released after successful qualification. The 6 GiB output cap and 60 GiB free
disk floor were preserved. On-host recovery timing excludes the measured
62.820 s inbound transfer and subsequent evidence return/custody work.
The sealed archive on E: and independent recovery on D: passed byte-hash checks
for all 1,167 files. Evidence return took 169.415 s and recovery took 25.747 s.

Runtime, input, seed, output and custody identities are in the
[small manifest](s_transformer_parallel_20261108.json). This short initial-state
prefix leaves later checkpoints, larger allocations and Linux deployment for
their own representative qualification.
