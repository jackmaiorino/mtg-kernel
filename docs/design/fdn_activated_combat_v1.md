# FDN activated combat abilities v1

Issue #110 batch claim: Shivan Dragon, Axgard Cavalry and Rogue's Passage,
append IDs 327-329 after v60. The coordinator reserves catalog v61. The full
booster target remains 286 names; registration coverage becomes 130 full,
one partial and 155 missing. Native qualification and the frozen v61 checksum
are pending hosted CI; this document makes no gameplay-completion claim yet.

| Card | Printed activation | Existing interpreter machinery |
| --- | --- | --- |
| Shivan Dragon ({4}{R}{R}, 5/5 flying Dragon) | {R}: +1/+0 until end of turn | Fixed-value pump using `TargetRef::ThisSource`; check the activation's exact source incarnation before installing the temporary effect |
| Axgard Cavalry ({1}{R}, 2/2 Dwarf Berserker) | {T}: target creature gains haste until end of turn | Normal creature targeting, tap cost and temporary keyword |
| Rogue's Passage (land) | {T}: add {C}; {4}, {T}: target creature cannot be blocked this turn | Intrinsic generated mana ability plus independent mana/tap activation and temporary keyword |

The two new build recipes emit existing `EffectOp` variants. No effect
discriminants, snapshot schema or target choice shape changes. The narrow
source-pump guard requires the captured `ability_source_contract`, the
battlefield zone and matching zone-change count. An ability whose source
left and returned resolves without boosting the new incarnation. Targeted
pumps retain their existing target contract behavior.

Oracle characteristics were checked against the retained Scryfall printing
metadata used in the product manifest. XMage source references are attached
to registry rows. A synthetic 40-card fixture supports deterministic deck
resolution. The focused tests cover red-only repeated source activations,
tap and summoning-sickness restrictions, both players' creature targets,
invalid targets, exact source/target incarnation changes, expiry, real haste
attack eligibility, unblockability, mana ability separation, and pending
target/stack snapshot restore.

The v60 native-store profile must remain readable and fail publication or
resume against the new live catalog. A v61 profile will be pinned only after
the generated checksum is captured from hosted compilation. No local
Cargo/native check, training, paid run or reservation takeover is used while
Stage4a reserves the desktop.
