# Raid declaration history source preparation

Issue110 still targets all286 frozen Foundations booster names. This source
preparation follows eight-card candidate f029ec46 and preserves the earlier
raid design26fff4e0. No Raid card is registered or qualified here.

`GameState.attack_declaration_turn_v1` stores the round and active seat of a
successful nonempty declaration. The engine writes it after declaration
validation and before trigger collection, independent of source abilities.
The public Rust query requires both timestamp components and the requesting
player to match. Failed declarations are read-only; an empty declaration
neither creates nor erases history. Tapped-and-attacking token creation does
not write declaration history. The field survives controller changes,
departures, drained events, combat reset and additional combats. Cleanup
retains the current turn's fact; actual Untap clears it. Using the active seat
with the round distinguishes both turns, including a P1-starting game.

The optional field is omitted and contributes no hash bytes when absent.
Historical snapshots missing it load with None. Recorded history deliberately
changes the privileged state hash and survives serialization/replay. This
does not claim byte identity for newly recorded post-declaration states or
authorize changes to frozen measurements or their pinned executables.

Five engine primitive test groups are prepared for valid/duplicate/invalid/
empty declarations, actual tapped-and-attacking token creation, control
changes/death/event draining/extra combat, Cleanup and actual round transitions
for both starting seats, and omitted-field/recorded-state replay. Formatting
and diff checks pass. Native compilation and all five groups remain unrun.
Source qualification will also check affected engine/state cases in both
Limited and combined configurations using guarded cores and shared-target
serialization. It will not count as new-card gameplay qualification.

Before admitting Raid cards, give this public fact an explicit observation,
tensor and Human consumer disposition with independent successor identities.
Frozen predecessor consumers currently omit it. Engine-only history does not
establish their completeness for Raid. Preserve the complete executable menu
and snapshot bindings in those successors, without deriving history from
surviving attackers or source-specific event logs.

The earlier design's first cards are Skyship Buccaneer and Goblin Boarders.
Pinned Mage a5c90fe180021e70e2a644ade00eeab07f857a40 is the attributed input.
Skyship needs conditional capture and resolution using its captured controller;
Boarders needs a replacement counter before entry triggers and SBAs, respecting
ability removal. Gorehorn's damage needs source LKI; Strongbox needs an exact
one-of-two exile permission choice. They remain separate unimplemented work.
Actual card groups, generated catalog/profile acceptance, native/Python parity,
reference comparisons and current-head delivery remain pending. This source
adds no playing-strength, model admission or experiment claim.
