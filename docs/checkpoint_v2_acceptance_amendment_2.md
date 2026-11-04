# Checkpoint v2 acceptance (design 7.2): amendment 2, the pool moves to cores 4-23

Status: pre-run amendment to design revision 10, section 7.2 (container PR #468, harness PR #508, amendment 1 PR #509). No full-scale block has run on any host. No outcome, contrast, calibration or counter from the production host exists or was used.

## The rule as frozen

The measurement runs inside one host_slots claim on the pool cores 0-19 of the production host (an i7-13700K: logical 0-15 are the 8 P-cores with their hyperthreads, 16-23 the 8 E-cores). A block is contaminated when another claim or a timed hold overlaps the pool.

## Why it cannot run as written now

A long GPU campaign holds cores 0-3 of the same host (two claims, one per GPU) until about 11-07. A 0-19 claim cannot be granted while they run, and every claim poll would see them as overlap. Re-pinning a running production campaign was judged not worth the risk. Cores 4-23 are otherwise idle; waiting would leave 20 cores unused for about three days.

## Amendment

The pool becomes **4-23** (20 cores: 12 P-core threads on 6 physical P-cores, plus the 8 E-cores). The harness's `formal_placement` accepts exactly 4-23 and refuses 0-19, any subset, any superset and `own`. Contamination and the start precondition count only claims and timed holds that overlap 4-23, so the campaign on 0-3 is not overlap; a claim on any core of 4-23 still is. The pool is recorded in the manifest constants.

Nothing else changes: the spec (sha256 cc5d8546, binaries 0092d247 and 4b2d6cd3), the runs, segments, seeds, schedule, calibration, decision rule, margin, guard, contamination thresholds (with amendment 1's floor), precondition, settle, retention and re-measurement rule are as before. The spec's free-text note that names 0-19 is not read by the harness and is superseded by this amendment.

## Why the decision rule is unaffected

The statistical criteria are relative and measured inside one pool on one host. Throughput enters only as within-pair log ratios ln(B / A) of adjacent blocks in the same pool. The calibration (A against A) that sets validity, precision, n and the read baseline runs in that same pool before any v2 block. No statistical criterion uses an absolute throughput, a wall time or a per-core figure.

The operational guards are unchanged and are absolute: the disk limits (60 MB/s reads, 50% busy, amendment 1's floor), the claim-poll gap and the 3600 s block timeout. A block was projected at about 1.8 minutes on 0-19, so the timeout would need a slowdown of about 33 times. The pool composition alone cannot plausibly cause that.

The result is scoped to the 4-23 pool. The relative speedup of v2 over v1 need not equal what a 0-19 pool would have shown, because the two formats may load P-cores and E-cores differently. The pool sets the absolute speed and the wall time; the decision rule applied to the measured contrast is unchanged.

## Cost, stated plainly

- **Composition.** 12 P-threads plus 8 E-cores, against 16 plus 4 before. The pinned training binary opts out of Windows power throttling at startup (#357). That lets the scheduler use both core types within the claim's affinity, rather than keeping below-normal work on the E-cores; it does not guarantee any particular placement.
- **Wall time (a conditional planning estimate, not a measurement).** Aggregate capacity is close (two fewer physical P-cores, four more E-cores), but the E-core share doubles and the campaign on 0-3 shares memory bandwidth and cache. Assuming blocks 1.0 to 1.3 times as long as on 0-19 (a group cycle of about 9.8 to 12 minutes): about 3.8 to 4.6 h when clean at n = 6 (21 groups), and about 6.0 to 7.4 h at the maximum of 35 groups (3 warm-ups, 12 calibration, 20 comparison). That is 76 to 148 pool core-hours, before any start-window delays.
- **Comparability.** Absolute throughput figures from this run (block throughputs, per-update walls) are not comparable to figures from a 0-19 pool or another host. Only the run's own relative contrasts are its result.
- **Noise and interference.** The campaign on 0-3 shares memory bandwidth, cache and the power budget with the pool. Its interference need not affect both formats equally, and it can change over the run. The A-against-A calibration measures v1's variability during the calibration only. It cannot detect later changes in the interference, or a format-specific sensitivity to it. The claim and disk rules do not see this kind of interference. It is a stated limitation of this placement.

## Effect on the decision

The amendment changes only which cores the measurement uses and which claims count as overlap. It is fixed before any full-scale block runs, and no outcome from any host informed it.
