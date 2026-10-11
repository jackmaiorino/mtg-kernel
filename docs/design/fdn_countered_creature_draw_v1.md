# Countered creature draw and keyword grant

Inspiring Call is prepared without registration. Pinned Mage
`a5c90fe180021e70e2a644ade00eeab07f857a40` supplies its 2G instant definition:
draw once per controlled creature with a +1/+1 counter, then grant those
creatures indestructible until end of turn. Multiple counters count once.

The appended effect captures live Arena battlefield creatures under the
resolving controller with positive +1/+1 counters. It draws once per captured
object through ordinary draw events, then installs existing layer-6 keyword
effects for their exact incarnations. Later counter loss, control changes and
new eligible objects do not retarget the grant; blinking loses it. Ordinary
cleanup and later ability removal use the existing engine paths. Existing
enum ordinals, public fields, catalog membership and observation hashes are
preserved. The generated recipe records the shared snapshot; RulesMeaning
uses existing draw/keyword atoms and Opaque for the counter predicate and
shared binding absent from its fixed vocabulary.

One registered primitive case and five unregistered gameplay groups cover
counting, zero, response changes, restore, control changes, blinking, actual
destroy-all, cleanup and later Witness Protection. Formatting and diff checks
are preparation; compilation, actual games and catalog/profile acceptance
remain pending. No strength, search, evaluation or training result is claimed.
