# Heuristic backup controls

2026-09-22. B/v4-core-controls-007 complete37/37pass,0.77stest/280.406925sinvocation. B=E:/mtg-meta-recovery-20260921. Four BelowNormal Cargo jobs, guarded ownership, pinned toolchain and14sourcefiles, no GPU calls or formal games. No natural-root replay yet. Tests are implementation correctness evidence, not strength or M1.

Structural checks: sparse two-world action wins yield optimistic H10000, balanced win/loss samples yield0; weighted terminal/live/cutoff accounting, incoming-count and depth failure cases; diamond refresh through both parents and idempotence; root-relative signs across consecutive same-actor decisions, both seats; single-action selection. Existing core/sampler/key/terminal controls pass.

Engine both-player Bolt life3 fixture: Off/Report/Blend,128simulations/1024transitions/depth8/seed29, both seats. Report minus optional estimator equals Off exactly; Off JSON omits estimator. Recompute count equals simulations. Each of the four reported mode/seat trees has2edges with multiple live successors. This verifies real sampler/key/binding integration for successor aggregation beyond the natural fixtures' single-child topology. No calibration claim.

| Seat/mode | Root estimates E | Root empirical means | Multiple-successor edges |
|---|---|---|---:|
| P0 Report |[3065,-206]|[3765,196]|2|
| P0 Blend |[3065,-144]|[3765,213]|2|
| P1 Report |[5002,-108]|[5640,232]|2|
| P1 Blend |[5002,-76]|[5640,72]|2|

Fresh design review B/v4-backup-design-review-001 and same-session source follow-up B/v4-backup-source-review-001 completedexit0/is_errorfalse with actual reads. Session51b2f20d-37c9-4452-b17e-293eb157269f. Source verdict proceed, no defect found in accounting/parity/signs/transpositions. Accepted: existing Off routes unchanged; no estimator update in Off; exact Report control; child creation counted once; sorted global graph refresh; optional output field omitted for Off. Reviewer withdrew at-any-budget impossibility. Optional redundant nodecount/rootSome assertions and extra exact blend-score assertion are deferred; current runtime checks and both-seat controls cover their substantive invariants. Independent replay reconstruction of every E/H from leaf/count/link inputs remains required.

No replay wrapper added yet. Next expose explicit Off/Report/Blend consumed-root diagnostic, keep original allocation diagnostic unchanged, compare Off to archived RR/PriorFree and Report to Off, measure wall time outside semantic output and independently reconstruct E/H. No retuning based on outcome, no playing override. g115 unchanged, M1 unmet.
