# Fake Your Own Death paid-printing candidate

This isolated candidate appends the real printing with `Partial` capability.
It does not add Full coverage or permit a fully supported deck preflight. The
canonical PR266 registration and accepted issue110 counts are unchanged.

The first actual native build rejected empty deck coverage before generation.
The registry now names a separate synthetic 40-card reference containing the
Partial printing. It leaves the original WG/UG fixtures unchanged and supplies
the provenance required by the existing generator. The reference remains
ineligible for fully supported deck preflight because Fake is Partial; it does
not establish gameplay, model, sampler or Full admission.

The source is pinned Mage commit `a5c90fe180021e70e2a644ade00eeab07f857a40`,
`Mage.Sets/src/mage/cards/f/FakeYourOwnDeath.java`, inspected directly. It is
a black instant costing `{1}{B}`, targeting any creature. The temporary power
bonus and gained death ability last until end of turn. The death ability returns
the original card tapped under its owner and creates Treasure for its controller
at death. Distinct grants remain distinct triggers; a failed return still creates
Treasure. The existing strict selector accepts executable Partial and Full
capabilities with the same exact printing recipe, costs and target shape.

Three genuine-printing fixture groups pay with two actual Swamps, cast the real
registered card and finalize its target. They cover both seats and a target
controlled by either seat, restore the actual published gained trigger, compare
complete resulting states, assert the new tapped owner-controlled incarnation
and independent Treasure beneficiary, and distinguish countering the paid spell
from countering its gained ability. A real acquired removal loan also checks
capture/collection erasure before any journal mutation. These groups are
uncompiled and unrun until the next qualified native attempt.

The candidate registry expects 251 FDN records and 443 combined definitions,
including one additional Partial printing. Actual generated count, printing ID,
catalog hash and frozen successor profile await the native generator export;
no hash or ID is guessed. Existing profile literals and full-coverage assertions
are retained. A new live profile cannot be published before its actual identity
is observed and reconciled.

Remaining Full work includes returning animated Auras with an untargeted legal
host choice, entry-choice restore/coexistence, every other departure path,
complete V50 public/backend/Human/meaning/model consumers, sampling and fair
search/training integration, and current-head qualification. This intermediate
registration enables real paid correctness checks while those requirements remain
open. No training, paid compute or model admission is authorized by it.

The returning animated Banner checkpoint now authenticates its enclosing
Step/Advance, original resolution input and complete pre-pop stack, then
reconstructs the existing entry checkpoint before admitting exactly the restored
original publication row. Countered and completed resolutions still require
absence. A genuine paid Banner/Fake fixture checks both seats, serialization at
the color choice, answer completion, one Treasure, expired animation and refusal
of changed, duplicated, absent or ownerless rows. Its animation is the existing
captured-target primitive operand, not a paid Tough Cookie printing. Compilation
and runtime remain pending.


The returning-Aura candidate is now implemented separately in
`fdn_fake_aura_return_candidate_v50.md`: zero/one/many untargeted host selection,
tapped entry through the existing attachment batch, private actual relation loan,
checkpoint replay and explicit no-host closure. Its paid fixture is written;
compilation, runtime and complete-consumer qualification remain pending. The
printing remains Partial and current PR266442 coverage is unchanged.

A sixth genuine paid Fake fixture separates physical owner from death controller:
an opponent-owned Elf starts under the actor through the actual entry-controller
constructor, receives the paid grant, dies and restores. Return belongs to the
owner; Treasure belongs to the captured death controller. The initial entry is a
fixture operand, not a claim of a paid control-changing printing. Runtime remains
pending.

Four additional genuine paid fixture groups now cover a target leaving or
departing and reentering before spell resolution, the captured card leaving the
graveyard in response to the gained ability, copied-token death, and expiration
through the real cleanup phase. Each compares a complete serialized/restored
execution. They distinguish a fizzle with no grant from a failed return with
Treasure, preserve token identity, and require both the power modifier and death
ability to expire. Response and token-copy initialization use existing engine
operands, not additional paid-printing claims. Together with the previous six
groups these remain uncompiled and unrun. The isolated source includes the
published bfb immutable-cache changes; Native125 still uses its exact original
published source and reservation.
