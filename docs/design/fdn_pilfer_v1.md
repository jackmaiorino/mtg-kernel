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

One primitive and four card cases are prepared for old/new eligibility,
both seats, creature/spell choice, invalid land/self-target refusal,
public hand identities, pending/answered restore, empty/land-only hands,
stale hand membership and changed incarnations. They have not executed.
Registration, fixture, catalog/profile and gameplay qualification await
serial admission; accepted coverage is unchanged.