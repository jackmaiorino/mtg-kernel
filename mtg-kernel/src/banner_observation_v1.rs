//! Separate public Banner transport. This is not V8/V9 or tensor/model admission.
use crate::ids::PlayerId;
use crate::policy_observation_v8::PolicyObservationExtensionsV8;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, CardStableRefV1, KnownLibraryCardV4, PlayerSeatV1,
    PublicObservationProjectionV5, RlContractError,
};
use crate::state::GameState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA: &str = "mtg-kernel/banner-observation/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootV1 {
    pub card_id: u16,
    pub root: crate::rules_meaning_heraldic_banner_v1::RootV1,
    pub root_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BannerObservationV1 {
    pub schema: String,
    pub producer_sha256: String,
    pub kernel_version: String,
    pub surface_version: u32,
    pub policy_surface_version: u32,
    pub card_db_hash: u64,
    pub acting_player: PlayerSeatV1,
    pub step_index: u64,
    pub physical_decision_id: u64,
    pub substep_index: u32,
    pub substep_count: u32,
    pub projection: PublicObservationProjectionV5,
    pub own_hand: Vec<CardPrivateV1>,
    pub known_library_cards: [Vec<KnownLibraryCardV4>; 2],
    pub known_hand_cards: [Vec<CardPrivateV1>; 2],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known_face_down_cards: Vec<crate::rl::FaceDownCardKnowledgeV1>,
    pub extensions: PolicyObservationExtensionsV8,
    pub attacked_this_turn: [bool; 2],
    pub entry_choice: Option<EntryChoiceV36>,
    pub chosen_colors: Vec<ChosenColorV36>,
    pub prior_entry_colors: Vec<PriorEntryColorV36>,
    pub roots: Vec<RootV1>,
    pub projection_sha256: String,
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("banner_observation_v1.rs"),
        include_str!("rules_meaning_heraldic_banner_v1.rs"),
        include_str!("linked_aura_operation_v1.rs"),
        include_str!("linked_aura_return_v1.rs"),
        include_str!("heraldic_banner_v1.rs"),
        include_str!("effect/crystal_prevention_v1.rs"),
        include_str!("source_power_activation_v1.rs"),
        include_str!("gorehorn_damage_v1.rs"),
        include_str!("equipment_damage_source_v4.rs"),
        include_str!("banishing_light_v1.rs"),
        include_str!("authority_of_the_consuls_v1.rs"),
        include_str!("garruks_uprising_v1.rs"),
        include_str!("arahbo_v1.rs"),
        include_str!("effect/squad_rallier_resolution_v1.rs"),
        include_str!("raid_entry_v1.rs"),
        include_str!("raid_entry_custody_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
        include_str!("trigger.rs"),
        include_str!("effect.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("extra_turn_v1.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("rl.rs"),
        include_str!("rl_session.rs"),
        include_str!("rl_session/banner_v36.rs"),
        include_str!("human_bo3_v1/banner_v36.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("human_bo3_v1/life_ward_v8.rs"),
        include_str!("policy_observation_v8.rs"),
        include_str!("policy_observation_v7.rs"),
        include_str!("rl_session/flat_action_v3.rs"),
        include_str!("rl_session/flat_action_v4.rs"),
        include_str!("policy_surface_v5.rs"),
        include_str!("surface_v2.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("standard_statics_v1.rs"),
        include_str!("standard_keywords_v1.rs"),
        include_str!("standard_legends_v1.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn commitment(observation: &BannerObservationV1) -> Result<String, RlContractError> {
    let mut value = serde_json::to_value(observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("Banner projection is not an object".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&value)?))
}
impl BannerObservationV1 {
    /// Checks transport identity only. Execution always rederives the entire
    /// observation and private ordered candidate binding from the owning session.
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.producer_sha256 != producer_sha256_v1()
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong Banner artifact identity".into()));
        }
        let expected = roots()?;
        if self.roots != expected {
            return Err(RlContractError("wrong Banner printed root".into()));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryChoiceV36 {
    pub entry_ordinal: u32,
    pub chooser: PlayerSeatV1,
    pub colors: [crate::mana::ManaColor; 5],
    pub printed_card_db_id: u16,
}
/// Already announced answers from the current pre-entry operation, in APNAP answer order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriorEntryColorV36 {
    pub entry_ordinal: u32,
    pub chooser: PlayerSeatV1,
    pub printed_card_db_id: u16,
    pub chosen_color: crate::mana::ManaColor,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChosenColorV36 {
    pub source: CardStableRefV1,
    pub chosen_color: crate::mana::ManaColor,
    pub active_printed_abilities: bool,
}
fn roots() -> Result<Vec<RootV1>, RlContractError> {
    use crate::rules_meaning_heraldic_banner_v1::{self as meaning, RootV1 as Root};
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Ok(vec![]);
    }
    let Some(id) = crate::card_def::card_id_by_name("Heraldic Banner") else {
        return Ok(vec![]);
    };
    let selector = meaning::live_selector()
        .map_err(|_| RlContractError("Banner selector unsupported".into()))?;
    [
        Root::PermanentSpell,
        Root::AsEntersColor,
        Root::ChosenColorAnthem,
        Root::ChosenColorMana,
    ]
    .into_iter()
    .map(|root| {
        meaning::meaning(&selector, id, root)
            .map(|m| RootV1 {
                card_id: id,
                root,
                root_sha256: m.root_sha256,
            })
            .map_err(|_| RlContractError("Banner genuine root unsupported".into()))
    })
    .collect()
}
pub(crate) fn validate_projection_authority(state: &GameState) -> Result<(), RlContractError> {
    crate::heraldic_banner_v1::validate_state(state).map_err(RlContractError)?;
    crate::effect::validate_pending_effect_choice(state).map_err(RlContractError)?;
    if let Some(pending) = state
        .engine
        .linked_aura_operation_v1
        .pending_general_v36
        .as_ref()
    {
        if !matches!(
            pending.choice,
            crate::linked_aura_operation_v1::GeneralChoiceV36::Color { .. }
        ) {
            return Err(RlContractError(
                "Banner V36 does not admit linked Aura owner transport".into(),
            ));
        }
    }
    Ok(())
}
fn entry_choice(state: &GameState) -> Result<Option<EntryChoiceV36>, RlContractError> {
    let Some(pending) = state
        .engine
        .linked_aura_operation_v1
        .pending_general_v36
        .as_ref()
    else {
        return Ok(None);
    };
    let crate::linked_aura_operation_v1::GeneralChoiceV36::Color { choice, .. } = &pending.choice
    else {
        return Err(RlContractError(
            "Banner selected choice has a foreign family".into(),
        ));
    };
    Ok(Some(EntryChoiceV36 {
        entry_ordinal: choice.ordinal,
        chooser: choice.controller.into(),
        colors: crate::heraldic_banner_v1::COLORS,
        printed_card_db_id: choice.card_def,
    }))
}
fn prior_entry_colors(state: &GameState) -> Result<Vec<PriorEntryColorV36>, RlContractError> {
    use crate::event::CommittedEvent;
    use crate::linked_aura_operation_v1::GeneralAnswerV36;
    let Some(pending) = state
        .engine
        .linked_aura_operation_v1
        .pending_general_v36
        .as_ref()
    else {
        return Ok(Vec::new());
    };
    // Whole-checkpoint replay already authenticates the original operation and
    // ordered answers. Also bind each exported answer to its committed capture.
    pending
        .answers
        .iter()
        .map(|answer| {
            let GeneralAnswerV36::Color { choice, color } = answer else {
                return Err(RlContractError(
                    "Banner V36 refuses foreign prior owner answers".into(),
                ));
            };
            let captures = state
                .engine
                .event_history
                .iter()
                .enumerate()
                .filter_map(|(index, event)| match event {
                    CommittedEvent::BannerEntryColorCapturedV1 {
                        operation,
                        choice: captured,
                    } if *operation == pending.operation_history_index
                        && captured.as_ref() == choice =>
                    {
                        Some(index)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            let [capture] = captures.as_slice() else {
                return Err(RlContractError(
                    "Banner prior answer lost unique original capture".into(),
                ));
            };
            let answers = state
                .engine
                .event_history
                .iter()
                .enumerate()
                .filter(|(index, event)| {
                    *index > *capture
                        && matches!(event, CommittedEvent::BannerEntryColorChosenV1 {
                capture: original, color: chosen
            } if *original as usize == *capture && chosen == color)
                })
                .count();
            if answers != 1 || !crate::heraldic_banner_v1::COLORS.contains(color) {
                return Err(RlContractError(
                    "Banner prior answer lost actual announced color".into(),
                ));
            }
            Ok(PriorEntryColorV36 {
                entry_ordinal: choice.ordinal,
                chooser: choice.controller.into(),
                printed_card_db_id: choice.card_def,
                chosen_color: *color,
            })
        })
        .collect()
}
fn chosen_colors(state: &GameState) -> Result<Vec<ChosenColorV36>, RlContractError> {
    state
        .objects
        .iter()
        .filter(|(_, o)| {
            o.zone == crate::state::Zone::Battlefield
                && crate::heraldic_banner_v1::owns_definition(o.card_def)
        })
        .map(|(source, _)| {
            Ok(ChosenColorV36 {
                source: crate::rl::card_ref(state, source)?,
                chosen_color: crate::heraldic_banner_v1::authenticated_color(state, source)
                    .map_err(RlContractError)?,
                active_printed_abilities:
                    crate::continuous_characteristics_v1::printed_abilities_active(state, source),
            })
        })
        .collect()
}
#[allow(clippy::too_many_arguments)]
pub fn observe_v1(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
) -> Result<BannerObservationV1, RlContractError> {
    validate_projection_authority(state)?;
    let base = crate::rl::banner_observation_storage_v1(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let base = storage.base;
    let mut observation = BannerObservationV1 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256_v1(),
        kernel_version: base.kernel_version,
        surface_version: base.surface_version,
        policy_surface_version: base.policy_surface_version,
        card_db_hash: base.card_db_hash,
        acting_player: base.acting_player,
        step_index: base.step_index,
        physical_decision_id: base.physical_decision_id,
        substep_index: base.substep_index,
        substep_count: base.substep_count,
        projection: base.projection,
        own_hand: base.own_hand,
        known_library_cards: base.known_library_cards,
        known_hand_cards: base.known_hand_cards,
        known_face_down_cards: base.known_face_down_cards,
        extensions: storage.extensions,
        attacked_this_turn: [
            state.player_attacked_this_turn_v1(PlayerId::P0),
            state.player_attacked_this_turn_v1(PlayerId::P1),
        ],
        entry_choice: entry_choice(state)?,
        chosen_colors: chosen_colors(state)?,
        prior_entry_colors: prior_entry_colors(state)?,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    observation.projection_sha256 = commitment(&observation)?;
    Ok(observation)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absence_retains_genuine_predecessor_public_and_private_payload_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let state = crate::policy_observation_v6::tests::ready_state();
            let surface = PolicySurfaceV5::new_for_session();
            let bytes = serde_json::to_vec(&state).unwrap();
            let old = crate::rl::observe_policy_v8(&state, &surface, actor, 0, 0, 0, 1).unwrap();
            let new = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
            assert_eq!(new.projection, old.projection);
            assert_eq!(new.own_hand, old.own_hand);
            assert_eq!(new.known_library_cards, old.known_library_cards);
            assert_eq!(new.known_hand_cards, old.known_hand_cards);
            assert_eq!(new.known_face_down_cards, old.known_face_down_cards);
            assert_eq!(new.extensions, old.extensions);
            assert!(
                new.entry_choice.is_none()
                    && new.chosen_colors.is_empty()
                    && new.prior_entry_colors.is_empty()
            );
            new.validate_artifact().unwrap();
            assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
        }
    }
}
