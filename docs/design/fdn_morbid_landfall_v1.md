# FDN targeted creature trigger preparation

Tragic Banshee (FDN73) is a {4}{B} 5/3 Spirit. Its entry trigger targets an
opponent-controlled creature, applying -1/-1 through cleanup, or -13/-13
when a creature has died during the current turn. The existing conditional
effect evaluates morbid at resolution, including deaths after trigger placement.
Grappling Kraken (FDN39) is a {4}{U}{U} 5/6 Kraken. A controlled land entering
triggers a targeted tap followed by one stun counter, including an already
tapped target. Both use existing target contracts and effect primitives.

Primary references are the pinned XMage implementations of
[Tragic Banshee](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/t/TragicBanshee.java)
and [Grappling Kraken](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/g/GrapplingKraken.java).
Names and printings belong to the frozen full booster manifest. Definition-owned
trigger recipe strings bind the target and effect semantics when admitted.
Tentative IDs357-358 follow the retained Rise of the Dark Realms preparation.

Five unregistered card cases cover exact costs/payment rollback, both seats,
morbid timing/cleanup, source departure, target reentry/control changes,
opponent land exclusion, already-tapped targets, actual stun-consuming untap
and finalized-trigger restore. No registry, profile, accepted coverage or live
catalog identity changes. Native gameplay qualification remains pending.

Prerequisite source e2fe17c3601a4a0c00d21ea72f8b20b0520d9be6 integrates accepted
main5114e679 and preserves Standard enum positions, including target IDs55-56
and AnimateSource. The future permanent-graveyard target is now57. Separate
read-only integration review found no actionable defects. The existing Haley
queue targets that prerequisite source while the whole-host reservation holds;
these five additional card cases are not part of its queued source yet.

Source review identified three fixture errors before native qualification:
target selection had not finalized trigger placement, and direct assignments
to Cleanup/Untap skipped their entry actions. Fixtures now explicitly finalize
the targeted stack item before changing state or taking snapshots, and pass
through EndStep priority into real Cleanup and the opponent's Untap. Original
source c67ad5b6 is retained; these repairs remain pending native qualification.

Repair receipt, October 10: separate read-only review of
61c1c64a3018e004c6d04707314ca7b92537a918 confirmed all three findings closed
with no remaining actionable defects. Formatting/diff checks pass. Before
admission, the existing unstarted Haley queue was updated to this exact source,
preserving its guard, cache and reservation. It includes compilation of these
five card cases; unregistered card gameplay remains unexecuted.

The local native preflight at source867a79ae caught two API-name errors missed
by source review: Step::EndStep and state_hash_v4 do not exist. The fixture now
uses Step::End and diagnostic_state_hash, preserving real cleanup/untap and
full-state replay comparisons. Primitive and Duress checks passed; the failed
local-6 log remains retained and card gameplay qualification remains pending.
