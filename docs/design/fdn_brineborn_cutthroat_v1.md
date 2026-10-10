# Brineborn Cutthroat source preparation

Pinned Mage constructor a5c90fe180021e70e2a644ade00eeab07f857a40:
`Mage.Sets/src/mage/cards/b/BrinebornCutthroat.java`. The card is a
1U 2/1 Merfolk Pirate with Flash. Whenever its controller casts any spell
during an opponent's turn, put a +1/+1 counter on this permanent.

The appended trigger condition checks the caster and active player when the
cast event is collected. It is not an intervening-if condition. The existing
bound-source counter recipe captures the battlefield incarnation when the
trigger is created. Its resolution skips a departed or returned incarnation;
changing the source's controller, removing abilities or changing turns does
not cancel an existing trigger. Creatures and noncreature spells both count;
copies and land plays do not emit a spell-cast event. The battlefield gate
prevents the source from observing its own cast while on the stack.

Code generation binds Flash and the precise trigger recipe into the generated
catalog identity. Rules facets describe a controller cast and a conditional
gate, with an opaque marker for the missing opponent-turn relationship.
No new public observation field, effect opcode or subtype is required.

Two primitive regressions cover both seats, spell types, turn/caster matrices,
serialized-state equivalence, changed control and stale source incarnations.
Native13 at b42370ef passed both primitive tests in the Limited configuration.
The combined configuration is still running under supported guard
0fd098388a7449f597dbf38080d96709 on Haley cores12-13 BelowNormal with pinned
Rust1.94.1/MSVC19.50.35725. No combined or terminal result is claimed yet.
Four additional card fixture groups cover metadata and colored Flash payment,
the two-seat caster/turn/spell-type matrix, departure and return, and ability
removal before trigger resolution. Each restores and compares serialized states.
They remain unexecuted because the card is unregistered. Read-only review found
no actionable findings; formatting and diff checks pass.
Required remaining checks are native primitive execution,
actual registered card casting and restore/ability-removal games, metadata and
live profile migration, CI and default integration. The card remains
unregistered and adds no accepted coverage. Serial admission follows the
prepared Sprite/Archmage batch after canonical PR213 integration.
