# Volrath's Shapeshifter and ordered graveyards

This work supports the Full English Breakfast mainboard in #460. The
Shapeshifter remains absent from the registry pending native qualification of
the foundation. The mainboard still fails the playable catalog check.

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
native ordinal and incarnation. Mixed graveyard and battlefield arrivals use
the selected final text before native entry capture. The legacy serialized
state omits empty producer and ordering records, and their empty hashes add
no bytes.

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
`premodern_ordered_public_v1::tests::`. Source fixtures exercise paid abilities
on an ordinary public host, both-seat order and hidden-world pairs, native
menu binding, and restored/tampered order state. Formatting and diff checks
do not establish native behavior. The new event kinds require genuine sample
harvest before the every-kind event-history checks can qualify.

After foundation qualification, the Partial registration needs real native
tests for no donor, another Shapeshifter on top, unsupported public donor
text, and a paid donated activation on the registered physical card. Qualify
ordinary copy, activation and frozen observation compatibility on the exact
delivery head. Mainboard playability still requires actual owning gameplay
and catalog evidence. This work grants no hidden-world search or research
admission.
