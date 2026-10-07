# Equal128 comparison passes the frozen local feasibility gates

All10 jobs,100 unique reserved archive games and48 fresh tactical positions completed. All11 declared gates passed with no threshold or endpoint changes. The semantic control now fits32/32 of its own teaching labels, resolving the specific inadequate-control failure of the earlier32-update comparison. That earlier result remains NO ADVANCE; this is a distinct equal128 comparison on fresh reserved inputs.

| Final model | Fresh winning choices /32 | Own teaching targets /32 | Mean policy TV from g115 |
| --- | ---: | ---: | ---: |
| g115 parent |16|16|0|
| Correct, no retention |32|32|0.171177|
| Correct, with retention |32|32|0.052156|
| Wrong-label semantic control |0|32|0.117880|

For g115 the teaching column measures correct targets without any new optimization. The semantic arm instead measures its deliberately wrong control targets.

Both correct learners score8/8 in each fresh family/seat cell, face-required and creature-required for P0/P1. g115 scores0/8 face and8/8 creature in each seat; semantic scores0/8 everywhere. The other16 fresh positions are no-win controls with null optimal-action accuracy. These48 cases are correlated synthetic relatives of the teaching task, not48 independent games. Under production sampling, retained mean winning probability is0.8450 on face-required cases and0.9987 on creature-required cases; perfect argmax choices do not imply deterministic perfect play.

Retention TV averages rows within physical decisions, then decisions within games, then100 equal-weight games. The retained/unretained ratio is0.304689, a69.53% reduction in movement from g115. Paired retained-minus-unretained difference is-0.119021,95% percentile interval[-0.145727,-0.098149], using the frozen20,000 PCG64 draws with seed2026092102 over10 paired archive-update blocks. This measures policy movement, not win-rate gain or value calibration.

The100 archives contain19,647 rows,19,399 multi-action rows and17,826 physical choices. Changed top actions: unretained3,368 rows/3,273 physical choices; retained1,062/1,034; semantic2,076/2,015. Per-game mean physical top-change fractions are0.166084/0.055899/0.124144 respectively. Mean absolute value movement is0.268885/0.139648/0.175736, not calibrated error. All9,499 opponent behavior rows were replay-checked:5,057 g115 and4,442 other-parent rows.

Coverage includes all8 decks and both seats,7..16 game appearances per deck/seat,52 preboard and48 postboard games. Retained TV is lower in every deck/seat and every action category. Residual retained TV is largest for targets(0.174832) and blockers(0.115049); the corresponding unretained values are0.285370 and0.241344. Small heterogeneous strata are descriptive, not separately powered significance tests. The historical archive predates the trample correction and is not globally unseen model ancestry.

The public native evaluator4ec59ee8 was frozen throughout. Both new correct128 checkpoints and the previously completed semantic128 checkpoint loaded under their actual schemas, original32 predecessors and full Adam state. All teacher logits/value bits matched their training reports; g115 matched the earlier reader. Fresh replay was byte-identical; duplicated endpoints, wrong age, changed moments and altered label identity were rejected. The reserved panel was interpreted only after complete counts and fresh tactical replay. The original analyzer was pinned before training and used unchanged.

Kimi's replica2 qualification initially reserved both PCs. After its completion, actual controller39436 and selected local-d-w10 benchmark showed both replica arms on the maintainer GPU1, leaving compute host eligible. The new execution-only runner bound that controller's PID, creation time, command and actual allocation. Serial/four/eight-worker development scoring gave45.917/17.353/14.679 seconds including recovery, with identical signatures. The guarded launcher selected eight concurrent jobs. No new paid allocation; authenticated RunPod inventory returned HTTP403. GPUs were inventoried but this evaluator is CPU-only.

Actual formal archive native wall20.219seconds, output transfer/recovery included wall24.522seconds; staging8.297seconds; fresh tactical panel2.853seconds and replay2.871seconds. Their sum38.543seconds excludes engineering, qualification and owner-inventory overhead. The archive-plus-staging forecast39.922seconds compared with actual32.819seconds. Read-only telemetry sampled107.828 CPU-seconds across the10 native jobs, approximately5.33 occupied cores averaged over the20.219-second group; peak per-process RSS was250,871,808 bytes. Fewer than eight cores on average reflects job lengths and the final tail, not eight continuously occupied cores. No two60-second idle-capacity windows occurred. All recovered result hashes matched; the compute host's final worker inventory was empty.

Authoritative root: `E:/mtg-meta-recovery-20260921/equal-budget-evaluation-001`, especially `completion.json`, `analysis.json`, `compute-choice.json`, `engineering/completion.json`, per-job `execution.json` and `owners-after.json`. Transport: `equal-budget-evaluation-remote-v2.py`; preserved failed preflight: `equal-budget-evaluation-preflight-recovery.json`. No frozen source/input/gate changed during transport recovery.

Decision: advance the retained128 lineage to integration engineering and a separately designed current-engine whole-match comparison. Do not promote it or replace the human preview. The explicit inference adapter242b2c47 is implemented and syntax-parsed, but not compiled or exercised; whole-package identity, legal-action mapping and matched-seed replay remain required. Keep sideboard/opening policies fixed across candidate and control because updated embeddings may invalidate a learned sideboard head. Human competitiveness, broad meta strength and league readiness remain unproven.

Independent Fable review is still absent under the recorded zero-read HTTP429 until September22 at07:00EDT. Proceeding under the maintainer's research assignment with that review gap explicit, not as endorsement. CP7 outcomes were excluded from selection. No new games or training updates occurred in this evaluation.
