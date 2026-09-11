# Pauper meta wave 1: kernel branch-coverage parity record

Task 14a (kernel-side parity half of Task 14). Lead ruling of 2026-09-10
split Task 14: the CP7 shadow harness cannot play the V2 registrations, so
the XMage shadow games are Task 14b, a separate plan after the deck-model
harness. This wave is **kernel-covered, not XMage-shadow-certified**.

Manifest: `data/wave_branch_manifests/pauper_meta_w1.json`
Checker: `python/tools/check_wave_branch_coverage_v1.py`
Checker JSON report (this record's machine-readable twin):
`docs/research/pauper_meta_wave1_parity_2026-09.json`

## 1. Branch-coverage verdict

`uvx uv@0.11.29 run --no-sync python python/tools/check_wave_branch_coverage_v1.py data/wave_branch_manifests/pauper_meta_w1.json --json docs/research/pauper_meta_wave1_parity_2026-09.json`

```
kernel-covered: 23 of 23 cards, 0 unexercised branches
```

All 23 manifest entries (21 wave 1 cards, the Insectile Aberration
transform face, and the Squirrel Token) have every declared branch bound to
at least one passing test. No branch is marked `unreachable` in this wave.

## 2. Per-card coverage table

`post_board` follows `data/pauper_pool_v1.json` mainboard/sideboard
membership (a token or transform face follows its creating/front card).

| Card | post_board | Branches (covering test) |
| --- | --- | --- |
| Abandon Attachments | false | `discard_accepted_draws_two`, `discard_declined_no_draw`: `pauper_meta_w1_spells.rs:375 abandon_attachments_draws_two_only_if_a_card_is_discarded` |
| Acorn Harvest | true | `creates_two_squirrel_tokens`, `flashback_from_graveyard_costs_1g_plus_3_life`, `flashback_unavailable_below_three_life`: `pauper_meta_w1_spells.rs:441 acorn_harvest_creates_two_squirrels_and_flashback_costs_three_life` |
| Ancient Grudge | true | `destroys_target_artifact`, `flashback_from_graveyard_for_g`: `pauper_meta_w1_spells.rs:171 ancient_grudge_destroys_an_artifact_and_flashes_back_for_g` |
| Arms of Hadar | true | `targeting_p1_shrinks_p1_creatures_only`, `targeting_p0_shrinks_p0_creatures_only`: `pauper_meta_w1_black_red.rs:217 arms_of_hadar_shrinks_only_the_targeted_players_creatures` |
| Artful Dodge | false | `grants_target_unblockable_this_turn`, `unblockable_grant_expires_end_of_turn`: `pauper_meta_w1_spells.rs:237 artful_dodge_makes_the_target_unblockable_this_turn_and_flashes_back`; `flashback_from_graveyard_for_u`: `pauper_meta_w1_spells.rs:338 artful_dodge_flashes_back_for_u_from_the_graveyard` (new test) |
| Azure Fleet Admiral | true | `etb_becomes_the_monarch`, `monarch_draws_at_own_end_step`, `non_monarch_end_step_no_draw`, `crown_does_not_move_without_combat_damage`: `pauper_meta_w1_adventure_and_monarch.rs:309 azure_fleet_admiral_makes_its_controller_the_monarch_who_draws_at_end_step`; `combat_damage_to_monarch_transfers_crown`: `:366 combat_damage_to_the_monarch_moves_the_crown`; `monarchs_creatures_cannot_block_the_admiral`, `non_monarchs_creatures_may_block_the_admiral`: `:417 admiral_cannot_be_blocked_by_the_monarchs_creatures` |
| Contaminated Aquifer | false | `enters_tapped`, `taps_for_u_or_b`: `pauper_meta_w1_snuff_out_and_duals.rs:237 contaminated_aquifer_enters_tapped_and_taps_for_u_or_b` |
| Delver of Secrets | false | `upkeep_trigger_controller_only`: `pauper_meta_w1_delver.rs:258`; `reveal_instant_or_sorcery_transforms`, `reveal_declined_no_transform`: `:154 delver_transforms_when_the_revealed_top_card_is_an_instant_or_sorcery`; `non_instant_top_no_transform`: `:232`; `transformed_face_does_not_retrigger`: `:285` |
| Fang Dragon | true | `adventure_castable_from_hand_at_sorcery_speed`, `adventure_damages_only_creatures_you_dont_control`, `adventure_resolution_exiles_card_with_on_adventure_permission`, `creature_face_castable_from_exile_via_on_adventure`, `on_adventure_permission_clears_when_creature_enters_battlefield`, `plain_exile_placement_never_grants_adventure_permission`: `pauper_meta_w1_adventure_and_monarch.rs:139 forktail_sweep_is_cast_from_hand_then_the_dragon_is_cast_from_exile`; `creature_castable_directly_from_hand`: `:243`; `visible_name_resolves_adventure_face_to_face_index_2`: `:287`; `adventure_cast_still_triggers_cast_instant_or_sorcery_abilities`: `:512`; `creature_cast_does_not_trigger_cast_instant_or_sorcery_abilities`: `:545` |
| Gixian Infiltrator | false | `counter_on_sacrifice_of_another_permanent`, `self_sacrifice_does_not_trigger`: `pauper_meta_w1_creatures.rs:183 gixian_infiltrator_grows_when_another_permanent_is_sacrificed` |
| Glint Hawk | false | `etb_sacrificed_with_no_artifact_to_return`, `etb_accept_returns_artifact_keeps_hawk`, `etb_decline_sacrifices_hawk_even_with_artifact_available`: `pauper_meta_w1_creatures.rs:335 glint_hawk_is_sacrificed_unless_an_artifact_is_returned` |
| Gurmag Angler | false | `delve_pays_generic_by_exiling_all_graveyard_cards`: `pauper_meta_w1_delve_and_longbow.rs:181`; `not_castable_when_neither_mana_nor_delve_pays_the_cost`: `:224`; `delve_combines_with_partial_mana_to_pay_remaining_generic`: `:246`; `planner_prefers_paying_mana_over_delving_when_both_plans_legal`: `:286`; `delve_never_exceeds_the_printed_generic_amount`: `:353`; `delve_exiles_oldest_graveyard_cards_first`: `:379` |
| Ice Tunnel | false | `is_snow_typed_dual_land`: `pauper_meta_w1_snuff_out_and_duals.rs:262 ice_tunnel_is_snow` |
| Insectile Aberration | false | `transformed_face_characteristics`: `pauper_meta_w1_delver.rs:154`; `visible_name_resolves_to_face_index_1`: `mtg-kernel/src/card_def.rs:1641 visible_name_resolves_faces` |
| Kessig Flamebreather | false | `noncreature_spell_cast_damages_each_opponent`, `creature_spell_cast_no_trigger`: `pauper_meta_w1_creatures.rs:145 kessig_flamebreather_pings_each_opponent_on_noncreature_casts_only` |
| Raze | true | `not_castable_without_any_land`, `sacrifices_a_land_as_additional_cost_and_destroys_the_target_land`, `sole_land_pays_mana_and_is_still_sacrificed`: `pauper_meta_w1_black_red.rs:417 raze_requires_sacrificing_a_land_and_destroys_the_target_land` |
| Smash to Smithereens | true | `destroys_target_artifact`, `deals_three_to_artifacts_controller`: `pauper_meta_w1_black_red.rs:281 smash_to_smithereens_destroys_the_artifact_and_burns_its_controller`; `no_legal_target_without_artifact`: `:388 smash_to_smithereens_has_no_legal_target_without_an_artifact` (new test) |
| Snuff Out | false | `alt_cost_offered_only_with_a_swamp`, `alt_cost_pays_4_life_no_mana`, `cannot_target_black_creatures`: `pauper_meta_w1_snuff_out_and_duals.rs:121 snuff_out_offers_the_life_payment_only_with_a_swamp`; `still_castable_for_printed_mana_cost_without_swamp`: `:199` |
| Squirrel Token | true | `token_is_1_1_green`: `pauper_meta_w1_spells.rs:441 acorn_harvest_creates_two_squirrels_and_flashback_costs_three_life` |
| Suffocating Fumes | false | `pump_opponents_creatures_minus_one_minus_one`, `effect_expires_end_of_turn`, `casters_own_creatures_unaffected`: `pauper_meta_w1_black_red.rs:131 suffocating_fumes_gives_opponents_creatures_minus_one_until_end_of_turn`; `cycling_for_two`: `:177` |
| Terminate | true | `destroys_target_creature`: `pauper_meta_w1_spells.rs:114`; `no_legal_target_without_a_creature`: `:141` |
| Viridian Longbow | true | `equip_grants_tap_ping_ability`, `unequipped_creature_has_no_granted_ability`: `pauper_meta_w1_delve_and_longbow.rs:429`; `summoning_sick_creature_cannot_activate_granted_ability`: `:496`; `granted_ability_resolves_after_equipment_destroyed_in_response`: `:530`; `granted_ability_resolves_after_equipped_creature_destroyed_in_response`: `:593` |
| Webweaver Changeling | true | `etb_no_life_gain_below_three_creature_cards`, `etb_gains_five_life_with_three_or_more_creature_cards`, `intervening_if_rechecked_on_resolution`: `pauper_meta_w1_creatures.rs:225 webweaver_changeling_gains_five_only_with_three_creature_cards_in_graveyard`; `changeling_counts_as_every_creature_type`: `:309` |

No branch is listed under `unreachable` for this wave: every declared
branch is reachable and tested within the existing Pauper pool.

## 3. One-seed bit-identical rerun (Step 5)

Lead ruling on what "the standing small run" is: the documented smoke in
`mtg-kernel/examples/rollout_record.rs`.

Branch worktree (`lead/pauper-meta-cards-v1`, this record's commit range):

```
CARGO_TARGET_DIR=E:/cargo-target-pauper-meta cargo run --locked -p mtg-kernel --release --example rollout_record -- --matchup burn_mirror --games 4 --seed 5151 --out E:/pauper-meta-parity/branch_seed5151
```

Base tree (`C:\Users\Jack\IdeaProjects\mtg-kernel`, `lead/cycle4-refresh-manifest-v1` at `75406ffe`, read-only):

```
CARGO_TARGET_DIR=E:/cargo-target-pauper-meta-base cargo run --locked -p mtg-kernel --release --example rollout_record -- --matchup burn_mirror --games 4 --seed 5151 --out E:/pauper-meta-parity/base_seed5151
```

Both runs: `p0_wins=2 p1_wins=2 draws=0 truncated=0 halted=0 policy_steps=499 physical_decisions=496`, 507 audit records and 507 policy records, 4 games.

### Raw sha256sum (includes the expected identity fields, see below)

Branch (`E:/pauper-meta-parity/branch_seed5151/`):

```
4cebf2d8ce112607e8524faca8a78688bcefd7fb6dcb2415ce0229e95fb967f2  audit_episodes.jsonl
cd533ce4433a355b064fae6e01fd21456ab39bef4b3938f28b810572e469b31e  policy_episodes.jsonl
a0efb11cde12023f5754b3da4f14c81c7b04df3850b0032a3c55371d7834afad  manifest.json
```

Base (`E:/pauper-meta-parity/base_seed5151/`):

```
5863478f4d8dcf66f021027da992de722c899bde2f12c786098d1a4ff6277776  audit_episodes.jsonl
f8d871a5f895ed6ae423468dca3c1c2d5323308e258846a4e8fb92354f891f11  policy_episodes.jsonl
2df793ee13c0aa015b4ead9fb067d1116274ee63b8d71a642b8a5ea22a9a163e  manifest.json
```

The raw sha256sums differ, as expected: the raw bytes embed the identity
fields excluded below, most visibly `card_db_hash`: branch
`0xde59c501e943f3fd` (`KERNEL_CARDDB_HASH` frozen post-wave-1, see
`card_def.rs` `card_db_hash_v33_is_frozen`) versus base
`0x64c82a261e078f1a` (the pre-wave-1 hash, Task 5's starting value), plus
`git.commit`/`git.dirty`, which are trivially the two different commits.

### JSON-aware diff

A JSON-aware diff walked every header, decision, and terminal record in
both `audit_episodes.jsonl` and `policy_episodes.jsonl` (507 records each)
plus `manifest.json`, recursively comparing every field except an excluded
set, per Addendum guidance:

Excluded fields (present in both, differ for a known, documented reason,
not compared):

- `diagnostic_state_hash`, `environment_hash`: state hashes. Per the
  Addendum, `GameState` gained `monarch: Option<PlayerId>` this wave,
  which is folded into these hashes, so every pinned/computed state hash
  moved even when the underlying game state (for a monarch-free matchup
  like Burn mirror) did not.
- `card_db_hash` (wherever nested: episode header, each decision's
  `observation`, and the manifest), `observation_projection_hash` (each
  decision record's own top-level field), and
  `observation.visible_projection_hash` (nested inside each decision's
  `observation`): all three belong to the card-DB-hash cause, not the
  monarch-field cause. `rl.rs:2729` sets
  `observation_projection_hash: observation.visible_projection_hash`
  directly, a literal copy, and `rl.rs:3332-3336` asserts the two stay
  equal on every audit record, so `observation_projection_hash` is not an
  independent field: it is `observation.visible_projection_hash` under a
  second name. `visible_projection_hash_v5` (`rl.rs:6264-6300`) hashes an
  `ObservationHashInput` that explicitly includes `card_db_hash`, so both
  names move purely because the registry's card count changed, not
  because of any observed content difference (confirmed below: the
  `projection`, `own_hand`, `known_library_cards`, and `known_hand_cards`
  fields the hash is computed over are byte-identical). Branch
  `16022053761346434045` (`0xde59c501e943f3fd`) versus base
  `7262100742335860506` (`0x64c82a261e078f1a`).
- `git` (manifest): the two trees are, by construction, two different
  commits (`27001b793b8a1917a45f7981fa8494d2fa02789e` dirty vs.
  `75406ffe3a5363491e889d3df177c50f5e29faf5` clean); not a rules field.
- `variable_metadata`, `cli_args` (manifest): contain the `--out` path,
  which was intentionally different between the two invocations
  (`E:/pauper-meta-parity/branch_seed5151` vs. `.../base_seed5151`); not a
  rules field.

Everything else was compared exactly, including: every `legal_actions`
entry's `action_kind`/`actor`/`source`/targets/choice arguments (the full
`semantic` object, not just the selected one), `selected_index` and
`stable_id` (a content hash of `semantic` alone: it does **not** embed
`card_db_hash`, so a match here is a second, independent confirmation that
the actions themselves are identical), decision counts per episode (499
decisions + 4 terminals per side), legal-action counts per decision,
`terminal_outcome`/`winner`/`terminal_reason`/`terminal_classification`/
`terminal_reward`, `projection.life_totals` and every other projection
field, `seed` (env/policy seeds derived from the shared base seed 5151),
and `deck.deck_hashes` (`6907250323055407041` on both sides, confirming
the fixed Burn mainboard deck used by `burn_mirror` is untouched by the
wave 1 registry growth, as expected since new cards were appended, not
inserted).

**Verdict: MATCH.** Zero differences outside the excluded identity fields
across all 1,014 audit+policy records and the manifest. No kernel
divergence.

## 4. Certification status

**kernel-covered, not XMage-shadow-certified.** This record establishes
that every wave 1 card branch is exercised by a passing kernel test, and
that the kernel's rules engine did not regress between the pre-wave-1 base
(`75406ffe`) and the wave-1 branch tip for the standing one-seed smoke.
It does **not** establish agreement with XMage's own implementation of
these cards (the Oracle authority used to derive the branch list was
XMage's Java source, read by hand, not run). That is Task 14b: the XMage
shadow harness generalization plan, not yet written (the CP7 shadow
harness as it exists today cannot play the V2 registrations; see the lead
ruling of 2026-09-10 in the Task 14 research ledger entry).

See also `docs/research/pauper_meta_wave1_record_2026-09.md` (Task 13's
wave record), which is amended to link here.
