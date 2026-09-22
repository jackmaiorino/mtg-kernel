# Interior bonus diagnostic result

2026-09-22. Source5210eb2e. Completed fixed mechanism diagnostic, NO-ADVANCE as a two-fixture tactical candidate. No strength gate or training. M1 unmet, g115 unchanged. B=E:/mtg-meta-recovery-20260921.

Controls B/v4-core-controls-005:33/33pass,0.75stest,277.8080346sinvocation; all12sourcepins unchanged. B/v4-interior-build-001 and v4-interior-replay-001 complete; script check-v4-interior-bonus.py, analysis summarize-v4-interior.py -> interior-summary.json. Each of the two original arms exactly matches every archived outcome field from allocation-replay-001. Within-process witnessed repeats, fresh-process byte repeats, source/policy preservation and full collection off/on equality all pass. All trace scores and final node totals independently reconstructed, both root actors handled by sign. No playing override.

| Root/interior (RR root) | S | T | Selected/maxmean | Root0 wins | Root0 mean | Best other mean |
|---|---:|---:|---|---:|---:|---:|
| cell25 prior-weighted |64|152|0/0|3|8621|8404|
| cell25 prior-free |64|150|0/0|1|8426|8382|
| cell45 prior-weighted |128|338|3/3|0|-3761|-166|
| cell45 prior-free |128|308|3/3|1|-2926|-603|

All RR root actions16visits. Frozen S16N,T8S,depth8,seed20260922, same checkpoint/sampler/key/value/mean backup and prior-ordered unvisited expansion. Actual transitions differ. Natural wins by root action: burnnew[1,0,0,0],handnew[1,0,0,0,0,0,0,0]; no natural losses/draws. Root0new work burn37T/15forwards,hand38T/15forwards, each14expanded+1coverage+1natural. These are search terminal counts, not match win rates.

Cell45 prediction confirmed: node5 prior-free selects cast0 at ordinal68 with six scores[-490,-940,-1429,-1628,-650,-2350] and identical predecision edge sums. Later node5 actions at68,76,84,92,100,108,116,124 are[0,0,0,4,0,0,0,0]. Exact certified line[0,0,0] reaches natural rootwin at ordinal108. Cell25 line[0,6] reaches natural rootwin at ordinal9. Predicted zero-or-one handroot0win and unchanged final choices3/0 supported. Unlike the previous comparison, a win was now discovered under handroot0 and still not selected: its average remains below competing action3. This is a measured finite-budget mean-backup limitation, not a proof that arbitrary max backup or sampled-world proof propagation is valid.

Disposition: bonus asymmetry causally prevented this terminal discovery at the declared budget, but removing it is insufficient to resolve both fixtures. Preserve negative final selection; no budget sweep or rerun. Both original games were wins, so even a tactical selection repair would not establish whole-match non-regression. Review this new result before choosing a backup/playing-route change. The standalone target-enumeration probe is no longer necessary to establish that this sampled path can consume a natural winning target; only implement it if a distinct unresolved semantics question requires it.
