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
Empty descriptors add no semantic token to the catalog canonical input. Old
vector extraction refuses a future nonempty descriptor until a precise meaning
successor exists.

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

The continuation adds `CardDef::source_type_hexproof_v1`, selecting the typed
Instants rule only for Elenda. `source_type_hexproof_v1` reads effective live
types, the selected pending/finalized Adventure/Omen/Bestow form, or original
departure types. Form viability uses prospective types before announcement,
including cloned free-cast selections. Ordinary target enumeration, individual
effect guards and resolution revalidation share the rule. Opponents' sources
are restricted against the target's current controller, while ability removal
suppresses the printed rule. Recomputed public facts expose that typed rule.

Historical types append an optional projection to the existing independently
frozen `DepartureColorV1`, including physical spell departures. The existing
prebatch, SBA and sacrifice capture paths freeze types before any member moves.
`fixed_source_damage_v3`, `source_power_activation_v1` and `gorehorn_damage_v1`
use their authenticated original source contract to read this projection; their
original payment/publication/departure authorities remain intact. Missing,
duplicate or foreign type evidence refuses, without printed-type fallback.
The journals retain their existing trusted-occurrence assumption.

Copies append the optional projection to immutable `SpellCopyOriginV4` and
retain their actual alternative cast form. Validation compares the copy's form,
origin and parent live/departure types. A ceased copy retains its original
projection on its arena object; a departed/recast physical parent cannot replace
it. Empty projections omit JSON and contribute no Hash input. Existing enum
ordinals and all previous struct field prefixes remain fixed. New capture
emission is catalog-selected, so all current printings retain absent projections.

Additional test source covers typed predicates, exact legacy copy-origin
bytes/Hash, current-controller/suppression primitive inputs and foreign-projection
refusal. Genuine paid source-form cases use registered Fang Dragon/Sagu Wildling
and Standard Questing Druid, copy them, counter the parent and restore. Those
are source-form tests, not Elenda gameplay. The new opt-in emitters remain
inactive in the current unregistered catalog and require genuine-printing
qualification. All Rust cases and generated catalog exports remain **unrun**.

Pending genuine-printing acceptance: paid Elenda casts/restores at all four
thresholds and downward transitions; opponent instant refusal versus own
instant/opponent sorcery acceptance; paid pending-target restore followed by
control/ability changes; Instant Adventure versus noninstant form and copies
after departure; combat lifelink crossing thresholds, menace legality and
printed-ability suppression, and tampering with opt-in departure/copy type
projections. These remain documentation, not ignored tests or runtime passes.
Registration and exact-parent native qualification remain root-owned; no Partial
printing is fabricated here.
