# FDN mill and modal creatures, source preparation

This batch prepares Billowing Shriekmass and Apothecary Stomper for issue #110,
following the retained v62-v66 batches. Tentative admission is v67, IDs340-341.
Neither card is registered yet; no supported gameplay coverage is claimed.

Printed behavior was read from pinned XMage
`a5c90fe180021e70e2a644ade00eeab07f857a40`:

| Card | Printed behavior | Primary source |
| --- | --- | --- |
| Billowing Shriekmass | {3}{B}, 2/3 Spirit, flying; ETB mill three; threshold +2/+1 | [XMage](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/b/BillowingShriekmass.java) |
| Apothecary Stomper | {4}{G}{G}, 4/4 Elephant, vigilance; ETB choose two counters on a controlled creature or gain four life | [XMage](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/a/ApothecaryStomper.java) |

Shriekmass uses the existing private multi-card mill continuation. Its
threshold reads the current controller's graveyard on each effective P/T
query, after base-setting effects, and stops applying when printed abilities
are removed. Existing name bindings are unchanged. The rules vector reads
the shared self-boost table and marks its engine predicate opaque.

Stomper reuses placement-time modal trigger choice before targets. Only the
counter mode targets, and its target has the existing controlled-creature
incarnation contract. Both modes use existing effects. Elephant appends to
the subtype enum; its changeling expansion is enabled only in the FDN build.
Generated semantic recipes describe only these future names.

Eight unexecuted gameplay tests cover metadata, threshold transitions in both
seats, controller changes, ability suppression, private mill continuation
restore, short and empty libraries, both modal branches, pending mode restore,
opponent target rejection and a blinked target. Noncards in the graveyard
do not satisfy threshold, and the static boost
operates only on the battlefield. Selected modal branches have explicit
definition-owned provenance and target-spec admission.
Registration, checksum, store-profile admission and native gameplay qualification remain pending
the serial predecessors. Formatting and diff checks are source checks only.
