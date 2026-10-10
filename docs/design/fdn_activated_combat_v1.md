# FDN activated combat abilities v1

Issue #110 batch claim: Shivan Dragon, Axgard Cavalry and Rogue's Passage,
append IDs 327-329 after v60. The coordinator reserves catalog v61. The full
booster target remains 286 names; registration coverage becomes 130 full,
one partial and 155 missing. Hosted compilation emitted v61 checksum
`949beb8c995c006c`; affected gameplay/profile checks remain pending, so this
document makes no gameplay-completion claim yet. The first final-profile
focused Linux run executed ten new cases: seven passed and three expiry cases
failed because the test helper assigned Cleanup directly, skipping its entry
action. The repaired helper passes the real End window into Cleanup and asserts
the next turn plus cleared temporary effects. Required hosted rerun is pending;
the failed [job 113996712270](https://github.com/jackmaiorino/mtg-kernel/actions/runs/37981912915/job/113996712270)
remains retained. This test-only repair does not alter the generated checksum.

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

The v60 native-store profile retains its frozen literals and remains readable;
publication/resume refuse it against the new live catalog. The independent
v61 profile is pinned to the generated checksum from [hosted build job
113990960984](https://github.com/jackmaiorino/mtg-kernel/actions/runs/37980736036/job/113990960984).
All four store/science classifiers include the new profile. No local
Cargo/native check, training, paid run or reservation takeover is used while
Stage4a reserves the desktop.
