# Ghalta and Elvish Archdruid source preparation

Both names remain unregistered and add no accepted coverage. The pinned Mage
constructors at a5c90fe180021e70e2a644ade00eeab07f857a40 define Ghalta as
10GG, legendary Elder Dinosaur, 12/12 trample, with a generic discount equal
to the total power of controlled creatures. Archdruid is 1GG, Elf Druid,
2/2, other controlled Elf creatures get +1/+1, and tapping adds green for
each controlled Elf permanent, including itself.

Ghalta uses an appended dynamic-cost query. It scans live controlled effective
creatures, sums signed effective power in i64, and floors the total at zero.
The existing complete cost quote preserves colored pips and freezes the selected
cost before payment. Elder is appended without renumbering existing subtype
ids; the Limited creature-type list includes it for changeling. Rules facets
mark the missing sum-of-power vocabulary as opaque.

Archdruid reuses the live subtype-lord path and the rich TapSelf mana ability
with ControlledPermanentsWithSubtype(Elf). Mana activation remains explicit,
as for Priest of Titania; no generic solver admission or yield clamp is added.
The rich activation path evaluates exact representable output and refuses a
pool-capacity overflow before costs or ability-use mutations. The primary
action offers apply the same refusal.

Six unregistered fixture groups cover printed metadata, actual signed-power
casts and colored payment, restored results, lord stacking/control/zone changes,
nine-Elf mana production and the 255-mana boundary, effective noncreature Elf
counts, and actual Witness Protection suppression/removal. A registered-card
primitive test covers signed power, control, live zones and restore. Formatting
and diff checks pass. Native compilation, gameplay, catalog/version/profile
admission, CI, review and integration remain pending. The subtype extension
belongs to a future catalog identity, and cannot be used as accepted v67.

Read-only source and fixture review at63efd00c found no actionable issues.
Composition54fd3f06 preserves the reviewed Brine, ferocious, global flash and
power/Elf primitives, including effective-Haste admission and recursive bound
trigger validation. The staged six-name admission is reviewed but unexecuted.
It requires actual accepted v67 ancestry and exact registry equality before
writing candidate IDs371-376, version68 or coverage177.
