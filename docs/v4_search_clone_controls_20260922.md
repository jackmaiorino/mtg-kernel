# V4 clone engineering, 2026-09-22

The additive implementation is in `rl_session/flat_action_v4/search_state.rs`, under the V4 encoder so it can reuse its frozen-trigger semantic normalization. The old V2 clone and its guards are unchanged. There is no playing caller, node key or search consume integration yet.

Controls-001 failed compilation (exit101, 41.042 seconds): missing GameState import produced two missing-type errors and a downstream closure-inference error. Evidence `E:/mtg-meta-recovery-20260921/v4-search-clone-controls-001` is preserved. Added the import, exact serialized sampled-state equality for repeat seeds, and explicit legacy/terminal rejection controls. Controls-002 is running; no acceptance claim.

The tests exercise both actors and perspective-scoped known hand/library locks, changed hidden-state hashes, identical repeats, tensor/output bit invariance, hidden-trigger sources, explicit library-search exclusion, and visible corruption rejection. Fixed synthetic positions only, no game outcomes, training or statistical gate. The separate evaluator correction passed8/8controls006 and was committed50051665.

Fable source review is active at `v4-clone-source-review-001`, session `774344c2-35a3-497d-b551-136050e1791e`, actual reads verified. Complete feedback and test results remain required before calling this clone accepted. This seam does not establish a calibrated belief distribution or search playing strength.

Review update: completed exit0/error-free with source reads. Verdict proceed; normalization, lock semantics and V3 cache-error handling accepted. Two requested strengthenings are accepted before key/step work: include the scorer's own FlatDecisionV4 in the compared boundary (its V7 extensions are independent of the V6 observation), and assert per-owner hand-plus-library card-definition multiset preservation. These are pending, so the current boundary is not final acceptance. The same-seed serialized-state comparison was already added while review ran.

Step-design caveats carried forward: opponent self-knowledge follows the existing determinization convention, not a calibrated posterior; a pending trigger can later consult its relabeled live source. Root observation equality alone does not establish correct simulation of those states. Review the live-source dependencies before admitting hidden-trigger stepping, or declare that context unsupported and report it. No change to engine rules or the certificate whitelist is authorized by this review.
