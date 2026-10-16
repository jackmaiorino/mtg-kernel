# Imprisoned in the Moon native source, FDN156

Native Partial484, source-only. No Rust compilation or execution, Full promotion,
consumer, owner, model, Human, transport or deck-preflight admission is claimed.
The old292 registry records, frozen286 names and24 token dependencies retain
exact bytes/metadata; this appends only the real FDN156 printing.

Printed source: pinned XMage a5c90fe180021e70e2a644ade00eeab07f857a40,
`Mage.Sets/src/mage/cards/i/ImprisonedInTheMoon.java`: `{2}{U}`, Aura,
enchant creature, land or planeswalker. The enchantment does not tap its host
on entry. The host is a colorless land with `{T}: Add {C}` and loses all other
card types and abilities. Its name, supertypes, physical identity, controller,
incarnation, counters, damage and face are not changed by this effect.

Rules references: [CR](https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf)
205.1a/205.3d (types/subtypes),613.1d-f/613.7 (layers/timestamps),302.6
(tap symbol and creatures),704.5m (illegal Aura), and
[Foundations release notes](https://magic.wizards.com/en/news/feature/foundations-release-notes).
Pinned Mage explicitly applies type/land-subtype changes in4, colorless in5,
and ability removal plus the new colorless mana ability in6. It does not set P/T.

The new appended `AttachmentDef::AuraLandOverride` shares the native exact
bidirectional attachment/incarnation graph. `host_matches` preserves every old
attachment domain and adds the exact creature/land/planeswalker union for this
one descriptor, including return and copied-Aura initialization. The appended
Standard target filter uses existing full native target/source contracts and
resolution-time incarnation/fizzle checks. No global world field or new action,
decision or continuation schema is introduced.

`continuous_characteristics_v1.rs` selects actual active attached sources and
applies the new type/color/subtype/ability removals in their layers. Witness
Protection's type/color/ability settings and native animation operands keep
timestamps. Basic and other defined land subtypes remain; nonland subtypes do
not remain merely because they are stored on the physical incarnation. Moon
does not supply a P/T setting. Later layer-6 grants retain the existing removal
timestamp test. Both explicit mana actions and the existing automatic payer use
the current host/controller's distinct single-C grant, with one mana per tap and
ordinary effective-creature summoning-sickness checks. Original printed mana
colors, restrictions, amounts and costs are not substituted for that grant.
Removing an active Moon selects existing final-zone checked-PT planning, so a
wide hidden creature envelope cannot be exposed after a partial zone mutation.

Legacy shared observations explicitly refuse the new currently known printing;
unknown opposing hands/libraries and neutral faces do not select the refusal.
Partial deck preflight remains unchanged. All new executable behavior lives in
existing production files. The98 independent engine/effect/card/trigger source
arrays include each changed child body they previously omitted, with their
original path/flat framing, old ordered entries and duplicates retained.
No generated source identity is guessed. Existing world Hash/serde fields and
old enum prefixes are unchanged; only the two real descriptor/filter tails append.

Eight source groups in `engine::imprisoned_moon_v1_tests` cover:

- Real paid creature and Full Ajani plus genuine Forest land play, paid2U,
  exact stack contracts, physical fields, subtype/loyalty and restore.
- Paid Bolt response, original target departure and labelled native reentry,
  followed by native fizzle rather than attachment to a new incarnation.
- Paid Witness then Moon, illegal Witness SBA, paid Broken Wings departure,
  labelled native card return followed by an actual second paid Moon cast.
- Paid Self-Reflection before Moon, independent copiable source, and a labelled
  native later animation with a real later Witness cast and timestamp checks.
- Young creature becoming noncreature, genuine explicit C activation, floating-C
  payment, automatic C payment, control/untap operands and full restore parity.
- Native-width counters and neutral-face operands, preserved physical face and
  real Full Ajani ledger; checked refusal before a Moon departure exposes PT overflow.
- Independently initialized opposing hidden worlds, read-only malformed graph
  refusals and legacy observation refusal.
- Genuine paid Replication and Season, actual next-upkeep target, two separate
  native host answers, copied Aura batch and waiting/answered restore parity.

Resource placement, arbitrary counter/animation/face/control/zone-return operands
are labelled native primitives, not invented paid cards or printing capabilities.
Full runtime qualification and complete typed Moon consumer meanings remain future
work. Standard-only later mana grants are not newly certified by these pure-FDN
fixtures. No tests, fields, timeouts or existing acceptance assertions are relaxed.
