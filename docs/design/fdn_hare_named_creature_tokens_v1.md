# Named creatures excluding a captured source incarnation

Hare Apparent is prepared without registration. Pinned Mage
`a5c90fe180021e70e2a644ade00eeab07f857a40` defines a 1W 2/2 Rabbit Noble
with an untargeted entry trigger. At resolution, it creates one 1/1 white
Rabbit token per other controlled creature named Hare Apparent. Limited deck
loading already permits more than four copies. RabbitToken.java defines the
white 1/1 Rabbit dependency, frozen identity
`63ae4f9c-ad6e-4d6a-89f1-653916bf5373`; it remains unregistered.

The appended effect samples the authoritative Arena's battlefield creatures
under the captured trigger controller, using effective names. It excludes
the captured source object and generation together. If that source leaves
and returns, the new incarnation can count for the older trigger. Source
departure, control changes and ability removal preserve an already created
trigger. The count is sampled once before ordinary token entry events.
Missing source contracts refuse execution. RulesMeaning emits the existing
creature count and dynamic token atoms, includes the token dependency and
marks the name/source-incarnation predicates Opaque. Existing enum ordinals,
state fields, observations and catalog membership are preserved.

Reference discrepancy: Mage's filter is FilterControlledPermanent with
AnotherPredicate and NamePredicate, but no creature predicate. Its text and
the [frozen printing's Oracle record](https://api.scryfall.com/cards/9fc6f0e9-eb5f-4bc0-b3d7-756644b66d12)
both specify creatures. The engine implements that restriction. This is an
identified reference gap, not a claim of parity for a hypothetical named
noncreature. Oracle text was fetched successfully on October11; frozen
membership and printing identities are unchanged.

Two registered primitive cases and five unregistered gameplay groups cover
both seats, zero/counting, current names/types/controller, response changes,
source departure and blinking, actual entry triggers, saved-game replay and
forged roots. Formatting/diff checks are preparation; compilation, gameplay
and catalog/profile admission remain pending. No strength or training claim.
