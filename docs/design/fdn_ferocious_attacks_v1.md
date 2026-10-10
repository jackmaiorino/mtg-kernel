# Ferocious attack source preparation

Pinned Mage constructors at a5c90fe180021e70e2a644ade00eeab07f857a40:
`Mage.Sets/src/mage/cards/r/RubyDaringTracker.java` and
`Mage.Sets/src/mage/cards/c/CourageousGoblin.java`.
Ruby is a legendary RG 1/2 Human Scout with haste and tap for R or G.
Courageous Goblin is a 1R 2/2 Goblin. On attacking while their controller
controls a creature with effective power at least four, Ruby gets +2/+2 and
Goblin gets +1/+0 and menace until end of turn. The source itself can qualify.

The appended attack predicate checks the event's source incarnation and
controller, then current battlefield control, creature type and effective
power. This is a trigger-time condition. Removing the qualifying creature
before resolution does not cancel the effect. Ruby reuses the bound-source
temporary boost. Goblin combines that boost with the existing keyword grant
guarded by SourceStillInTriggerZone; the captured ability source contract
must keep both effects attached to the original battlefield incarnation.
Changing control preserves the pending effect. Departure and return cancels
its application to the returned object. Cleanup expires both effects.
Source review found that restored nested boosts were matched to their printed
recipe without authenticating the bound object. The existing stack validator
now checks bound-source leaves recursively through sequences and conditionals;
the original source contract supplies object, zone and generation. A regression
rejects redirection, zone and generation changes in both nesting shapes.

Ruby's generated haste mapping is added. Its printed mana colors will be
registered through the existing primary mana ability generator. Both precise
trigger recipes enter generated catalog fingerprints only upon registration.
Rules facets describe a conditional attack with an opaque marker for the
missing controlled-creature effective-power relationship. No new public
observation field or effect opcode is introduced.

Two trigger primitive tests cover both seats, the four-power threshold, opposing and
source qualification, current control, event generation, qualifying creature
departure, source control change, restored execution and stale returned sources.
They remain uncompiled and unexecuted. Actual card games must also verify
attack declarations and no-trigger cases, haste and both Ruby mana choices,
colored payment, menace blocking, cleanup, ability removal and restore.
Card registration, live identity/profile migration, CI and default integration
remain pending. Neither name adds accepted coverage.
