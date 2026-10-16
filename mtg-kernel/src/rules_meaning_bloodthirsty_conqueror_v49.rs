//! Exact printed loss-batch semantics, separate from all frozen vectors/models.
use crate::effect::{EffectOp, ObjectRef};
use crate::state::Zone;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/rules-meaning/bloodthirsty-conqueror/v49";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorV49 {
    pub schema: String,
    pub card_db_hash: u64,
    pub producer_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootV49 {
    CreatureSpell,
    OpponentLifeLossBatch,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV49 {
    pub selector: SelectorV49,
    pub card_id: u16,
    pub root: RootV49,
    pub root_sha256: String,
    pub ordered_facts: Vec<String>,
    pub frozen_public_model_sampler_admitted: bool,
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut h = Sha256::new();
        for text in [
            SCHEMA,
            include_str!("rules_meaning_bloodthirsty_conqueror_v49.rs"),
            include_str!("bloodthirsty_conqueror_v49.rs"),
            include_str!("bloodthirsty_conqueror_v49/custody.rs"),
            include_str!("event.rs"),
            include_str!("trigger.rs"),
            include_str!("effect.rs"),
            include_str!("engine.rs"),
            include_str!("state.rs"),
            include_str!("card_def.rs"),
            include_str!("standard_cards_v1.rs"),
            include_str!("standard_statics_v1.rs"),
            include_str!("continuous_characteristics_v1.rs"),
            include_str!("effect/crystal_prevention_v1.rs"),
            include_str!("life_gain_turn_v1.rs"),
            include_str!("fiendish_panda_v1.rs"),
            include_str!("fiendish_panda_v1/custody.rs"),
            include_str!("rl_session/flat_action_v4/search_state/effect_refs.rs"),
            include_str!("rl_session/flat_action_v4/search_state/sampler.rs"),
            include_str!("../build.rs"),
            include_str!("counter_target_effects_v1.rs"),
            include_str!("zimone_paradox_sculptor_v1.rs"),
            include_str!("standard_creatures_v1.rs"),
            include_str!("planeswalker_v1.rs"),
        ] {
            h.update([0]);
            h.update(text.as_bytes());
        }
        h.finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    })
    .clone()
}
pub fn live_selector() -> Result<SelectorV49, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("Conqueror precise meaning requires actual Limited catalog".into());
    }
    Ok(SelectorV49 {
        schema: SCHEMA.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256(),
    })
}
pub fn meaning(selector: &SelectorV49, card_id: u16, root: RootV49) -> Result<MeaningV49, String> {
    if *selector != live_selector()? {
        return Err("Conqueror precise selector changed".into());
    }
    if !crate::bloodthirsty_conqueror_v49::owns_definition(card_id) {
        return Err(
            "Conqueror requires genuine exact Partial or Full definition and original root".into(),
        );
    }
    let (program, ordered_facts) = match root {
        RootV49::CreatureSpell => (
            EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            },
            vec![
                "pay_3BB_ordinary_creature_timing",
                "black_5_5_vampire_knight",
                "flying_and_deathtouch",
                "this_spell_enters_battlefield",
            ],
        ),
        RootV49::OpponentLifeLossBatch => (
            crate::bloodthirsty_conqueror_v49::recipe(),
            vec![
                "mandatory_battlefield_original_active_printed_observer",
                "actual_positive_committed_player_loss",
                "damage_replacements_and_prevention_before_loss_capture",
                "one_occurrence_per_losing_opponent_per_genuine_operation",
                "sum_actual_loss_members_not_proposed_amount_or_turn_boolean",
                "capture_post_event_original_source_and_controller_before_SBA",
                "normal_APNAP_publication_and_controller_trigger_order",
                "unique_full_pending_and_published_owners",
                "original_private_prepop_removal_input",
                "gain_saved_actual_amount_for_original_controller",
                "source_departure_reentry_or_control_change_does_not_change_beneficiary_or_amount",
                "actual_gain_commits_normal_gain_observers",
                "countered_original_root_has_no_gain",
                "targetless_original_root_cannot_fizzle",
                "independent_actual_gain_operation_and_unique_closed_completion",
            ],
        ),
    };
    let mut h = Sha256::new();
    h.update(SCHEMA);
    h.update([0]);
    h.update(
        serde_json::to_vec(&(card_id, root, program, &ordered_facts)).map_err(|e| e.to_string())?,
    );
    Ok(MeaningV49 {
        selector: selector.clone(),
        card_id,
        root,
        root_sha256: h
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        ordered_facts: ordered_facts.into_iter().map(str::to_owned).collect(),
        frozen_public_model_sampler_admitted: false,
    })
}
pub fn validate(selector: &SelectorV49, artifact: &MeaningV49) -> Result<(), String> {
    if *artifact != meaning(selector, artifact.card_id, artifact.root)? {
        return Err("Conqueror precise meaning forged".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn foreign_definition_or_catalog_never_substitutes_for_printed_conqueror() {
        let card = crate::card_def::card_id_by_name("Forest").unwrap();
        let selected = live_selector();
        #[cfg(all(
            feature = "limited-fdn-fixtures",
            not(feature = "standard-magezero-fixtures")
        ))]
        assert!(selected.is_ok());
        #[cfg(not(all(
            feature = "limited-fdn-fixtures",
            not(feature = "standard-magezero-fixtures")
        )))]
        assert!(selected.is_err());
        if let Ok(selector) = selected {
            for root in [RootV49::CreatureSpell, RootV49::OpponentLifeLossBatch] {
                assert!(meaning(&selector, card, root).is_err());
                let mut wrong = selector.clone();
                wrong.card_db_hash ^= 1;
                assert!(meaning(&wrong, card, root).is_err());
                let mut wrong = selector.clone();
                wrong.producer_sha256.clear();
                assert!(meaning(&wrong, card, root).is_err());
            }
        }
    }
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn genuine_printed_meaning_all_field_order_and_unknown_field_forgeries_refuse() {
        let card = crate::card_def::card_id_by_name("Bloodthirsty Conqueror").unwrap();
        let selector = live_selector().unwrap();
        for root in [RootV49::CreatureSpell, RootV49::OpponentLifeLossBatch] {
            let real = meaning(&selector, card, root).unwrap();
            validate(&selector, &real).unwrap();
            let mut forged = real.clone();
            forged.ordered_facts.reverse();
            assert!(validate(&selector, &forged).is_err());
            let mut forged = real.clone();
            forged.root_sha256.clear();
            assert!(validate(&selector, &forged).is_err());
            let mut forged = real.clone();
            forged.frozen_public_model_sampler_admitted = true;
            assert!(validate(&selector, &forged).is_err());
            let mut unknown = serde_json::to_value(real).unwrap();
            unknown["unrepresented"] = serde_json::Value::Bool(true);
            assert!(serde_json::from_value::<MeaningV49>(unknown).is_err());
        }
    }
}
