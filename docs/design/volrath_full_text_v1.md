# Volrath's Shapeshifter and ordered graveyards

This work supports the Full English Breakfast mainboard in #460. The
Shapeshifter is appended to the Eternal registry as Partial for scoped
native gameplay. The mainboard still fails the playable catalog check.

The guarded Rust 1.94.1 catalog generator at
`0051f21f1049d7befe9250ea7f8b6eb3f26779ee` emitted Volrath as Partial at
ID 1198 and catalog hash `b7272db8a877989e`. The generated source SHA-256 is
`ff3cfba884d3d0d3fa5a00f1286470ec2080f2574a12721324b5582d47c2600b`.
The historical generator receipt SHA-256 is
`25727e984f5d21006e6f6ce6d28ece47ce7ab7c8d77cebf9c43e5f66c6e8aae8`.
The accepted Elves integration preserves Birchlore Rangers and Thornscape
Battlemage at IDs 1198 and 1199, then appends the unchanged Partial Volrath
record. The combined catalog fingerprint still awaits guarded generation.

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
`premodern_ordered_public_v1::`, including the nested `feb::tests::` terminal
and Human-owner fixtures. CI uses the same complete module filter. The registered-card fixtures under
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

The exact `a8baba3df` native packet completed 61 of 62 checks successfully,
with no timeouts and all children drained. Its manifest is
`E:/codex-eternal-feb-a8ba-evidence/manifest.json`, SHA-256
`bbfe4e1b0dcaf1ed664ae0c94d5403fc188fdf31901c1cafbaddecc292b15561`.
The paid Fanatic prevention and donor-keyword repairs, both native history
guards, every-kind consumers, static audit and all retained producers passed.
The terminal fixture alone failed at starting seat P0, step 63: Cabal Therapy's
native naming choice had no supported Human label.

The source successor labels both naming stages only for the acting player,
the validated physical spell and the exact ordered options at its printed
program path. Copied spells and unrelated generic choices remain unsupported.
Its both-seat paid fixture checks native naming, reordered-menu refusal,
snapshot restore, stale tokens and the exact physical discard order selected
through sealed Human ordinals. Independent source review passed.

The exact successor `ea95de740b1158fc1c644720bab9294a9530bd49` completed
all 63 native checks with exit 0, no timeouts and all children drained.
Manifest: `E:/codex-eternal-feb-ea95-evidence/manifest.json`, SHA-256
`b726aecb7499fe873b0f33634228fd77757a2c60da436d37fbb08c15636fd47c`.
The unchanged frozen terminal/replay fixture passed for both starting seats,
as did paid Cabal naming and exact discard order, all retained donor and
producer cases, both history guards and every-kind consumers. Catalog
fingerprint for that packet is `b7272db8a877989e`. This establishes the scoped in-process
terminal and replay result. Transport, further donor compatibility, hosted
checks and default-branch integration remain open. Volrath remains Partial
and the FEB mainboard remains outside PLAYABLE.

The subsequent source adds actor-only Human labels for Unearth's exact printed
hand-cycling recipe and the original physical or donated Guide echo options.
Echo presentation uses an authenticated historical public source even after
the original permanent leaves. Only cloned view operands change; the native
decision, sealed token and ordinal still execute the original bound action.
Both-seat fixtures cover paid cycling, physical and donated echo, paid
Palinchron bounce above echo, donor departure, restore, stale tokens and
forged source or reordered-option refusal. Source review and pinned formatting
passed; these new fixtures have not run. The inherited engine's controller-
change sacrifice halt and differing physical/departed echo payment behavior
remain under rules review. Neither this source nor the preceding terminal
fixture establishes complete echo or transport qualification.
