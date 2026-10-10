# FDN Pilfer preparation

Pilfer is a sorcery for {1}{B}: target opponent reveals their hand; its
caster chooses one nonland card, and that opponent discards it. The pinned
[XMage source](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/p/Pilfer.java)
uses the nonland filter, including creatures. Tentative admission is v76,
ID350, after v63-v75.

Append `RevealTargetHandChooseNonlandDiscard` and reuse Duress's public
hand reveal, exact hand/incarnation bindings, mandatory controller choice
and authenticated post-answer discard. Derive eligibility from the
validated definition-owned leaf at both pending-choice and answer-frame
validation; no mutable filter field or existing continuation wire change
is added. Duress retains its original noncreature/nonland filter. The
rules facets record public reveal and one-card hand-to-graveyard choice;
the exact nonland-card restriction is opaque in the fixed vocabulary.

One primitive and five card cases are prepared for old/new eligibility,
both seats, creature/spell choice, invalid land/self-target refusal,
public hand identities, pending/answered restore, empty/land-only hands,
stale hand membership, changed incarnations and the owner-controlled Madness
offer for both Pilfer and Duress. They have not executed.
Registration, fixture, catalog/profile and gameplay qualification await
serial admission; accepted coverage is unchanged.

Read-only review of `dfd0b600` found that the inherited direct graveyard move
bypassed Fiery Temper's Madness replacement and that a whole-observation text
search did not prove revelation of excluded land cards. Extract and reuse
the existing engine discard commit, preserving ordinary Madness trigger
placement and the old continuation wire. The repaired reveal case compares
exact hand rows, including identities/incarnations, in the caster's opponent
knowledge and the opponent's own hand. Native execution remains pending.

Repair review of `355256638c0f529aaa94e95a872249416a7282db` confirmed both
findings resolved, existing discard order and delayed Madness trigger
placement preserved, and no remaining actionable findings. Diff check
passed; the reviewer did not execute tests.
