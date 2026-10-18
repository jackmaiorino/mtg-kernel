# Elenda FDN119 Partial source receipt

Owned source branch `codex/fdn-elenda-source-20261015`, based on
`2cc01ba4fb1fc49010ed6a48347cf6c2233ee8f7`, for #110. The genuine printing is
now registered **Partial** in source. Runtime qualification, generated catalog
export, append-only profile composition and integration remain root-owned after
PR272 and Ravenous. No Full, model, consumer or fair-sampling admission is claimed.

Primary reference: Mage commit `a5c90fe180021e70e2a644ade00eeab07f857a40`.
SHA-256 hashes the exact Git blob bytes at that commit:

| Mage source path | SHA-256 |
| --- | --- |
| `Mage.Sets/src/mage/cards/e/ElendaSaintOfDusk.java` | `992ab2c2e1030a0172269211f362689934fcfe4f7ae24b7a181407d0b928a334` |
| `Mage/src/main/java/mage/abilities/keyword/HexproofFromInstantsAbility.java` | `58beba16b2d630ba5d37ee4f02988d62fde41cf5ef2d28d300f4f29fc6cbeb88` |
| `Mage/src/main/java/mage/abilities/condition/common/MoreThanStartingLifeTotalCondition.java` | `ab1c23fe9abbc57de0859ca76b80c4e982cb2af4399e8e2eb84270da2de6a0f7` |

Frozen `booster_pool_v1.json` identifies FDN119, Scryfall
`24955f5f-093c-4d33-b0c1-911cd36032ce`, oracle
`bfd614d5-4c7f-416b-9cb2-bb167c0893a0`: rare, legendary Vampire Knight,
`{2}{W}{B}`, 4/4, lifelink and hexproof from instants. This branch appends that
exact registry entry, a by-name reference deck and the lifelink recipe.
`build.rs` uses the ordinary executable permanent program and selects the two
typed descriptors only for this Limited printing. ID443 is provisional, derived
from the owned 192-card prefix plus 251 prior FDN rows. Root must compose and
renumber after the accepted parent/Ravenous. Tests resolve names, never ID443.
No catalog hash was guessed, generated export committed or frozen profile edited.

`starting_life_self_bonuses_v1` contains two additive conditions: +1/+1 and
menace at controller life at least starting life +1; an additional +5/+5 at +10.
The supported contract remains fixed `STARTING_LIFE = 20`.

| Controller life | P/T | Menace |
| --- | --- | --- |
| 20 | 4/4 | No |
| 21 or 29 | 5/5 | Yes |
| 30 | 10/10 | Yes |

Shared P/T and keyword hooks read current-controller life and printed ability
suppression on each query. `source_type_hexproof_v1` uses effective live types
or the selected Adventure/Omen/Bestow spell form, with copy form captured in
immutable provenance. Ordinary target enumeration and resolution revalidation,
plus `fixed_source_damage_v3`, `source_power_activation_v1` and
`gorehorn_damage_v1`, share opponent-only Instant protection. Printed names,
colors and generic hexproof cannot substitute for the source type.

Physical spell and battlefield departures append optional effective types and
an independent raw prebatch capsule to existing `DepartureColorV1`. Validation
reconstructs the original source plus transitive attachment/host/suppression,
turn, loyalty and crew inputs and recomputes effective types. Spell forms bind
original finalized-cast or copy authority. Valid-bit type-only edits refuse
without erasing original journals. Existing source authentication and trusted
occurrence assumptions remain intact. A retained ceased copy binds its frozen
original spell contract. Old enum ordinals and serialized field prefixes remain
fixed; absent projections omit JSON and Hash input. Default/Standard catalogs
retain absent descriptors and the three legacy copy paths keep Normal there.
Existing positive manual-copy fixtures now select correct catalog projections.

`fdn_elenda_v1.rs` supplies nine active acceptance tests using the real printing:
paid both-seat casts and paid-stack JSON restores at all four thresholds; exact
colored payment refusal; paid self-Bolt downward changes; own Instant and
opponent Sorcery targeting versus opponent Instant refusal; restored pending
Bolt revalidation after real flashed Involuntary Employment and Witness
Protection using a genuine High Fae Trickster source; paid Witness and Broken
Wings suppression/removal; actual three-turn unblocked lifelink combat across
both thresholds; menace single/two-blocker legality; and genuine opted-in
physical Bolt-departure type-only tamper negatives preserving original inputs.
Fixture life/mana and supporting battlefield permanents are seeded explicitly;
subsequent turns, control changes, damage and ability removal use real actions.
The by-name Partial preflight test requires Full-deck rejection.

Separate source tests cover all cast methods in absent/selected mode, legacy
zero JSON/Hash, genuine paid Fang Dragon/Sagu Wildling forms and copies after
parent departure, and raw historical replay including a real Zoetic Glyph
animation. Standard Questing Druid's Instant Adventure evidence remains separate.
The FDN pool contains no Instant Adventure dependency, so a joint genuine
Elenda/Instant-Adventure/copy gameplay case remains **pending**. No out-of-pool
printing, fabricated alias or ignored placeholder test supplies that case.

Observed local checks: pinned `cargo +1.94.1 fmt --all`, JSON/registry/protected
prefix static checks and `git diff --check`. **All Rust compilation, gameplay
tests and generated catalog execution remain unrun** under the no-heavy-local
constraint. Qualification and any runtime-discovered repairs remain necessary.
Frozen rules-vector extraction still refuses these descriptors. The separate
`rules_meaning_elenda_v52` successor authenticates ordered immutable printed
facts: lifelink, opponent Instant-source protection at selection and resolution,
then +1/+1 and menace at starting +1 and additional +5/+5 at +10. It retains
current-controller/current-world evaluation, printed-ability suppression, the
fixed supported starting life20, additive bonuses and effective selected/copy/
original-source type semantics. It exports no runtime source capsule, object,
private-world operand or executable index; `model_admitted` remains false.
V52 is reserved after root's Ravenous V51 integration. The standalone module
adds no shared dispatch variant, so no local ordinal collision is introduced.
Narrow source tests cover exact ordered facts, descriptor omission and altered
threshold/keyword/protection/admission refusal, closed schema, actual Limited
source forms and Default/Standard refusal. These Rust tests remain unrun.
Existing registry, provisional ID443 and profile/export identities are unchanged.
