# V4 whole-object sampler repair

2026-09-22. Engineering prerequisite, no strength claim or playing route. The old V1 sampler remains unchanged. The diagnostic V4 clone now calls a dedicated sampler that permutes complete ObjectIds among unknown owner-specific hand/library slots, preserving all per-object data except zone. Root-known hand/top slots and the root's own hand stay fixed. Eligible IDs and slots are canonical before seeded Fisher-Yates.

Same-incarnation historical references reject admission rather than pinning a card to its actual hidden position or retrying. The scan includes pending triggers/grants, stack/pending-effect sources, designation records, pending activation/land/cast/copy, linked exile, timed keywords and permissions. Conservative exclusions remain for unversioned replacement and temporary-set references. This enumeration is under fresh source review, not a completeness claim. Invalid clone errors abort search; there is no fallback or seed retry.

The other observer's hand/library knowledge follows slots under the explicit sampling convention. Hand entries map once from the original slot map, avoiding in-place permutation cascades. No calibrated posterior, sideboard inference, hidden-history equivalence or complete simulation validity is claimed. Retained object history can differ between observationally indistinguishable roots; same-seed byte equality is tested only for clones of the same root.

Evidence: original synthetic source-relabel failure retained in controls004 and key-controls001 archives. The repaired probe tests32fixed seeds, preserved historical definition, unchanged root tensor/output, legal direct stepping and a changed unknown library position. Additional controls test per-object data preservation, both seats, repeatability, canonical hidden-arrangement invariance, other-observer slot rebinding and same-incarnation rejection. v4-sampler-controls-001 is RUNNING through ownership-checked check-v4-sampler.py,4BelowNormaljobs,5pinnedsourcefiles. No pass claimed. Fresh high-effort Fable source review v4-sampler-source-review-001 is RUNNING, actual guidance/literature reads verified; no endorsement yet.

Remaining: inspect all test results/source hashes and actual reviewer findings, address material omissions, then qualify the search algorithm and powered whole-match measurement separately. g115 unchanged; M1 unmet.

## Fixed checks completed

v4-sampler-controls-001 completed exit0:20/20 passed,0failed,0.77s tests,276.971s full invocation. All5pinnedsource SHA256s matched after completion. This includes repaired key natural-terminal fixture, both-seat key/consume parity, identity/knowledge/repeatability/admission controls, and historical-source stepping across32fixedseeds with unknown position movement. Fixed synthetic checks only; no simulation campaign, model update or formal gate. Source review remains active; passing these fixtures does not establish exhaustive world validity or playing acceptance.

Controls002 completed before the review repairs:22/22passed,0failed,.77stest/275.475sinvocation,all5sourcehashesverified. Added bothseat successor-node consume/direct-step parity and legacy-relabel fault injection returning StepFailed with a halted clone. This does not address the four findings in v4_sampler_review_disposition_20260922.md.

F2/F3 edits now present but UNTESTED: pooled objects must match physical hidden-card baseline; residue rejects without normalization; old hidden-source helpers reset V4/control/tap/damage/counters/plot fields when moving into library; remove dead hand-knowledge remap and assert implicit self-hand invariant; positively test both library-knowledge owner rows. F1 pending-effect/transient-event scan and F4 cross-zone stepping remain open. No core caller or playing acceptance.

## Pending repair validation

F1 implementation added effect_refs.rs: typed recursive frame, choice purpose/candidates, answered-guard and transient-event scan; bound effect operations are handled, with exhaustive enumeration of remaining symbolic leaf operations. F4 adds sampler_tests.rs: engine-staged Preordain scry rejection versus Thought Scour mill admission, plus both-seat opponent-pool historical source sampling into hand/library and subsequent direct/consume parity. The latter counter-and-library-move fixture deliberately is not an ordinary-reachability proof. Both remain UNVALIDATED.

Controls003 terminated compile failure (37.458s, exit101): wrong MayExileFromPlayersGraveyardMatchingThen variant spelling. Preserved. Corrected spelling; added bound Undercity operation cases and exhaustive EffectOp matching. Controls004 is RUNNING with8pinnedsourcefiles,4BelowNormaljobs through check-v4-sampler-repair.py. Same-session independent follow-up v4-sampler-repair-review-001 active, actual changed-source reads verified; no repair acceptance yet. Old sampler and three Disabled guards remain unchanged.
