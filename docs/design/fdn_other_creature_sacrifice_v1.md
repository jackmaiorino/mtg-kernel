# FDN sacrifice another creature

Hungry Ghoul is a 2/2 Zombie for {1}{B}. Its {1}, sacrifice another creature
activation puts one +1/+1 counter on that same source incarnation. The pinned
[XMage implementation](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/h/HungryGhoul.java)
supplies the printed contract. This is source preparation for issue #110's
serial v68 batch, tentatively ID 342 after v67's mill/modal creatures.
Registry, profile pins and complete gameplay qualification remain pending.

`SacrificeOtherControlledCreatures` appends a cost component. It reuses the
existing incarnation-bound activation choice and atomic payment machinery,
with the source excluded at offer, selection, restore validation and payment.
Candidates use current control and effective creature type independently of
ownership. Tokens, tapped creatures and creatures with summoning sickness
qualify. Selected objects retain their zone and incarnation bindings.

The existing `AddPlusOneCounterToAbilitySource` effect refuses a departed or
returned source. Costs remain paid if the ability later loses its source.
The rules record preserves the exact count and exclusion. Its fixed feature
vocabulary reports source exclusion as opaque rather than omitting that fact.
No state field, existing cost identity, default card recipe or historical
catalog pin changes in this preparation.

Three primitive tests cover exclusion, control, atomic mana failure, counts,
duplicate rejection and candidate incarnation binding. Prepared integration
tests cover metadata, full payment, token choice, pending restore, borrowing
and old-source refusal. The three primitive tests passed with pinned Rust
1.94.1 on the default catalog at source `239db43b`, through the supported
BelowNormal two-core launcher (cores 16-17). The initial fixture-card lookup
failure is retained; the repaired fixture uses registered Faerie Miscreant.
The prepared gameplay cases still require serial card admission. This result
does not establish admitted card support or playing strength.
