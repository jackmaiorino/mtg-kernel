# Sphinx's Tutelage native Partial source

Frozen printing: SPG75, Scryfall `94a3673b-6003-4612-a8f6-02829aa9de7d`,
Oracle `158db438-52ba-4c30-b7bf-d1c4f76da917`. Membership remains the existing
frozen FDN booster target. Appends local definition466 as Partial, preserving
all preceding definitions and capability metadata. The combined registry now has
467 definitions/275 FDN rows; the frozen target reports245 Full,15 Partial,
26 Missing.

Implementation reference: pinned Mage
`a5c90fe180021e70e2a644ade00eeab07f857a40`,
`Mage.Sets/src/mage/cards/s/SphinxsTutelage.java:30-39,69-118`.
Its printed enchantment costs2U. Each successful controller draw creates one
mandatory target-opponent trigger. Resolution mills two cards together and
repeats only when that original pair consists of two nonland cards sharing at
least one color. Colorless cards do not share a color; multicolored cards need
one common color. The activated ability costs5U, requires no tap, and draws one
then discards one. Putting a card into hand without drawing does not trigger it.

Production semantics live in already explicitly hashed `event.rs`, `effect.rs`,
`engine.rs`, `trigger.rs`, and the existing public refusal/visitor sources.
There is no new production child file or inferred transitive identity input.
For example, `current_mixed_policy_v1.rs::feature_identity` already explicitly
frames each changed core input once; the V53 and V61 rules-meaning identities
also explicitly consume event/effect/engine/trigger source. The native mill
helpers and draw capture therefore change those real existing source bindings.
`build.rs` binds the ordinary permanent move, trigger recipe and actual paid
activation program into the existing generated definition identity. Earlier
enum variants are unchanged; new EffectOp, EffectFrame, target-purpose and
CommittedEvent variants append. No GameState field or absent-state wire/hash
change is introduced. A game with no actual Tutelage observer emits the old
Draw event alone.

Draw capture occurs at final native successful Draw commit, before subsequent
instructions can change the source's zone, controller, or effective abilities.
The private roster is adjacent to the original Draw history index. Collection
consumes it once, with no selection of targets until ordinary APNAP placement.
Published roots retain their original source/controller and target contracts.
They use native mutable-world structural validation, not authentication of
coordinated arbitrary edits to an entire raw GameState and its event history.

Resolution captures the original complete library privately. Each repeat is
reconstructed from the exact original pair, ordered native zone changes and
linked batch receipt. Its owner's graveyard-order prompt retains original
prefix order independently from the chosen permutation. No arbitrary loop cap
is used: each actual batch removes up to two cards from the original finite
library. Public generic begin/resume cannot create native pre-pop authority.
A consumed private owner preserves the original row and complete remaining
stack, including loan cleanup on both Ok and Err. Snapshot validation keeps the
ordinary full row/source/target/context checks and adds closed required stages.

This adds no model, Human, sealed owner, transport, Full, profile, sampling or
training permission. Existing public projections refuse visible Tutelage and
its private native history instead of dropping it into old semantics. Full-only
deck resolution refuses the new genuine forty-card reference deck.

Eight source fixture groups cover genuine paid2U casting and5U activation for both
seats, actual discard and mill-order restore/replay, empty and one-card libraries,
land/colorless/disjoint stops, multicolor and repeated pairs, original graveyard
order, a paid Broken Wings response, and readonly context/root/pair/history/
progress/full-library corruptions. Paid Deep Analysis exercises each draw and the
real two-trigger order; paid Counterspell preserves the original cast costs
and prevents a countered enchantment from granting its activation. Event-time control and zone scenes use
explicit native primitives and are labelled as such, not claimed paid spells.

All Rust is uncompiled and unrun in this source checkpoint. Pinned formatting,
Python registry/refusal checks (181 passed) and static prefix/diff audits are source checks,
not native qualification or a runtime pass.
