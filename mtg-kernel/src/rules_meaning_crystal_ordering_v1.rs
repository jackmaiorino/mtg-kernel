//! Exact selected Crystal rules and the separately scoped ordering interpreter.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/rules-meaning/crystal-ordering/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV1 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
    pub printed_static_roots: Vec<crate::rules_meaning_crystal_barricade_v1::MeaningV1>,
    pub fixed_damage_sequence_and_conditional_supported: bool,
    pub damage_completion_requires_actual_prepare_and_result: bool,
    pub authenticated_historical_source_power_supported: bool,
    pub firebrand_fixed_damage_captures_actual_tap_then_sacrifice_facts: bool,
    pub firebrand_payment_and_removed_owner_are_mandatory: bool,
    pub following_direct_player_life_gain_supported: bool,
    pub post_damage_optional_one_discard_then_one_draw_supported: bool,
    pub chosen_creature_power_after_actual_counter_and_target_recheck_supported: bool,
    pub all_creature_batch_uses_prebatch_scene_and_apnap_choices: bool,
    pub all_creature_batch_commits_damage_after_all_choices: bool,
    pub bushwhack_fight_captures_both_legal_incarnations_and_current_powers: bool,
    pub fight_packets_commit_simultaneously_after_apnap_choices: bool,
    pub batch_requires_independent_actual_preparation: bool,
    pub indrik_choice_uses_original_controller_and_independent_accepted_answer: bool,
    pub indrik_fight_requires_original_source_creature_incarnation: bool,
    pub classified_nondamage_and_ward_keep_existing_interpreter: bool,
    pub affected_current_recipient_controller_chooses: bool,
    pub each_choice_rechecks_applicability_without_priority: bool,
    pub shield_consumed_only_when_selected: bool,
    pub other_synchronous_producers_refused_before_mutation: bool,
    pub full_mixed_producer_coverage: bool,
    pub model_and_sampler_admitted: bool,
}
pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut h = Sha256::new();
        for s in [
            SCHEMA,
            include_str!("rules_meaning_crystal_ordering_v1.rs"),
            include_str!("rules_meaning_crystal_barricade_v1.rs"),
            include_str!("crystal_barricade_v1.rs"),
            include_str!("effect/crystal_prevention_v1.rs"),
            include_str!("effect.rs"),
            include_str!("source_power_activation_v1.rs"),
            include_str!("gorehorn_crystal_v1.rs"),
            include_str!("gorehorn_damage_v1.rs"),
            include_str!("raid_entry_custody_v1.rs"),
            include_str!("fixed_source_damage_v3.rs"),
            include_str!("equipment_damage_source_v4.rs"),
            include_str!("rules_meaning_heartfire_v1.rs"),
            include_str!("creature_stats_lki_v1.rs"),
            include_str!("standard_legends_v1.rs"),
            include_str!("standard_cards_v1.rs"),
            include_str!("event.rs"),
            include_str!("engine.rs"),
            include_str!("state.rs"),
            include_str!("card_def.rs"),
            include_str!("../build.rs"),
            include_str!("bloodthirsty_conqueror_v49/custody.rs"),
            include_str!("linked_aura_operation_v1.rs"),
            include_str!("counter_target_effects_v1.rs"),
            include_str!("zimone_paradox_sculptor_v1.rs"),
            include_str!("standard_creatures_v1.rs"),
            include_str!("planeswalker_v1.rs"),
            include_str!("engine/omniscience_cast_v1.rs"),
            include_str!("resolution_cast_v1.rs"),
            include_str!("effect/harvester_v1.rs"),
            include_str!("leyline_axe_v1.rs"),
            include_str!("london_mulligan_v1.rs"),
            include_str!("life_loss_turn_v1.rs"),
            include_str!("soul_shackled_zombie_v1.rs"),
            include_str!("progenitus_v1.rs"),
            include_str!("controlled_creature_grants_v1.rs"),
            include_str!("effect/nonland_copy_v1.rs"),
            include_str!("rl_session/flat_action_v4/search_state/effect_refs.rs"),
            include_str!("effect/inspiration_resolution_v1.rs"),
            include_str!("effect/standard_exile.rs"),
            include_str!("snack_wide_life_v59.rs"),
            include_str!("snack_wide_life_v59/custody.rs"),
            include_str!("bloodthirsty_conqueror_v49.rs"),
            include_str!("fiendish_panda_v1.rs"),
            include_str!("fiendish_panda_v1/custody.rs"),
            include_str!("life_gain_turn_v1.rs"),
            include_str!("midnight_snack_v1.rs"),
            include_str!("continuous_characteristics_v1.rs"),
            include_str!("mana.rs"),
            include_str!("legend_rule_v1.rs"),
            include_str!("doubling_season_v48.rs"),
            include_str!("rl.rs"),
        ] {
            h.update([0]);
            h.update(s.as_bytes());
        }
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}
pub fn selected() -> Result<MeaningV1, String> {
    use crate::rules_meaning_crystal_barricade_v1::{live_selector, meaning, StaticRootV1};
    let selector = live_selector().map_err(|e| format!("{e:?}"))?;
    let id = crate::card_def::card_id_by_name("Crystal Barricade")
        .ok_or("Crystal remains unregistered")?;
    let roots = [
        StaticRootV1::ControllerHexproof,
        StaticRootV1::OtherControlledCreatureNoncombatPrevention,
    ]
    .into_iter()
    .map(|r| meaning(&selector, id, r).map_err(|e| format!("{e:?}")))
    .collect::<Result<Vec<_>, String>>()?;
    Ok(MeaningV1 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256_v1(),
        printed_static_roots: roots,
        fixed_damage_sequence_and_conditional_supported: true,
        damage_completion_requires_actual_prepare_and_result: true,
        authenticated_historical_source_power_supported: true,
        firebrand_fixed_damage_captures_actual_tap_then_sacrifice_facts: true,
        firebrand_payment_and_removed_owner_are_mandatory: true,
        following_direct_player_life_gain_supported: true,
        post_damage_optional_one_discard_then_one_draw_supported: true,
        chosen_creature_power_after_actual_counter_and_target_recheck_supported: true,
        all_creature_batch_uses_prebatch_scene_and_apnap_choices: true,
        all_creature_batch_commits_damage_after_all_choices: true,
        bushwhack_fight_captures_both_legal_incarnations_and_current_powers: true,
        fight_packets_commit_simultaneously_after_apnap_choices: true,
        batch_requires_independent_actual_preparation: true,
        indrik_choice_uses_original_controller_and_independent_accepted_answer: true,
        indrik_fight_requires_original_source_creature_incarnation: true,
        classified_nondamage_and_ward_keep_existing_interpreter: true,
        affected_current_recipient_controller_chooses: true,
        each_choice_rechecks_applicability_without_priority: true,
        shield_consumed_only_when_selected: true,
        other_synchronous_producers_refused_before_mutation: true,
        full_mixed_producer_coverage: false,
        model_and_sampler_admitted: false,
    })
}
pub fn validate(record: &MeaningV1) -> Result<(), String> {
    if *record != selected()? {
        return Err("Crystal ordering meaning changed".into());
    }
    Ok(())
}
