# Slumbering Cerberus prepared source v1

Base: a1e4f547d95e8a3c58d572866d00ef4911322956. Owned source only;
the registry, IDs, generated catalogs, profiles, models and frozen receipts
are not changed. The frozen booster_pool_v1.json and card_names.json contain
Slumbering Cerberus, Foundations 94. The selected base+FDN registry has no
record, and the base source/design search found no existing preparation.

## Printed authority

Pinned Mage a5c90fe180021e70e2a644ade00eeab07f857a40:
Mage.Sets/src/mage/cards/s/SlumberingCerberus.java. This is 1R, red 4/2 Dog,
with a controller-untap-step restriction and a mandatory beginning-of-each-
end-step Morbid untap. Mage uses DontUntapInControllersUntapStepSourceEffect,
BeginningOfEndStepTriggeredAbility(TargetController.ANY), MorbidCondition,
and UntapSourceEffect. The Gatherer page
https://gatherer.wizards.com/FDN/en-us/94/slumbering-cerberus returned HTTP403
during this preparation; its text was not claimed as fetched. The pinned
primary executable source is the rule authority used here.

## Implementation

Reuse the accepted actual creature-death turn ledger and
EffectCond::CreatureDiedThisTurn. Append BeginningAnyEndStepIfCreatureDied to
the existing trigger enum; capture is gated by the genuine BeginningEndStep
event's death flag and the effect independently rechecks the current round
and active seat at resolution. Neither controller's ownership of the dead
creature nor ownership of the ending turn is required. A death after the
beginning of an end step does not retroactively create a trigger.

Append StandardOpV1::UntapCapturedSourceV1. The common production stack
validator still owns the printed root. This leaf verifies its captured
source/controller/owner/BF/history contract before attempting any mutation;
then it acts only on the same source BF incarnation. Historical accepted
definition changes use the existing source validator. Current control or
ability removal does not erase a captured trigger. A departed or reentered
source is not untapped by the old trigger. The existing attempt_untap applies
stun replacement, consumes one stun only on an actual untap attempt, and does
not consume a separate skip-next-untap-step marker.

The untap step checks a pure definition-owned restriction for the actual
executable Cerberus definition while its printed abilities are active. It is
scoped to Limited without Standard and called only after the existing
controller-oriented BF enumeration and skip marker handling. It does not
interfere with resolving untap effects. No matching definition is present
in Default or Standard, and their helper returns false without mutation.
Witness removes the restriction and suppresses later trigger capture; an
already captured ability remains executable. The existing attachment-only
step restrictions and stun behavior are retained.

build.rs carries a precise canonical text recipe for eventual actual
registration. Ordinary 1R creature spell generation already supplies the
spell's permanent entry. Dog already belongs to the accepted subtype enum;
no subtype or targeting ID is appended. The prepared JSON is explicitly
outside generator input and gives no card ID or invented catalog hash.

## Fixtures and remaining qualification

Five registered typed-primitive groups use actual Llanowar Elves operands:
captured stun/replay, control/reentry, malformed source authority, absent
restriction readonly behavior, and genuine Morbid death plus stale-seat
resolution. These directly test existing typed internal mechanics with
captured authority, not a synthetic printed Cerberus definition.

Ten future printed integration groups require genuine root-owned Cerberus
registration. They use actual casting/announcement completion, both starting
seats, both end-step players and death owners, no/late death, stun and real
untap transitions, captured control/departure/reentry, actual Witness
responses with registered High Fae Trickster, restore/source forgery refusal,
private-hand observation, token/noncreature/discard/exile boundaries and
cleanup/turn reset. Control changes and stale-condition removal are labeled
typed fixture setups rather than invented printed spell programs. No root
validation bypass or test admission hook is added. Until serial registration
these printed fixtures intentionally fail missing-definition lookup.

Frozen RulesVector v1 retains the typed new trigger/op payload and marks it
Opaque; it does not claim the missing static restriction or complete printed
meaning. Frozen Human effect description refuses the new leaf. Public
ordinary tapped/BF state remains representable, but public policy history
does not provide a precise current-turn creature-death input. A fresh precise
meaning must bind each-end timing, intervening death capture/recheck, captured
source and BF/ZCC, active printed untap-step restriction and stun replacement.
A complete public/Human successor must transport the public death predicate
and labeled trigger/root authority, including duplicate/large trigger ordering,
without widening predecessor source/schema/catalog identities. Model/vector/
profile admission remains root-owned and is not provided by this source.

Only pinned rustfmt, JSON parsing and git diff checks are allowed here.
No native/typecheck/gameplay/sampler/model qualification has run.
