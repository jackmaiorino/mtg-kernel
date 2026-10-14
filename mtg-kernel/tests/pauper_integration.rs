//! Consolidated integration tests; CI runs each module in its own process.

#[path = "pauper/affinity_wildfire_value.rs"]
mod affinity_wildfire_value;
#[path = "pauper/artifact_control.rs"]
mod artifact_control;
#[path = "pauper/artifact_equipment_completion.rs"]
mod artifact_equipment_completion;
#[path = "pauper/async_rollout_v2_legacy_golden.rs"]
mod async_rollout_v2_legacy_golden;
#[path = "pauper/basic_land_substrate.rs"]
mod basic_land_substrate;
#[path = "pauper/blue_blasts.rs"]
mod blue_blasts;
#[path = "pauper/bo3_match_v1.rs"]
mod bo3_match_v1;
#[path = "pauper/bo3_session_v1.rs"]
mod bo3_session_v1;
#[path = "pauper/brainstorm.rs"]
mod brainstorm;
#[path = "pauper/burn_combat.rs"]
mod burn_combat;
#[path = "pauper/burn_goldfish.rs"]
mod burn_goldfish;
#[path = "pauper/caw_gates_completion_v1.rs"]
mod caw_gates_completion_v1;
#[path = "pauper/caw_gates_future_v1.rs"]
mod caw_gates_future_v1;
#[path = "pauper/cryptic_serpent.rs"]
mod cryptic_serpent;
#[path = "pauper/cycle4_m3_audit_cli_v1.rs"]
mod cycle4_m3_audit_cli_v1;
#[path = "pauper/deem_inferior.rs"]
mod deem_inferior;
#[path = "pauper/deep_analysis.rs"]
mod deep_analysis;
#[path = "pauper/elves_tribal.rs"]
mod elves_tribal;
#[path = "pauper/faerie_entry_v1.rs"]
mod faerie_entry_v1;
#[path = "pauper/faeries_future_v1.rs"]
mod faeries_future_v1;
#[path = "pauper/fast_sampler.rs"]
mod fast_sampler;
#[path = "pauper/final_pool_completion_v1.rs"]
mod final_pool_completion_v1;
#[path = "pauper/final_pool_initiative_v1.rs"]
mod final_pool_initiative_v1;
#[path = "pauper/flat_policy_v1.rs"]
mod flat_policy_v1;
#[path = "pauper/gut_shot_ichor_wellspring.rs"]
mod gut_shot_ichor_wellspring;
#[path = "pauper/kshadow_combat_departure.rs"]
mod kshadow_combat_departure;
#[path = "pauper/kshadow_continuous_effects.rs"]
mod kshadow_continuous_effects;
#[path = "pauper/kshadow_journey.rs"]
mod kshadow_journey;
#[path = "pauper/kshadow_ninjutsu.rs"]
mod kshadow_ninjutsu;
#[path = "pauper/kshadow_resolving_stack.rs"]
mod kshadow_resolving_stack;
#[path = "pauper/kshadow_subtype_case.rs"]
mod kshadow_subtype_case;
#[path = "pauper/landcycling_omen.rs"]
mod landcycling_omen;
#[path = "pauper/learned_bo3_configurations_v1.rs"]
mod learned_bo3_configurations_v1;
#[path = "pauper/lorien_revealed.rs"]
mod lorien_revealed;
#[path = "pauper/mana_permanents.rs"]
mod mana_permanents;
#[path = "pauper/mill_cards.rs"]
mod mill_cards;
#[path = "pauper/murmuring_mystic.rs"]
mod murmuring_mystic;
#[path = "pauper/pauper_counterspells.rs"]
mod pauper_counterspells;
#[path = "pauper/pauper_green_utility.rs"]
mod pauper_green_utility;
#[path = "pauper/pauper_green_value.rs"]
mod pauper_green_value;
#[path = "pauper/pauper_interaction_creatures_v1.rs"]
mod pauper_interaction_creatures_v1;
#[path = "pauper/pauper_meta_w1_adventure_and_monarch.rs"]
mod pauper_meta_w1_adventure_and_monarch;
#[path = "pauper/pauper_meta_w1_black_red.rs"]
mod pauper_meta_w1_black_red;
#[path = "pauper/pauper_meta_w1_creatures.rs"]
mod pauper_meta_w1_creatures;
#[path = "pauper/pauper_meta_w1_delve_and_longbow.rs"]
mod pauper_meta_w1_delve_and_longbow;
#[path = "pauper/pauper_meta_w1_delver.rs"]
mod pauper_meta_w1_delver;
#[path = "pauper/pauper_meta_w1_snuff_out_and_duals.rs"]
mod pauper_meta_w1_snuff_out_and_duals;
#[path = "pauper/pauper_meta_w1_spells.rs"]
mod pauper_meta_w1_spells;
#[path = "pauper/pauper_meta_w2_lands_and_rocks.rs"]
mod pauper_meta_w2_lands_and_rocks;
#[path = "pauper/pauper_meta_w2_tron.rs"]
mod pauper_meta_w2_tron;
#[path = "pauper/pauper_modal_hate.rs"]
mod pauper_modal_hate;
#[path = "pauper/pauper_optional_costs.rs"]
mod pauper_optional_costs;
#[path = "pauper/pauper_simple_draw_lands_v1.rs"]
mod pauper_simple_draw_lands_v1;
#[path = "pauper/piracy_charm.rs"]
mod piracy_charm;
#[path = "pauper/ponder.rs"]
mod ponder;
#[path = "pauper/preordain.rs"]
mod preordain;
#[path = "pauper/rally_goldfish.rs"]
mod rally_goldfish;
#[path = "pauper/removal_counterspell_tags_v1.rs"]
mod removal_counterspell_tags_v1;
#[path = "pauper/rl_contract.rs"]
mod rl_contract;
#[path = "pauper/rl_session.rs"]
mod rl_session;
#[path = "pauper/sideboard_v1.rs"]
mod sideboard_v1;
#[path = "pauper/sleep_of_the_dead.rs"]
mod sleep_of_the_dead;
#[path = "pauper/spy_combo_core.rs"]
mod spy_combo_core;
#[path = "pauper/spy_mana.rs"]
mod spy_mana;
#[path = "pauper/spy_sideboard_cards.rs"]
mod spy_sideboard_cards;
#[path = "pauper/t1_diagnosis_mechanics_v1.rs"]
mod t1_diagnosis_mechanics_v1;
#[path = "pauper/tolarian_terror.rs"]
mod tolarian_terror;
#[path = "pauper/wildfire_utility_future_v1.rs"]
mod wildfire_utility_future_v1;
#[path = "pauper/winding_way.rs"]
mod winding_way;
#[path = "pauper/xmage_counter_reference_windows.rs"]
mod xmage_counter_reference_windows;
