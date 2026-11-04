# Checkpoint v2 acceptance (design 7.2): amendment 2, the pool moves to cores 4-23

Status: pre-run amendment to design revision 10, section 7.2 (container PR #468, harness PR #508, amendment 1 PR #509). No full-scale block has run on any host. No outcome, contrast, calibration or counter from the production host exists or was used.

## The rule as frozen

The measurement runs inside one host_slots claim on the pool cores 0-19 of the production host (an i7-13700K: logical 0-15 are the 8 P-cores with their hyperthreads, 16-23 the 8 E-cores). A block is contaminated when another claim or a timed hold overlaps the pool.

## Why it cannot run as written now

A long GPU campaign holds cores 0-3 of the same host (two claims, one per GPU) until about 11-07. A 0-19 claim cannot be granted while they run, and every claim poll would see them as overlap. Re-pinning a running production campaign was judged not worth the risk. Cores 4-23 are otherwise idle; waiting would leave 20 cores unused for about three days.

## Amendment

The pool becomes **4-23** (20 cores: 12 P-core threads on 6 physical P-cores, plus the 8 E-cores). The harness's `formal_placement` accepts exactly 4-23 and refuses 0-19, any subset, any superset and `own`. Contamination and the start precondition count only claims and timed holds that overlap 4-23, so the campaign on 0-3 is not overlap; a claim on any core of 4-23 still is. The pool is recorded in the manifest constants.

Nothing else changes: the spec (sha256 cc5d8546, binaries 0092d247 and 4b2d6cd3), the runs, segments, seeds, schedule, calibration, decision rule, margin, guard, contamination thresholds (with amendment 1's floor), precondition, settle, retention and re-measurement rule are as before. The spec's free-text note that names 0-19 is not read by the harness and is superseded by this amendment.

## Why the decision is unaffected

Every acceptance criterion is relative and measured inside one pool on one host. Throughput enters only as within-pair log ratios ln(B / A) of adjacent blocks that run in the same pool, and the calibration (A against A) that sets validity, precision, n and the read baseline runs in that same pool before any v2 block. No criterion uses an absolute throughput, a wall time or a per-core figure. The pool sets the absolute speed and the wall time, not the comparison.

## Cost, stated plainly

- **Composition.** 12 P-threads plus 8 E-cores, against 16 plus 4 before. The pinned training binary opts out of Windows power throttling at startup (#357), so below-normal runs are scheduled onto the P-cores in the pool.
- **Wall time.** Aggregate capacity is close (two fewer physical P-cores, four more E-cores), but the E-core share doubles and the campaign on 0-3 shares memory bandwidth and cache. The estimate assumes blocks 1.0 to 1.3 times as long as on 0-19: about 3.8 to 4.6 h when clean at n = 6, and up to about 8 h at the maximum of 37 groups (76 to 160 pool core-hours), before any start-window delays.
- **Comparability.** Absolute throughput figures from this run (block throughputs, per-update walls) are not comparable to figures from a 0-19 pool or another host. Only the run's own relative contrasts are its result.
- **Noise.** The campaign on 0-3 adds some shared-resource noise to both arms equally. The calibration measures it, and if it is too large, the precision condition fails and the attempt ends inconclusive, as frozen.

## Effect on the decision

The amendment changes only which cores the measurement uses and which claims count as overlap. It is fixed before any full-scale block runs, and no outcome from any host informed it.
