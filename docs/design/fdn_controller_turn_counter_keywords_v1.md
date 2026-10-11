# Controller-turn first strike from Inspiring Paladin

Source preparation only. Inspiring Paladin remains unregistered and its actual
gameplay fixtures are unrun. Pinned Mage `a5c90fe180021e70e2a644ade00eeab07f857a40`,
`InspiringPaladin.java`, defines 2W, 3/3 Human Knight. During its controller's
turn it has first strike, and controlled creatures with positive +1/+1 counters
have first strike. These are two continuous abilities, not queued triggers.

The common keyword query recomputes the self grant and the controller-turn gate
on the existing controlled-counter keyword grant. It uses current controllers,
effective creature types, source battlefield/front-face status, live printed
abilities and the existing layer-six timestamp check for recipients. Counters
and controller changes take effect immediately. Existing Gnarlid Colony's
unconditional controlled-counter trample grant keeps its predicate.

Printed first strike stays absent: the conditional grant is recorded in the
generated recipe and RulesMeaningH. Rules facts represent conditional keyword
grants and disclose the turn/counter vocabulary gaps as opaque predicates.
No enum, observation layout, tensor layout, RNG or catalog identity changes
occur before registration. The existing effective-keyword observation and
combat paths consume the same query as target legality and damage.

Qualification must exercise both seats, actual casts and cost refusal,
positive versus absent counters, controlled versus opposing creatures,
noncreatures, controller changes, source departure, turn transition,
first-strike damage, actual ability-removing Auras and timestamp order,
with saved-game restoration. Registration, native qualification, CI and
default acceptance remain separate pending work. No coverage or strength
claim is made by this preparation.
