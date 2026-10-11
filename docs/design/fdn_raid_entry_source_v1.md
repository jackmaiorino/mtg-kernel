# Raid entry source preparation

This owned source follows Raid history09284047 and eight-card candidate9dd66a06.
It implements the first two Raid rules without registering either card. Issue110
accepted coverage remains177Full/1Partial/108Missing until candidate delivery.
Pinned Mage a5c90fe180021e70e2a644ade00eeab07f857a40 supplies the definitions:
Skyship Buccaneer is3UU4/3 Human Pirate with Flying and conditional entry draw1;
Goblin Boarders is2R3/2 Goblin Pirate entering with one +1/+1 counter after Raid.

The appended Skyship trigger condition accepts only its own battlefield entry
and the declaration history of its controller at capture. Its conditional draw
rechecks the resolving context's captured controller. Current source control,
death and log drainage do not substitute another player's history. A stale turn
refuses the draw. The enums append their variants, preserving previous ordinals.
RulesMeaning emits a conditional controller creature event-this-turn read and
marks the exact declaration predicate Opaque.

Boarders uses the common entry-counter initialization hook, before entry trigger
collection and SBAs. Executable printed definitions and active printed abilities
gate the hook; it checks the entrant's actual controller declaration history.
It adds its counter alongside ordinary entry-counter initialization, with no
stack ability. The hook is available in Limited-only and combined builds.
RulesMeaning records the conditional entry rule and its permanent counter/stat
change in both configurations. The generator includes Skyship Flying and its capture/resolution recipe, plus
Boarders entry-static fingerprint. Both appear in the keyed source inventory.
The unregistered names contribute no bytes to the existing generated catalog;
registration must generate and qualify the successor identity.

Two primitive groups use registered Guttersnipe objects and actual engine
attacker declarations/entry events to check capture source, both controllers,
captured-controller draw, stale saved-game history, and empty declaration refusal.
These are source-level primitives. Native compile/test/lint remain unrun. Six registered-card groups are prepared for printed metadata, both-seat Raid
entry/draw, legal off-turn Flash refusal, saved pending source departure and
new-incarnation counters. These six groups remain unrun because the cards are
unregistered. Suppression still needs a separate entry-characteristics fixture
and registered gameplay verification. Goblin Boarders gameplay is not qualified by these primitives.

The separate Raid history guard qualifies09284047; this source is not in that
running or queued checkout. Public observation/tensor/Human disposition with
independent successor identities remains required before admitting Raid cards.
Frozen predecessor consumers continue omitting Raid history. Gorehorn and
Strongbox remain unimplemented. No coverage, model admission, experiment or
playing-strength claim follows from source preparation.
