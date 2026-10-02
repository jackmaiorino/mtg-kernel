# FDN entry counters and landfall

This slice appends Gnarlid Colony (186) and Mossborn Hydra (187). The
Limited registry becomes v38 `39c83779971ee2c4`; default v32 and every older
catalog literal remain fixed. Earlier profiles remain readable, while
publisher and resume mutations require the current identity.

| Card | Required behavior |
| --- | --- |
| Gnarlid Colony | The existing kicker decision adds `{2}{G}` to `{1}{G}`. A kicked spell enters with two counters before triggers or SBAs. An ordinary entry has no counters. Its static grant continuously gives trample to creatures its current controller controls with positive +1/+1 counters. Counter changes, control changes and source departure immediately change the grant. |
| Mossborn Hydra | Every entry starts with one counter before the 0/0 SBA. It has trample. Every controlled land entry creates a separate trigger, bound to the Hydra's exact incarnation. Each resolution adds its then-current number of counters. A departed or returned source is unaffected by an older trigger. |

Entry counters are definition metadata enforced in the shared zone-change
and token-entry paths. Kicker is consulted only for a resolving spell's
Stack-to-Battlefield move, preventing unrelated entries from inheriting it.
Positive entry placements and doubling emit an exact counter event for later
counter-trigger mechanics. Existing counter-effect paths will be connected to
that event as part of Exemplar of Light's implementation.

+1/+1 storage expands to checked i32: sixteen ordinary landfalls produce
65,536 counters. Hashing uses the original i16 representation whenever the
value fits, preserving historical state hashes. Overflow halts explicitly;
values are never wrapped or saturated.

Marked damage expands to checked u32 so large Hydra hits retain their actual
amount. Lethal comparisons and remaining-lethal calculations use i64
intermediates. Ordinary damage hashes retain the original u16 bytes.

The frozen flat source files and layouts remain byte-identical. Custom
schema-v2 observations expose larger counts through the optional
`engine_context.wide_plus_one_counters` list, keyed by exact permanent
reference. This list overrides the legacy card counter slot, which is zero
for a listed permanent. Other states omit the list. Schema-v1 projection
and native flat publication refuse values they cannot represent, before
publishing buffers. Effective power/toughness use the actual count.
The optional `engine_context.wide_marked_damage` list similarly overrides
the zero legacy damage slot for larger marked totals. A regression covers
a 65,536-damage hit, restoration, and the next point causing lethal damage.

Focused checks cover real casting and payment, pre-SBA entry, continuous
grants, one and multiple landfall events, resolution-time counts, bounce,
wide counters and serialization. Custom-session kicker restoration compares
the pending response, stable action identity and next environment binding.

Reference implementations are the pinned XMage `GnarlidColony.java` and
`MossbornHydra.java` at `a5c90fe180021e70e2a644ade00eeab07f857a40`.
Five matching XMage scenarios now pass at `2a0b7edfb59`; see the validation
report for their scope. The full fixture goal still requires Exemplar of
Light, Sun-Blessed Healer, remaining mechanic slices, mulligans, complete
original-deck games and the remaining rules comparisons.
