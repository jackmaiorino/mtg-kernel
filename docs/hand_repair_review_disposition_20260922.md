# Independent review complete: measure conditional opportunity before repair

Fable reviewed the source and evidence, then completed a focused correction round. Session `481cd2ff-1618-406d-a243-b2bbab2d0bb1`; receipts `E:/mtg-meta-recovery-20260921/conditional-hand-repair-review-002` and `-003`, both status complete with nonempty reviews. The failed429 attempt001 is preserved. No experiment was run by the reviewer.

**Disposition: no intervention launch; prepare a fixed conditional continuation study.** The reviewer accepts the two natural available-win proofs as engine-relative lower bounds. Together with completed negative controls, the explicit independent-review requirement for those proofs is now satisfied. This is not strict action dominance, a strength finding, adoption of training labels, or M1.

| Material feedback | Lead disposition |
| --- | --- |
| Oracle lower-bound construction is sound; transparent auto-pass explains cell45's zero opponent-choice proof nodes | Accepted for these engine-relative certificates, with existing information/unsupported-action limits. |
| All non-face branches exhausted8,192 nodes | Corrected by reviewer: true for cell25, false for cell45's127/143-transition branches. Preserve abstentions; do not label them losses. |
| Two/eight roots over600 games impose a whole-match gain ceiling | Reviewer qualified: these are archive incidence ratios, not causal BO3 gain ceilings. Lead additionally rejects the remaining inference that gains must occur elsewhere: an observed win does not show that a chosen line has win probability1. |
| Value error and large returned L2 norms prove no lethal representation | Reviewer withdrew. L2 was not minimized; one value error does not diagnose missing features. Gamma1 equal-return explanation remains a hypothesis, not proof about historical advantages. |
| Relabel existing power as2pp-margin non-inferiority | Reviewer withdrew the recommendation and adequacy claim. No margin is authorized or changed; full non-regression design remains unresolved. |
| Seat-pair duplication may overstate game counts | Reviewer withdrew that inference after seeing different initial choosers. Completed full-record audit below finds no exact duplicate games under its stated normalization. |
| Approximate retention and held-out tactical coverage matter more than exact-score equality | Accepted as future design considerations, not a go signal. No new readout fit is needed before clarifying conditional benefit. |
| Explore chosen-action continuations on the two fixed roots before a repair | Accepted as the next design to prepare, subject to source-valid restart semantics, literature, reviewer/monitor scrutiny and guarded throughput qualification. |

## Duplication audit completed

All256 g115 matches and600 game records were hash-checked and compared. Remove only outer decision_index and observation step_index/physical_decision_id/substep_index/substep_count; preserve game start/index, all visible contents, behavior masses/selections, package identities and terminal. Result:600 distinct normalized game hashes, zero identical whole-match game sequences among128 seat pairs, zero shared full-game hashes between seat pairs. The previously identified cell45 visible-state duplicate remains a duplicate root, not a duplicate complete game. Unequal hashes do not establish statistical independence. No denominator, completed gate or result was changed.

Source `python/tools/audit_control_panel_duplicates_v1.py`; evidence `E:/mtg-meta-recovery-20260921/control-panel-duplicates-001.json`;91.413s read-only CPU analysis, no game simulation.

## Next design boundary

Estimate per-root win probability after the incumbent's selected action, conditional on the exact restored engine state, both archived policies and fresh sampler seeds. Hidden future library order stays fixed unless a separate justified estimand is declared; this is not a public-belief or whole-match estimate. Decode values: cell25+.8831638694, cell45-.3856699765. Neither value substitutes for rollouts.

Fable's suggested200 continuations/root is exploratory. Exact binomial planning provides a concrete draft: failure-to-win null<=1%, alternative5%, fixedn200; at two-root Bonferroni alpha.025 each, at least6 failures rejects the per-root null (actual null tail.0160229, power at5%=.9376575). Zero failures gives a one-sided97.5% upper bound1.8275%, not proof of zero regret. No early stopping, root replacement, failure dropping or search until significance. This is only a planning calculation, not a frozen or launched experiment. Independent training n=0; measured .76pp remains required for any later strength gate.

The complete restart contract, literature note and compute qualification must precede execution. No new rollout process, model update, paid compute or promotion has started. g115 remains the reference; M1 unmet, goal active.
