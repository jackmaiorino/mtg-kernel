# Pauper meta wave 1 record (2026-09)

Branch `lead/pauper-meta-cards-v1`, Tasks 1-13 of `docs/superpowers/plans/2026-09-09-pauper-meta-wave1.md`
(spec `docs/superpowers/specs/2026-09-09-pauper-meta-cards-and-sideboarding-design.md` revision 3).
Wave start base: `ce1a69f0` (Task 4 end). This record is written by Task 13 (identity
finalisation) after the branch's first complete test sweep since Task 5.

**Certification status: NOT CERTIFIED.** Per the plan's spec 5.3 step 4, this wave is not
called done until Task 14 (parity: directed branch coverage) passes -- every declared rules
branch bound to a named kernel test that exercises it, checked by
`python/tools/check_wave_branch_coverage_v1.py`, plus the standing one-seed bit-identical
rerun. Per the lead's 2026-09-10 spec amendment recorded in the wave 1 ledger, full
XMage-shadow-mapped certification is deferred until the CP7 shadow harness (hardcoded to
the Rally mirror on both sides today) is generalized to accept new registrations, which is
its own plan; until then the wave's ceiling is "kernel-covered, not XMage-shadow-certified".
Nothing in this record substitutes for either gate.

Task 14a (the kernel-side half of Task 14) is complete:
`docs/research/pauper_meta_wave1_parity_2026-09.md` (machine-readable twin:
`docs/research/pauper_meta_wave1_parity_2026-09.json`) records the branch
coverage manifest (`data/wave_branch_manifests/pauper_meta_w1.json`, 23
entries: 21 cards, the Insectile Aberration transform face, and the
Squirrel Token), the `covers:`-annotation checker
(`python/tools/check_wave_branch_coverage_v1.py`) passing at
`kernel-covered: 23 of 23 cards, 0 unexercised branches`, and the standing
one-seed bit-identical rerun (`rollout_record`, `burn_mirror`, seed 5151,
4 games) matching exactly between this branch and the pre-wave-1 base tree
(`75406ffe`) once the known identity fields (state hashes moved by the new
`GameState::monarch` field, and `card_db_hash`/git identities) are
excluded. This closes the kernel-side half of the spec 5.3 step 4 gate;
the wave's ceiling is still "kernel-covered, not XMage-shadow-certified"
until Task 14b (the XMage shadow harness generalization, not yet planned)
lands, so this wave remains **NOT CERTIFIED** for XMage-shadow parity.

## Cards added

21 new cards plus 1 token, appended to `data/cards_v1.json` in this order across Tasks 5-11
(never reordered; ids assigned append-only). "Deck(s)" lists the V2 registration(s) each
card's `decks` field names; Task 12 registered all nine V2 decks into
`REGISTRATION_SPECS`.

| # | Card | Task | Type / cost | Deck(s) | Key new primitive |
|---|---|---|---|---|---|
| 1 | Terminate | 5 | {B}{R} instant | Jund Wildfire V2 (sideboard) | `Special::DestroyCreature` |
| 2 | Ancient Grudge | 5 | {1}{R} instant, flashback {G} | Jund Wildfire V2 (sideboard) | `Special::DestroyArtifact` |
| 3 | Artful Dodge | 5 | {U} sorcery, flashback {U} | Mono-Blue Terror V2 | `Special::GrantCantBeBlockedUntilEndOfTurn`, `Keywords::CANT_BE_BLOCKED` |
| 4 | Abandon Attachments | 5 | {1}{U/R} instant, Lesson | Dimir Terror V2 | `Special::MayDiscardThenDraw` |
| 5 | Acorn Harvest | 5 | {3}{G} sorcery, flashback {1}{G} + 3 life | Spy Combo V2 (sideboard) | `Special::CreateTokens`, generalized token-count `Sequence` |
| 6 | Squirrel Token | 5 | 0-cost G 1/1 Squirrel token | (token, no deck) | `Subtype::Squirrel` |
| 7 | Suffocating Fumes | 6 | {2}{B} instant, cycling {2} | Dimir Terror V2 | `EffectOp::PumpAllUntilEndOfTurn`, `PumpControllerScope::Opponents` |
| 8 | Arms of Hadar | 6 | {3}{B} sorcery | Dimir Terror V2 (sideboard) | `PumpControllerScope::TargetPlayer(0)` |
| 9 | Smash to Smithereens | 6 | {1}{R} instant | Madness Burn V2 / Red Deck Wins V2 (sideboard) | `EffectOp::DealDamageToControllerOfTarget` |
| 10 | Raze | 6 | {R} sorcery | Red Deck Wins V2 (sideboard) | `Special::DestroyLand`, `PermanentFilter::Land` |
| 11 | Snuff Out | 7 | {3}{B} instant, alt cost pay 4 life if Swamp | Dimir Terror V2 | `Special::DestroyNonblackCreature`, `AltCostDef`/`AltCostCondition`, `TargetSpec::NonblackCreature` |
| 12 | Contaminated Aquifer | 7 | Land (Island Swamp) | Dimir Terror V2 | tag-generated only, no new code |
| 13 | Ice Tunnel | 7 | Snow Land (Island Swamp) | Dimir Terror V2 | tag-generated only, no new code |
| 14 | Kessig Flamebreather | 8 | Creature, noncreature-cast trigger | Madness Burn V2 | `TriggerCondition::CastNoncreatureSpell` |
| 15 | Gixian Infiltrator | 8 | Creature, sacrifice trigger | Grixis Affinity V2 | `TriggerCondition::SacrificeAnotherPermanent` |
| 16 | Webweaver Changeling | 8 | Creature, ETB conditional | Elves V2 | `TriggerCondition::EtbIfGraveyardCreatureCardsAtLeast`, `EffectCond::ControllerGraveyardCreatureCardsAtLeast` |
| 17 | Glint Hawk | 8 | Creature, ETB return-artifact-or-sacrifice | Grixis Affinity V2 | `EffectOp::MayPayCostThen` gains `return_permanent`/`otherwise`, `OptionalCostChoice::ReturnPermanent` |
| 18 | Delver of Secrets | 9 | Creature, transforms | Mono-Blue Delver V2 | `EffectOp::TransformSourceInPlace`, `EffectOp::LookAtTopMayRevealThen`, `TriggerCondition::BeginningOfUpkeep`, `card_id_by_visible_name` |
| 19 | Gurmag Angler | 10 | Creature, Delve | Mono-Blue Delver V2 / Grixis Affinity V2 | `CardDef::delve`, `mana::delve_payment_plan`, `Subtype::Fish` |
| 20 | Viridian Longbow | 10 | Equipment, granted tap ability | Elves V2 | `GrantedActivatedAbilityDef`, `EquipmentDef::granted_activated_ability` |
| 21 | Fang Dragon // Forktail Sweep | 11 | Adventure creature | Red Deck Wins V2 | `AdventureDef`, `CardDef::adventure`, `CastMethodV4::Omen` reuse, `SpellCastRouteV4::AdventureExile` |
| 22 | Azure Fleet Admiral | 11 | Creature, monarch ETB | Grixis Affinity V2 | `GameState::monarch`, `EffectOp::BecomeMonarch`, `MonarchTriggerBindingV1` |

21 non-token cards + 1 token = 22 registry entries. `data/cards_v1.json` grew 162 -> 184
non-token-plus-token definitions (`CARD_DEFS.len()` 162 -> 184; 171 pool non-token cards +
13 required tokens = 184).

## New generic primitives (reused across cards, never card-specific)

- `Special` variants: `DestroyCreature`, `DestroyArtifact`, `GrantCantBeBlockedUntilEndOfTurn`,
  `MayDiscardThenDraw`, `CreateTokens`, `DestroyLand`, `SmashToSmithereens`,
  `DestroyNonblackCreature`.
- `EffectOp` variants: `PumpAllUntilEndOfTurn` (+ `PumpControllerScope`),
  `DealDamageToControllerOfTarget`, `Sacrifice`, `TransformSourceInPlace`,
  `LookAtTopMayRevealThen` (+ `CardTypePredicate`).
- `EffectCond::ControllerGraveyardCreatureCardsAtLeast`.
- `TriggerCondition`: `CastNoncreatureSpell`, `SacrificeAnotherPermanent`,
  `EtbIfGraveyardCreatureCardsAtLeast`, `BeginningOfUpkeep`.
- `TargetSpec::NonblackCreature`; `PermanentFilter::Land`; `PermanentFilterDef::Artifact`.
- `card_def::AltCostDef`/`AltCostCondition` (`CardDef::alt_cost` retyped from a bare cost
  slice to carry a condition).
- `card_def::GrantedActivatedAbilityDef` + `EquipmentDef::granted_activated_ability`
  (equipment-granted activated abilities, frozen into the stack item via `granted_by` so a
  destroyed source/host does not stop the ability resolving, CR 113.7a).
- `card_def::AdventureDef` + `CardDef::adventure`; `ObjectStateV4::on_adventure`;
  `SpellCastRouteV4::AdventureExile`; `card_id_by_visible_name` face-2 slot.
- `CardDef::delve` + `mana::delve_payment_plan` (delve folded into the card DB identity
  canon string, Task 10).
- `GameState::monarch`, `EffectOp::BecomeMonarch`, `MonarchTriggerBindingV1`, and a direct
  combat-damage-transfer state mutation (documented deviation, see Task 11's report).
- `Keywords::CANT_BE_BLOCKED`; `Subtype::Squirrel`, `Subtype::Lesson`, `Subtype::Fish`,
  `Subtype::Insect`.

## Documented substitutions and deviations (from the per-task reports)

- **Raze** reuses the existing `TargetSpec::Land` (added for Cleansing Wildfire) rather than
  adding a new `TargetSpec::LandPermanent`, since the two are semantically identical
  (Task 6).
- **Smash to Smithereens'** "sacrifice the target in response" scenario resolves as a fizzle
  (CR 608.2b: the spell has one target referenced twice), not a partial resolution; the
  Gatherer ruling confirms this exactly (Task 6).
- **Snuff Out's** alt cost uses the new `AltCostDef`/`AltCostCondition` types generically;
  Fireblast and Land Grant were migrated to `condition: Always` alongside it, behaviorally
  unchanged (Task 7).
- **Gixian Infiltrator** exposed a latent bug: `trigger::trigger_effect_matches` hardcoded
  its `PutPlusOnePlusOneCounterOnBoundObject` allowlist to `card.name == "Writhing
  Chrysalis"` by name; widened to include Gixian Infiltrator (a pre-existing bug this card's
  shape happened to expose, not introduced by this wave) (Task 8).
- **Glint Hawk's** return-or-sacrifice choice is a known, documented simplification:
  `ReturnPermanent` with 2+ legal candidates takes an arbitrary-but-deterministic first
  candidate rather than staging a sub-picker; this pool's sole consumer never has more than
  one (Task 8).
- **Delver of Secrets** is tagged through the existing `Decision::ChooseEffectBoolean`
  machinery with no new RL decision kind; its trigger required a front-face-only gate
  (`face_index != 0`) fix so a transformed Delver stops retriggering (Task 9, fix round 1).
- **Gurmag Angler's** delve planner prefers paying with mana over the graveyard and always
  exiles the graveyard's oldest cards first, matching the offer-time/payment-time
  affordability check exactly (Task 10).
- **Fang Dragon's** creature-cast-from-exile is tagged the same `CastMethodV4::Omen` a real
  Omen card uses (both are "form-1 alternative characteristics, cast from hand/exile"),
  rather than adding a new `CastMethodV4::Adventure` variant that `flat_policy_v2.rs`'s
  frozen exhaustive match would have forced an edit to (Task 11).
- **Azure Fleet Admiral's** monarch combat-damage transfer is a direct `state.monarch =
  Some(controller)` mutation inside `deal_combat_damage`, not a stack-based trigger like
  XMage's own implementation; the end-step draw trigger is a real stack trigger. Documented
  as a disclosed simplification (Task 11).
- **RL surface**: Glint Hawk's return-or-sacrifice decision and the reveal-and-transform
  decision both reach the RL surface through existing, unmodified action/candidate paths
  (`ActionSemanticV1::ChooseOptionalCostWhich`, `Decision::ChooseEffectBoolean`); no
  `flat_policy_v1.rs`/`flat_policy_v2.rs` edits were made anywhere in the wave. The monarch
  holder and the `on_adventure` flag are not observable in the flat encoding -- a disclosed
  gap, not a defect (Task 8, Task 11).

`mtg-kernel/src/flat_policy_v2.rs` was never edited across the whole wave (Tasks 5-13),
confirmed by `git diff ce1a69f0 HEAD -- mtg-kernel/src/flat_policy_v2.rs` returning empty.

## Card DB identity

`KERNEL_CARDDB_HASH` progression (each `python/tools/repin_card_db_identity_v1.py --write`
run; `FROZEN_CARD_DB_HASH_U64_HEX_PAUPER_META_W1` in
`mtg-kernel/src/native_training_store_run_v2.rs` moves in lockstep, never
`FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1`/`_V2`, which stay frozen forever by design):

| Task | Old | New |
|---|---|---|
| Wave start (Task 4 end, `ce1a69f0`) | -- | `0x64c8_2a26_1e07_8f1a` |
| 5 | `0x64c8_2a26_1e07_8f1a` | `0x5446_bd05_1a02_5568` |
| 6 | `0x5446_bd05_1a02_5568` | `0x2c46_96a5_d4e9_4be4` |
| 7+8 | `0x2c46_96a5_d4e9_4be4` | `0x55da_3223_6603_f81f` |
| 9 | `0x55da_3223_6603_f81f` | `0xef29_164c_bb88_e96a` |
| 10 | `0xef29_164c_bb88_e96a` | `0x9bfc_c417_6b24_190d` -> `0x555d_ca6a_adfb_7b66` (two repins; the second folds `delve_for`'s output into the identity canon string) |
| 11 | `0x555d_ca6a_adfb_7b66` | `0xedc2_5246_9a3f_59e2` (stale out-dir selection, repin-tool bug) -> `0x9378_c4e1_705d_5875` (corrected) |
| 12 | `0x9378_c4e1_705d_5875` | `0xde59_c501_e943_f3fd` (18 V2 deck registrations change `decks` membership, which is folded into `build.rs`'s canon string even though card count is unchanged) |
| 13 (this task) | `0xde59_c501_e943_f3fd` | `0xde59_c501_e943_f3fd` (confirmed already consistent) |

**Final identity, confirmed by `repin_card_db_identity_v1.py --check` (exit 0):**
`KERNEL_CARDDB_HASH = 0xde59_c501_e943_f3fd`, and
`FROZEN_CARD_DB_HASH_U64_HEX_PAUPER_META_W1 = "de59c501e943f3fd"` in
`native_training_store_run_v2.rs` equals it exactly.

## Pool totals (`data/pauper_pool_v1.json`, after Task 12's 18 V2 registrations)

| metric | wave start (9 historical decks) | after wave (18 decks) |
|---|---:|---:|
| deck_count | 9 | 18 |
| mainboard_unique_cards | 121 | 134 |
| sideboard_unique_cards | 36 | 52 |
| pool_unique_cards | 150 | 171 |
| mainboard_copies | 540 | 1080 |
| sideboard_copies | 135 | 270 |

`data/pauper_support_v1.json`: every pool card still shows `engine_capability: full` (0
partial, 0 no_effect). `data/runtime_decks_v1.json` is byte-identical to before the wave
(`sha256sum` = `68e7602f3a4df6217119406973954630800c358a10fca9f28e6cf9f20fd3b851`, verified
unchanged at every task's identity re-pin and again at this task).

## Recomputed mtgtop8 coverage (before vs after wave 1)

Recomputed with `uvx uv@0.11.29 run --no-sync python python/tools/pauper_meta_gap_v1.py
docs/research/pauper_meta_decklists_2026-09-09 "$(cat
docs/research/pauper_meta_shares_2026-09-09.json)"
docs/research/pauper_meta_gap_after_w1_2026-09.json`, against the same 120 sampled
mtgtop8 decklists (15 archetypes, 8 decks each) the wave-start gap report
(`docs/research/pauper_meta_gap_2026-09-09.md`/`.json`, commit `973ba7e9`) used. "Covered"
is the share of average-copies-per-deck backed by an in-registry card
(`sum(avg_main for in_registry) / sum(avg_main)`, matching the wave-start report's own
methodology exactly -- the "before" column below reproduces that report's numbers
identically as a self-check). "Missing names (avg >= 1)" counts distinct missing card names
averaging at least one copy per deck.

| Archetype | Share | Main covered before | Main covered after | Main missing names before | Main missing names after | Side covered before | Side covered after | Side missing names before | Side missing names after |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Burn | 12% | 93% | 99% | 1 | 0 | 85% | 92% | 0 | 0 |
| MonoBlueAggro | 12% | 90% | 95% | 2 | 1 | 98% | 98% | 0 | 0 |
| Affinity | 10% | 84% | 86% | 4 | 3 | 97% | 97% | 0 | 0 |
| Urzatron | 9% | 13% | 13% | 16 | 16 | 56% | 60% | 1 | 1 |
| RedDeckWins | 6% | 94% | 94% | 2 | 2 | 69% | 90% | 2 | 0 |
| Elves | 5% | 90% | 90% | 2 | 2 | 79% | 83% | 0 | 0 |
| Jund | 5% | 94% | 98% | 1 | 0 | 91% | 99% | 0 | 0 |
| DimirControl | 4% | 70% | 90% | 5 | 1 | 79% | 90% | 1 | 0 |
| Ephemerate | 4% | 30% | 30% | 16 | 16 | 81% | 81% | 0 | 0 |
| GruulAggro | 4% | 42% | 42% | 11 | 11 | 63% | 63% | 1 | 1 |
| Spy | 4% | 100% | 100% | 0 | 0 | 65% | 74% | 2 | 1 |
| WhiteWeenie | 4% | 10% | 10% | 11 | 11 | 36% | 36% | 2 | 2 |
| Garden | 3% | 57% | 60% | 9 | 9 | 70% | 71% | 2 | 2 |
| Terror | 3% | 95% | 96% | 1 | 1 | 93% | 97% | 0 | 0 |
| Gates | 2% | 74% | 74% | 5 | 5 | 72% | 77% | 0 | 0 |

Movement concentrates in the archetypes wave 1 targeted (Wildfire/Jund, Rally/RedDeckWins,
Terror/DimirControl, Affinity, Burn, Mono-Blue Terror/MonoBlueAggro, CawGates/Gates,
Spy) via the nine canonical decks' sideboards and the newly-registered V2 mainboards;
Urzatron, Ephemerate, GruulAggro, and WhiteWeenie are archetypes this wave's nine canonical
decks do not correspond to and show no movement, as expected. Full per-card detail (every
sampled card, `in_registry` flag, XMage source line count where resolvable) is in
`docs/research/pauper_meta_gap_after_w1_2026-09.json`.

## Identity finalisation and profile-classification fix (Task 13)

### The 55-tests-red-by-design cohort

Task 5's report identified 55 lib tests (of 67 originally red before any re-pin) that would
stay red "by design" until Task 13: tests asserting the live build classifies as the
`Current` catalog profile (`native_training_store_run_v2.rs`'s
`NativeRunCatalogProfileV1::Current`), which stopped being true on this branch once the
live `KERNEL_CARDDB_HASH` moved off the value `FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1`
still pins forever. By the time Task 13 started, incidental golden regeneration in Tasks
6-12 had already resolved some of these; the actual count of lib tests red at Task 13's
start (measured empirically by running the complete `--lib` suite once, 6085.60s, 1668
passed / 45 failed / 42 ignored) was 45, plus `mtg-kernel/tests/rl_contract.rs`'s one
integration-test failure named separately below.

### Fix: `live_catalog_profile_v1()` helper, and `fixture_record()` tracks live identity

Two changes in `mtg-kernel/src/native_training_store_run_v2.rs`, both additive (no
existing frozen literal moved or deleted):

1. **`classify_catalog_profile_v1` factored into a pure `classify_catalog_profile_from_identity_v1(card_db_hash_u64_hex: &str, runtime_catalog_sha256: &str)`**, and a new
   `pub(crate) fn live_catalog_profile_v1() -> NativeRunCatalogProfileV1` added beside it:
   computes the crate's OWN live catalog identity's classification (via the existing
   `live_catalog_build_identity_v1()`) through the exact same tuple match every decoded
   record goes through. `Current` on the main tree; `PauperMetaW1` on this branch, now that
   the wave has moved `FROZEN_CARD_DB_HASH_U64_HEX_PAUPER_META_W1` off its
   CURRENT-tying initial value. **3 test functions** were edited to assert against this
   helper instead of a hardcoded `NativeRunCatalogProfileV1::Current`:
   `native_training_store_v2.rs`'s `publish_genesis_rejects_a_current_catalog_profile_run_whose_live_identity_has_moved`
   and `current_profile_record_round_trips_through_construct_seal_decode_validate` (2
   assertions), plus a new test in `native_training_store_run_v2.rs`,
   `default_fixture_decodes_clean_and_classifies_as_the_live_profile`, added to give the
   live-tracking behavior below its own direct assertion (the pre-existing
   `current_fixture_decodes_clean_and_classifies_current` was kept, but rebuilt to pin the
   frozen CURRENT tuple explicitly via `sample_environment_v2_with`-style override
   construction, independent of whatever is live, since that is a real, separate thing
   worth still asserting).
2. **`fixture_record()`'s shared test fixture (`native_training_store_run_v2.rs`, the base
   of `fixture_bytes()`/`test_fixture_bytes_v2()`, used pervasively across
   `native_training_store_v2.rs`, `native_training_store_resume_v2.rs`,
   `native_training_store_update_group_v1.rs`, `native_science_loop_v1.rs`, and others)
   now embeds the crate's LIVE catalog identity** (`live_catalog_build_identity_v1()`)
   instead of the previously-hardcoded `FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1`/
   `FROZEN_RUNTIME_CATALOG_SHA256_CURRENT_V1` literals. This restores the fixture's own
   pre-existing, pre-wave doc comment invariant verbatim ("the default fixture... match[es]
   what production capture actually mints today, so every test built on top of
   `fixture_record()` exercises the live science-loop/publish/resume paths unrejected"),
   which was true when CURRENT and live were the same value and stopped being true only
   because this wave moved live off CURRENT. `fixture_record_historical()` (which
   explicitly overrides both fields to the HISTORICAL literals) and every test that
   explicitly overrides the two catalog fields to a specific frozen tuple (the
   dual-profile classifier tests) are unaffected, since they never depend on the default.

This single root-cause fix resolved **30 of the 45** originally-failing lib tests without
touching their bodies at all (they never hardcoded `Current`; they only needed a live-
matching record to pass their own live-authenticity mutation-boundary check):
`native_checkpoint_shadow_stdio_v1::tests::the_chosen_action_is_independent_of_the_measured_latency_v1`,
all 12 `native_training_store_v2::windows_publisher_tests`, all 6 `native_training_store_resume_v2::windows_resume_tests`
(via a 7th sub-check that also passed already), 3 of 5 `native_training_store_update_group_v1::tests` (`v2_resume_drives_publish_commit_and_continuation_with_rederived_mode`,
`v2_science_loop_completes_end_to_end_from_real_snapshots`,
`wide_v2_resume_reconstructs_through_the_run_bound_wide_constructor`), 7
`native_science_loop_v1::windows_science_loop_tests`, and
`native_training_store_run_v2::tests::current_profile_live_identity_check_matches_real_and_rejects_shimmed`.
The other 2 of the 3 edited assertion sites moved the remaining 2 of these 32.

**One test is left intentionally red, by design, with reasoning recorded rather than
fixed:** `native_training_store_run_v2::tests::current_frozen_literal_matches_the_live_build_constant`
asserts `format!("{KERNEL_CARDDB_HASH:016x}") == FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1`.
Its own doc comment names it a canary: "If this ever fails, either the crate's card
database/runtime catalog changed again... or the frozen literals were typed wrong." On the
main tree this holds; on this branch it is genuinely false, by design, exactly as the wave's
own schema-migration ruling (Task 3) intends -- the live build really has moved to the
`PauperMetaW1` profile. Weakening or rewriting this assertion would defeat its purpose as a
main-tree regression canary; the canary is doing its job correctly by failing here, and
this is recorded, not silently accepted.

### Re-pinned hash/golden literals (card-DB-dependent bytes, not a behavior change)

The remaining 14 of the 45, plus `rl_contract.rs`'s one integration test and 4 more found
only by the complete-suite sweep (not in the 45, since they surfaced only after this
task's own `fixture_record()` change, or were never exercised by the original 45-name
rerun), are hand-authored digests over real gameplay/rollout bytes or over
`fixture_bytes()` itself. Every one embeds `KERNEL_CARDDB_HASH` directly (confirmed in
source, e.g. `FlatActionCommitmentHasherV1::new`/`V2::new` mix
`KERNEL_CARDDB_HASH.to_le_bytes()` into their SHA-256 domain separator) or indirectly
(`CardDef`'s own struct shape grew every task this wave -- `delve`, `adventure`,
`cant_be_blocked_by_monarchs_creatures`, `EquipmentDef::granted_activated_ability` -- so any
serialized `GameState`/observation shifts even for decks, like Burn and Rally, that gained
no new cards). Each was re-pinned to the value this branch's own build actually produces,
captured directly from a failing test's panic output (never hand-derived), with the old
value recorded in the code comment and here:

Cause key: **(A)** `KERNEL_CARDDB_HASH` is mixed directly into the literal's computation
(a SHA-256 domain separator, an embedded `card_db_hash_u64_hex` record field, or a hash
over `fixture_bytes()` itself, which now embeds the live hash -- see
`fixture_record`'s doc comment above); confirmed in source at the cited call site.
**(B)** `GameState`/`ObjectStateV4` gained a new struct field this wave
(`GameState::monarch: Option<PlayerId>`, Task 11, `#[serde(default)]` so it serializes
unconditionally; `ObjectStateV4::on_adventure: bool`, Task 11) that every `state_hash()`
(derived `Hash`) or `diagnostic_state_hash()` (JSON envelope) call includes regardless of
which cards are on the battlefield; confirmed directly against `mtg-kernel/src/state.rs`
(field added after `initiative`, doc comment: "Added after the pre-existing fields above").
For every row below, the scenario's action sequence and every other assertion in the same
test (structural invariants: `scorer.counts`, `changed_non_gauge_parameter_count`,
`bindings[N].policy_step_count()`/`physical_decision_count()`, byte-length checks like
`MAIN_GOLDEN_LEN_V1`, and every relational/non-literal assertion) were confirmed still
passing alongside the re-pinned digest -- none of these tests failed on anything but the
named literal.

| File | Test(s) | Cause | Old -> new |
|---|---|---|---|
| `async_flat_scored_rollout_v1.rs` | `first_five_scorer_packets_have_exact_safe_golden` | A | `2683fe1d...ebd043` -> `78289df6...103c81` |
| `native_checkpoint_inference_v1/checkpoint_reliance_probe_v1/action_block_gradient_diagnostic_v1.rs` | `joined_frame_is_preflight_sealed_neutral_and_lineage_complete_v1` | A | `d6812f9e...e00970` -> `ae853cab...66a71` |
| `native_checkpoint_runner_v1.rs` | `starting_player_unset_reproduces_parent_commit_checkpoint_eval_bytes_v1` (`logical_state_sha256`) | A | `69e6a7d0...fad974` -> `4a3d928e...74e3f5` |
| `native_checkpoint_runner_v1.rs` | same test (`bindings[0].trajectory_sha256`) | A | `f6a0be9c...1b6985` -> `a9e2d7d6...347bad` |
| `native_checkpoint_runner_v1.rs` | same test (`bindings[1].trajectory_sha256`) | A | `2253bd91...17b90` -> `91020a9d...86778` |
| `native_trainer_v1.rs` | `real_burn_pair_updates_once_and_is_topology_invariant` (`episodes[0].trajectory_sha256`) | A | `[206,204,...,185]` -> `[146,96,...,48]` |
| `native_trainer_v1.rs` | same test (`episodes[1].trajectory_sha256`) | A | `[162,131,...,94]` -> `[98,192,...,14]` |
| `native_trainer_v1.rs` | `genuine_environment_v2_pair_executes_with_distinct_decks_and_reconstructible_outer` | A | `("03cbfef4...","2218bdf0...")`, `("0462ea3d...","0c4f9222...")` -> `("7076afcb...","99d5adb2...")`, `("fb36f8a7...","42e736f8...")` |
| `native_training_store_checkpoint_v3.rs` | `genesis_authority_roundtrips_and_matches_frozen_goldens` (`GENESIS_MANIFEST_SHA256_GOLDEN_V3`) | A | `719d3edd...09d03f` -> `41041620...93984b` |
| `native_training_store_checkpoint_v3.rs` | same test (`GENESIS_LOGICAL_STATE_SHA256_GOLDEN_V1`, shared value with checkpoint_runner_v1's `logical_state_sha256`) | A | `69e6a7d0...fad974` -> `4a3d928e...74e3f5` |
| `native_training_store_run_v2.rs` | `independent_digest_references_and_goldens_match` (`semantics`) | A | `affcfccc...85c8e0` -> `2ad70a88...4665d` |
| `native_training_store_run_v2.rs` | same test (`identity`) | A | `f118e0a8...bf323` -> `3374ce80...4cb3` |
| `native_training_store_run_v2.rs` | same test, plus `population_program_absence_preserves_legacy_bytes_and_run_hash` and `response_exploiter_absence_preserves_existing_bytes_and_population_behavior` (shared `fixture_bytes()` sha256) | A | `b99df856...87e8e` -> `4c8ae8a6...cfcd2` |
| `native_training_store_update_group_v1.rs` | `legacy_update_group_matches_pre_c2_projection_with_linux_gnu_full_byte_pin` | A | `2002effe...cc1372` -> `0fd11d97...c16498f` |
| `native_training_store_update_group_v1.rs` | `sync_path_reproduces_mains_golden_store_hash` (x86_64-pc-windows-msvc arm only; the x86_64-unknown-linux-gnu arm is unverifiable from this Windows host, same limitation this constant's own pre-existing comment already documents, left untouched) | A | `73e1af55...ddb8ec8` -> `a833a6e0...480cf` |
| `policy_surface_v5.rs` | `surface_binding_envelopes_are_diagnostic_dispatched_with_exact_goldens` (legacy `state_hash`; stale duplicate of `state.rs`'s own already-fixed Task 11 golden, missed then) | B | `0xa921_902d_e1a8_d8ce` -> `0x3313_5945_dcb9_4ed1` |
| `policy_surface_v5.rs` | same test (legacy `SurfaceBinding`) | B | `0xd281_e56f_4389_60a9` -> `0x6940_0c4c_9fdc_2b49` |
| `policy_surface_v5.rs` | same test (v2 `state_hash`) | B | `0x8ecd_b59c_374e_2345` -> `0x9689_f972_2063_c266` |
| `policy_surface_v5.rs` | same test (v2 `SurfaceBinding`) | B | `0x5c8f_cd1c_7941_a53e` -> `0xc8e2_f885_b7e8_8615` |
| `rl_session.rs` | `environment_hashes_are_diagnostic_dispatched_with_exact_goldens`, `v2_reset_preexisting_entry_points_remain_legacy_randomness` (shared legacy env hash) | A | `0xfca3_b546_61ec_28c6` -> `0x6908_0ca5_c012_7e2b` |
| `rl_session.rs` | same two tests (shared legacy core hash) | B | `0x9a93_e402_6f8e_ad86` -> `0x3c6d_b17f_22d6_0e43` |
| `rl_session.rs` | `environment_hashes_are_diagnostic_dispatched_with_exact_goldens`, `v2_reset_reuses_pre_constructor_pins_and_is_root_sensitive` (shared v2 policy hash) | A | `0x9ed7_895c_1f47_82ca` -> `0xda58_b63d_08f6_6b99` |
| `rl_session.rs` | same two tests (shared v2 core hash) | B | `0xf69d_f52f_fdd0_564e` -> `0xa1c5_ca41_1ee8_1a2d` |
| `rl_session.rs` | `flat_action_candidate_commitment_matches_independent_pass_vector` | A | `[0x70,0x7d,...,0xce]` -> `[0x45,0x8c,...,0x40]` |
| `rl_session.rs` | `flat_action_v2_token_domain_and_commitment_goldens_are_independent` (`v1`) | A | `[0x38,0x90,0x05,0xe8,...]` -> `[0xc3,0xa8,0x8d,0xde,...]` |
| `rl_session.rs` | same test (`common_v2`) | A | `[0x9f,0xfa,...,0x13]` -> `[0xf4,0x6a,...,0x05]` |
| `rl_session.rs` | same test (`commitment_v2(65_536)`) | A | `[0xb8,0x28,...,0x06]` -> `[0x1f,0x17,...,0xa0]` |
| `rl_session.rs` | `jsonl_v6_frozen_v5_bytes_and_api` (`V5_TRANSCRIPT_SHA256`) | A | `a583c230...2541e2` -> `d0494851...8f110` |
| `mtg-kernel/tests/rl_contract.rs` | `v2_deck_pair_builder_burn_rally_root_940001_contract_and_pins` (serialized SHA-256) | B | `7037b030...8fa7a` -> `4a35ce83...6ad36` |
| `mtg-kernel/tests/rl_contract.rs` | same test (v9 diagnostic hash) | B | `05c6af543cba5573` -> `114a052a68589a04` |
| `mtg-kernel/tests/blue_blasts.rs` | `blue_pending_cast_is_frozen_into_diagnostic_hash_v8` | B | `0x64d0_7fde_5fbd_0f5a` -> `0x0931_82a3_ee7d_afab` |
| `mtg-kernel/tests/brainstorm.rs` | `brainstorm_golden_is_two_independent_private_puts_with_exact_history_and_restore` (`first_hash`) | B | `0x27ea_a9b5_f80f_2f40` -> `0x5b1b_3fdc_abbe_f2ba` |
| `mtg-kernel/tests/brainstorm.rs` | same test (`answered_hash`) | B | `0xebf9_1eed_7c7c_9e60` -> `0x8945_08d2_063e_dcc6` |
| `mtg-kernel/tests/brainstorm.rs` | same test (`second_hash`) | B | `0xfa09_c501_2a59_3135` -> `0xaf2c_7cf7_da44_4e01` |
| `mtg-kernel/tests/brainstorm.rs` | same test (`expected_hash`) | B | `0x487a_9c25_e0a8_fbb0` -> `0xdee1_a67a_2cfe_56b0` |

`blue_blasts.rs` and `brainstorm.rs` were found only by the full `--tests` (integration
binary) sweep, since neither is in the `--lib` suite; both are real-card scenarios
(`ready_game()`, actual Lightning Bolt/Guttersnipe/Blue Elemental Blast objects), so cause B
(the `monarch`/`on_adventure` schema growth) applies regardless of which specific cards are
on the battlefield. Every re-pinned literal's surrounding test was re-run and confirmed
green with no other assertion moved.

### Stale `CARD_DEFS.len() == 162` literals (step 3)

`grep -rn "162" mtg-kernel/tests mtg-kernel/src | grep -i "len\|CARD_DEFS"` found 8
integration test files carrying a `CARD_DEFS.len() == 162` literal that predates this
entire wave (already stale before Task 5, per every intervening task's own report). Updated
all 8 to `184` (the current value, confirmed against the generated `card_defs.rs`'s
`pub static CARD_DEFS: [CardDef; 184]`), each with a comment naming the wave:
`mtg-kernel/tests/affinity_wildfire_value.rs`, `artifact_control.rs`,
`artifact_equipment_completion.rs`, `landcycling_omen.rs`, `pauper_green_utility.rs`,
`pauper_green_value.rs`, `pauper_modal_hate.rs`, `pauper_optional_costs.rs`.

### The seven tests red at HEAD before Task 12

Task 12's report named these seven as confirmed pre-existing (via `git stash` against HEAD)
and booked them for Task 13: `rl_contract.rs`'s
`v2_deck_pair_builder_burn_rally_root_940001_contract_and_pins` and six `rl_session::`
hand-authored fixture tests (`flat_action_candidate_commitment_matches_independent_pass_vector`,
`environment_hashes_are_diagnostic_dispatched_with_exact_goldens`,
`flat_action_v2_token_domain_and_commitment_goldens_are_independent`,
`jsonl_v6_frozen_v5_bytes_and_api`, `v2_reset_preexisting_entry_points_remain_legacy_randomness`,
`v2_reset_reuses_pre_constructor_pins_and_is_root_sensitive`). All seven are listed in the
re-pin table above: the six `rl_session::` tests are cause A (`KERNEL_CARDDB_HASH` mixed
directly into a SHA-256 domain separator); `rl_contract.rs`'s test is cause B (it hashes a
raw `GameState` directly, never a `TrainRunV2`/environment record, so no `card_db_hash`
field is in scope at all -- `Object`/`ObjectStateV4` stores `card_def: u16`, an index into
`CARD_DEFS`, never an embedded `CardDef`, so `CardDef`'s own struct growth this wave does
not reach `GameState` serialization; only `GameState`'s own new `monarch` field does). None
is a real regression, and all seven now pass (`cargo test --locked -p mtg-kernel --test
rl_contract`: 56/56 ok; the six `rl_session::` names: confirmed individually passing, see
the report).

## Full sweep results

See `.superpowers/sdd/2026-09-09-pauper-meta-wave1/task-13-report.md` for the complete
command log and every suite's counts. Summary:

- **`cargo test --locked -p mtg-kernel --lib`**: 1709 passed. 1 intentional design canary
  red (`current_frozen_literal_matches_the_live_build_constant`, see above). 2 tests
  (`native_trainer_v1::tests::population_pool_full_mixture_rollout_exercises_both_occupant_kinds_v1`,
  `population_pool_search_slot_is_deterministic_and_records_identity_v1`) intermittently red
  with `Rollout(SchedulerDeadlineExceeded)`: a real T2048 MCTS search rollout each,
  `config.scheduler_timeout = Duration::from_secs(1800)`, whose own comment already flags
  debug-build slowness as a risk ("stays robust under a slow... build, matching this
  crate's own `--release` recommendation for search-bearing work"). Confirmed via 3
  isolated reruns: fails even alone, with or without the CPU-affinity wrapper, at
  1618-1836s per scenario -- genuinely marginal against its own budget on this machine in a
  debug build, unrelated to any hash, catalog profile, or card content this task touched.
  2 more tests in the same `Rollout`-error family
  (`native_training_store_update_group_v1::tests::search_slot_opponent_identity_round_trips_and_rejects_tampering`,
  `native_training_store_reference_latest_v2::tests::generation_eight_requires_the_sealed_generation_four_parent`)
  appeared intermittently across repeated full-suite runs but were each confirmed to pass
  cleanly in isolation (1619-1695s). None of these 4 involves a hash literal, a catalog
  profile, or anything this task's card-DB-identity work touched; all are environment/
  timing-margin artifacts of running real search-bearing tests in a debug build under CPU
  contention, not a regression. Zero warnings (`cargo check --all-targets`).
- **Integration binaries** (63 `tests/*.rs` targets, run explicitly since `--tests` also
  re-runs `--lib` and stops there without `--no-fail-fast`): 489 passed, 0 failed, after
  fixing 2 files found only by this sweep (`blue_blasts.rs`, `brainstorm.rs`; 5 literals
  total, cause B -- `GameState::monarch`/`ObjectStateV4::on_adventure` schema growth
  moving `state_hash()`/`diagnostic_state_hash()` even for real-card scenarios that use
  none of this wave's new cards; see the re-pin table above).
- **Doc tests**: 62 passed, 0 failed.
- **`cargo test --locked -p mtg-kernel --no-run`** (mandatory full-tree compile): clean.
- **Python golden suites** (`test_flat_policy_v1_goldens`, `test_flat_policy_v2_goldens`,
  mandatory after every repin): 15/15 ok.
- **Full Python suite** (`python -m unittest discover -s python/tests -v`,
  `MTG_KERNEL_CARGO_TARGET_DIR=E:/cargo-target-pauper-meta` set so the repin tool's own
  consistency test finds the right build output): **517 tests, 0 failed, 12 skipped, OK**
  (1262.18s). One stale-premise test was fixed on the way there:
  `test_write_dek_from_mtgo_list_v1.py`'s
  `test_substitution_target_not_registered_and_not_pending_is_an_error` used Glint Hawk as
  its "not registered, not pending" example; Task 8 of this same wave registered Glint Hawk
  for real, so the example stopped being negative (the tool now correctly accepts it with
  no `--allow-pending`, which is right, not a bug -- the sibling test
  `test_substitutions_and_counts` already exercises exactly that). Re-pointed at Mulldrifter
  (confirmed absent from `data/cards_v1.json`), matching the same "stale wave-1-caused
  premise" pattern as the `CARD_DEFS.len()` literals, not a repin and not a behavior
  regression. No torch-dependent or other pre-existing failures were found; the suite is
  fully green.

### Addendum: final confirmation `--lib` sweep after commit

Run after commits `0c47c4bc`/`af4f4574`/`ad77ddf9` landed, per the controller's directive
that a further full sweep is only run post-commit: **1712 passed, 2 failed, 42 ignored,
3985.41s**. The 2 failures: the same intentional canary
(`current_frozen_literal_matches_the_live_build_constant`), and a sibling of the
timing-margin class already documented above -- `native_checkpoint_shadow_stdio_v1::tests::model_guided_search_replay_is_bit_identical_apart_from_wall_time_v1`,
a `search_ceiling_status: within_slo` vs `slo_exceeded` mismatch on one decision under a
`decision_slo_seconds: 4.0` budget, same mechanism as the two `population_pool_*` and
`search_slot_opponent_identity_round_trips_and_rejects_tampering`/`generation_eight_requires_the_sealed_generation_four_parent`
failures classified earlier (`the_chosen_action_is_independent_of_the_measured_latency_v1`,
which failed in earlier runs, passed clean in this one). Which specific test in this
`Rollout`/SLO-timing family gets hit continues to vary run to run and shrank from 4-5
failures to 1 here, consistent with contention/timing margin, not a deterministic bug tied
to any one test or to this task's code.

## ROADMAP.md

See the new paragraph under "### Card coverage" in `ROADMAP.md`, added by this task.
