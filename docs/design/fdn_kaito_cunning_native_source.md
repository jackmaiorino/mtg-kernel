# Kaito, Cunning Infiltrator native source

Issue110 implementation in progress. Kaito494/V75 and its Ninja495 dependency
are reserved and unregistered. This branch is isolated from canonical PR308.
No runtime, Full, consumer, model or sampler admission is claimed.

## Printed contract

Primary source is local Mage commit a5c90fe, read without changing that checkout:
`Mage.Sets/src/mage/cards/k/KaitoCunningInfiltrator.java`,
`Mage/src/main/java/mage/game/command/emblems/KaitoCunningInfiltratorEmblem.java`
and `Mage/src/main/java/mage/game/permanent/token/NinjaToken2.java`.
Kaito is legendary, costs1UU, has subtype Kaito and starts at3 loyalty.

- Each controlled creature dealing combat damage to a player triggers one
  loyalty counter on the original Kaito incarnation. Capture the source and
  damage-controller facts at the damage boundary, including simultaneous deaths.
- +1 targets zero or one controlled creature. Its exact incarnation cannot be
  blocked this turn, then the controller draws one and discards one. With zero
  targets, draw and discard still occur. If the sole target becomes illegal,
  the entire ability fails to resolve under the ordinary target rules.
- -2 creates a2/1 blue Ninja creature token. The frozen24 dependencies already
  include Ninja UUID aeec04b1-475c-4e55-b72f-327ea5258146. Implement that existing
  dependency without editing the frozen pool.
- -9 creates a persistent emblem controlled by the ability controller. Each
  player casting a spell triggers a Ninja for each emblem. Spell copies are
  not casts; countering the spell does not remove the emblem trigger.

The existing Standard Kaito emblem boosts Ninjas and has a distinct effect.
Keep the new emblem in its own native journal. Reuse the shared exact-incarnation
end-of-turn evasion representation as a rule change, rather than a removable
creature keyword. Loyalty payment and resulting stack rows must retain their
original source/controller/cost/target bindings after source departure.

## Work and verification

The first prerequisite appends the native optional controlled-creature target
filter. It preserves the old Standard target prefix, retains max1/min0 and the
controller relation, and leaves the frozen rules-vector facets opaque for the
new domain. No existing printing selects it yet.

The unregistered native source now implements independent pre-cost payment and
exact full-row publication, lexical execution and consumed continuation owners,
pre-batch combat observer capture authenticated by the actual combat-wave finish,
source-independent persistent emblem capture at actual casts, and a distinct
private Ninja batch permit. The +1 uses the existing exact-incarnation evasion
rule, native draw and actual bound discard. Independent original discard input
and departure records precede its uninterrupted native resume. The current
private event log is checked against the complete original capture inventory.
Raw token creation, public effect execution and prior consumers still refuse.
The existing closed native-copy permit additionally accepts this exact Ninja
blueprint for real Self-Reflection, Electroduplicate and Replication ancestry.

The shared semantic source digest preserves its original ordered inputs and
appends the actual Kaito child once. All changed production source is included
in that digest. Existing event/effect prefixes and the237 sample bytes remain
unchanged; one appended kind is pending a genuine native sample.

Remaining work before registration is the genuine paid both-seat fixture set
covering all branches, source departure, target loss, restore, copy/Season,
independent hidden worlds and read-only forgery rejection. This source-only
checkpoint is not runtime qualification or complete consumer admission.
Register494/495 only after actual490 through493 exist. Add complete V75 owning
consumers in their own coherent follow-up before any consumer admission.

Pinned formatting and source checks are allowed locally. Rust build/check/test
must use the authorized remote/CI path; the active Native213 source and target
stay unchanged until original termination, all actual output custody and release.
