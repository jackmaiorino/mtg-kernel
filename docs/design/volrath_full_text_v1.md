# Volrath's Shapeshifter and ordered graveyards

This work supports the Full English Breakfast mainboard in #460. The
Shapeshifter is appended to the Eternal registry as Partial for scoped
native gameplay. The mainboard still fails the playable catalog check.

The guarded Rust 1.94.1 catalog generator at
`0051f21f1049d7befe9250ea7f8b6eb3f26779ee` emitted Volrath as Partial at
ID 1198 and catalog hash `b7272db8a877989e`. The generated source SHA-256 is
`ff3cfba884d3d0d3fa5a00f1286470ec2080f2574a12721324b5582d47c2600b`.
The generator receipt SHA-256 is
`25727e984f5d21006e6f6ce6d28ece47ce7ab7c8d77cebf9c43e5f66c6e8aae8`.

Full-text changes apply in layer 3 and copies use layer 1. The battlefield
object retains its physical definition. The public top card in its current
controller's graveyard supplies text only while that card is a creature.
Removing abilities in layer 6 leaves layer-3 characteristics in place. A paid
activation or collected trigger uses the original host and donor incarnations
even if the top changes. See rules 612.6, 707.2, 113.7a and 404.3 in the
[November 6, 2026 Comprehensive Rules](https://media.wizards.com/2026/downloads/MagicCompRules%2020261106.txt).

The Oracle text was cross-checked against XMage's
`Mage.Sets/src/mage/cards/v/VolrathsShapeshifter.java` at
`a5c90fe180021e70e2a644ade00eeab07f857a40` (source SHA-256
`f30f8378ea118a47bb9beac0320147938b9b1caad4f211defa460f0f194a7ebf`).
The retained `{2}` ability discards on resolution. The native producer keeps
its cost independent of the donor's capability and retains its original
action slot while donated abilities occupy distinct slots.

The native reset selects owner-ordered graveyard batches independently of
deck contents. Effects and SBA deaths retain separate native
continuations. The owner orders actual post-replacement arrivals by exact
native ordinal and incarnation. A battlefield entry in a batch that changes
a graveyard uses its final text after both arrivals and departures. The legacy
serialized state omits empty producer and ordering records, and their empty
hashes add no bytes.

The ordered public view exposes both graveyards bottom to top. While an order
is pending, it separates unplaced public arrivals from the established order
and exposes the menu only to the chooser. The numeric decision owner uses
native priority windows and existing guarded common facts after that boundary.
It binds regular actions to their original ordinal and stable ID. The Human
view uses public names and typed full-text meanings, and binds its selections
through the same private native menu. Older observation and search profiles
retain their refusal.

## Qualification

Use `--features eternal-fixtures --lib` for
`graveyard_order_v1::tests::`, `full_text_v1::tests::`, and
`premodern_ordered_public_v1::tests::`. The registered-card fixtures under
`full_text_v1::tests::registered` exercise paid casts and abilities, both-seat
order and native menu binding, and restored state. The four native sample
producers are the mixed entry batch, Palinchron-text cast, donated Wall of
Roots mana activation, and Dack Fayden's paid control transfer. The mixed
entry fixture drives a native batch with a real owner order and an actual
battlefield entry. Formatting and diff checks do not establish native
behavior. The new event kinds require genuine sample harvest before the
every-kind event-history checks can qualify.

The fixtures also cover no donor, another Shapeshifter on top, unsupported
public donor text, and the printed discard cost. The legacy guard uses actual
history events even when the event-kind index is stale or forced open. Donated
entry triggers check native suppression after all members of a simultaneous
batch have entered or left. The fixtures cover paid Palinchron and Karmic Guide
text with Torpor Orb, and an Orb entering or departing in the same batch.
A separate fixture covers a departing top donor in both batch member orders.

Donor support is finite. Mogg Fanatic's donated activation uses the physical
host's actual sacrifice and captures its damage characteristics before the
host leaves. Ravenous Rats uses the native targeted discard program with the
existing immediate and final-batch entry suppression rules. Wall of Blossoms'
donated draw and Academy Rector's donated death trigger remain outside this
set. Mainboard qualification also requires terminal play and replay evidence
from the actual snapshot list. Ordinary copy compatibility needs exact-head
gameplay evidence.

The in-process `premodern_ordered_public_v1::feb` owner resets both seats
from the frozen 60-card mainboard in snapshot row order and uses native
London mulligans and priority. Numeric views use local reference aliases.
The Human view includes the actor's native menu and an explicit unsupported
row for action meanings outside the current renderer. Same-owner snapshots
restore the pending menu and invalidate old decision tokens. The owner is
an in-process milestone. Transport and complete consumer qualification remain
open.

The terminal fixture uses the frozen mirror at seed `20261108` with each
starting seat and the catalog harness's 65,536 physical-action cap. The
public-menu policy keeps the dealt hand, plays lands and spells, and chooses
nonempty attacks and blocks. It prefers the last legal nonempty attack group
and otherwise uses the first native ordinal to break ties. It passes before
optional activations. An unsupported decision or Human action meaning
fails the fixture. A fresh reset must replay the recorded actor, menu and
native ordinal sequence to the same natural result, state hash and event
history. The fixture also restores a same-owner cast boundary and checks
that its original token is stale.

Run the focused acceptance fixture with the pinned toolchain and
`eternal-fixtures` feature.

```text
cargo test --release -p mtg-kernel --lib --features eternal-fixtures -- premodern_ordered_public_v1::feb::tests::frozen_feb_mirrors_reach_native_terminal_and_replay_both_starting_seats --exact
```

This is an in-process determinism and rules check. Transport and other
donated programs remain separate from this fixture.
