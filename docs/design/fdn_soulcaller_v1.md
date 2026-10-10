# FDN Vampire Soulcaller preparation

Vampire Soulcaller is a 3/2 Vampire Warlock for {4}{B}, with flying,
"This creature can't block", and a mandatory ETB ability returning target
creature card from its controller's graveyard to hand. The pinned
[XMage source](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/v/VampireSoulcaller.java)
defines the printed contract. Tentative serial admission is v69, ID343,
after the v63-v68 batches.

Reuse the existing ETB trigger, own-graveyard creature-card target filter,
exact target incarnation contract and `MoveAllTargets(Hand)` interpreter.
The shared printed cannot-block predicate also recognizes Soulcaller and
respects printed ability removal. The rules record retains the restriction
exactly and marks its fixed vocabulary's missing blocking predicate opaque.
No new engine state, effect/cost variant or historical profile pin is needed.

Five prepared regressions cover characteristics and payment, both players'
mandatory target filtering, pending-choice JSON restore, absence of legal
targets, stale target incarnations and a real blocker decision before and
after Witness Protection removes printed abilities. Registry, generated
catalog identity, profile, fixture and execution remain pending serial
admission. Source preparation does not establish supported card gameplay.
