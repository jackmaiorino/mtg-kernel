# Elenda starting-life static prerequisite

Source-only preparation for #110 on base
`2cc01ba4fb1fc49010ed6a48347cf6c2233ee8f7`. Elenda remains **Missing**,
unregistered, unqualified and unavailable for Full decks. Root owns later
qualification and integration after PR272. No profile, catalog hash, model,
consumer or fair-sampling admission is supplied here.

Primary reference is Mage commit `a5c90fe180021e70e2a644ade00eeab07f857a40`.
SHA-256 below hashes the exact Git blob bytes at that commit:

| Mage source path | SHA-256 |
| --- | --- |
| `Mage.Sets/src/mage/cards/e/ElendaSaintOfDusk.java` | `992ab2c2e1030a0172269211f362689934fcfe4f7ae24b7a181407d0b928a334` |
| `Mage/src/main/java/mage/abilities/keyword/HexproofFromInstantsAbility.java` | `58beba16b2d630ba5d37ee4f02988d62fde41cf5ef2d28d300f4f29fc6cbeb88` |
| `Mage/src/main/java/mage/abilities/condition/common/MoreThanStartingLifeTotalCondition.java` | `ab1c23fe9abbc57de0859ca76b80c4e982cb2af4399e8e2eb84270da2de6a0f7` |

The frozen printing is FDN119, Scryfall `24955f5f-093c-4d33-b0c1-911cd36032ce`:
legendary Vampire Knight, `{2}{W}{B}`, 4/4, lifelink, hexproof from instants.
Mage's ONE and TEN conditions mean life at least starting life +1 and +10.

The implemented prerequisite appends typed
`CardDef::starting_life_self_bonuses_v1`. `build.rs` selects only Elenda in
Limited without Standard and emits two additive descriptors: +1/+1 and menace
at +1; an additional +5/+5 at +10. Every current definition receives `&[]`.
Empty descriptors add no semantic token to the catalog canonical input. No
existing enum or serialized state field changes. Old vector extraction refuses
a future nonempty descriptor until a precise meaning successor exists.

The common effective P/T and keyword queries read these descriptors without a
name dispatch. Live current-controller life, battlefield/front-face membership
and printed-ability suppression determine every query. Public typed facts retain
the exact rules and suppression state. The only supported starting life remains
`STARTING_LIFE = 20`. The resulting intended printed stats are:

| Controller life | P/T | Menace |
| --- | --- | --- |
| 20 | 4/4 | No |
| 21 or 29 | 5/5 | Yes |
| 30 | 10/10 | Yes |

Downward life changes and control changes recompute immediately; no cached
threshold or permanent buff is stored. Primitive test source covers boundaries,
signed extremes, both controller seats, restore, zone/face absence, unchanged
real Epicure stats and real paid Witness suppression/departure. Supplied rules
are explicit primitive inputs, never installed as a fabricated Elenda alias.
Rust compilation and these tests are **unrun**. Pinned Rust1.94.1 formatting and
the scoped static/diff checks are the only local verification.

Remaining prerequisite: authenticate effective source types, including selected
Instant Adventure/Omen spell forms and copies after departure. Thread that
authority through ordinary announcement and resolution revalidation, trigger
source contracts and the separate historical projections in
`fixed_source_damage_v3`, `source_power_activation_v1` and `gorehorn_damage_v1`.
The current `HistoricalDamageSourceV1` carries color/keywords/subtype facts,
not a complete effective-type capture. Printed names, printed colors, generic
HEXPROOF or a filter on only the ordinary enumeration path cannot replace it.

Pending genuine-printing acceptance: paid Elenda casts/restores at all four
thresholds and downward transitions; opponent instant refusal versus own
instant/opponent sorcery acceptance; paid pending-target restore followed by
control/ability changes; Instant Adventure versus noninstant form and copies
after departure; combat lifelink crossing thresholds, menace legality and
printed-ability suppression. These remain documentation, not ignored tests or
runtime passes. Registration requires the complete executable recipe and actual
generator provenance; no Partial printing is fabricated here.
