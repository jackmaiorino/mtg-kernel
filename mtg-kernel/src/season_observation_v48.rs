//! Distinct Season V48 mixed-family transport. Predecessor schemas retain refusal.
use crate::ids::PlayerId;
use crate::policy_observation_v8::PolicyObservationExtensionsV8;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, KnownLibraryCardV4, PlayerSeatV1, PublicObservationProjectionV5, RlContractError,
};
use crate::state::GameState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/season-observation/v48";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeasonObservationV48 {
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
    pub grants: Vec<crate::akroma_public_v1::ActiveGrantV1>,
    pub families: Vec<crate::akroma_family_public_v40::FamilyFactsV40>,
    pub color_departures: Vec<crate::akroma_public_v1::ColorDepartureV1>,
    pub roots: Vec<crate::akroma_public_v1::RootV1>,
    pub projection_sha256: String,
    pub divine: crate::divine_public_v44::FactsV44,
    pub hunter: crate::high_society_hunter_public_v45::FactsV45,
    pub eaten: crate::eaten_public_v46::FactsV46,
    pub panda: crate::panda_public_v47::FactsV47,
    pub season: crate::doubling_season_public_v48::FactsV48,
    pub trigger_order: Option<crate::policy_surface_v5::TriggerOrderContextV45>,
    pub ajani: crate::ajani_public_v42::FactsV42,
    pub faebloom: crate::faebloom_public_v43::FactsV43,
    pub creature_target_traits: Vec<crate::faebloom_observation_v43::CreatureTargetTraitsV43>,
}
pub(crate) type SeasonPublicPartsV48 = (
    SeasonObservationV48,
    Vec<crate::akroma_family_public_v40::FamilyProjectionV40>,
);
pub fn producer_sha256() -> String {
    let mut h = Sha256::new();
    for s in [
        SCHEMA,
        include_str!("season_observation_v48.rs"),
        include_str!("state.rs"),
        include_str!("linked_aura_operation_v1.rs"),
        include_str!("season_token_attachment_v48.rs"),
        include_str!("divine_public_v44.rs"),
        include_str!("rules_meaning_divine_v44.rs"),
        include_str!("engine.rs"),
        include_str!("spell_costs_v1.rs"),
        include_str!("rl.rs"),
        include_str!("akroma_family_public_v40.rs"),
        include_str!("planeswalker_v1.rs"),
        include_str!("rl_session/season_v48.rs"),
        include_str!("rl_session/limited_common_v1.rs"),
        include_str!("rl_session.rs"),
        include_str!("rl_session/flat_action_v3.rs"),
        include_str!("rl_session/flat_action_v4.rs"),
        include_str!("policy_surface_v5.rs"),
        include_str!("policy_surface_v5/hunter_order_v45.rs"),
        include_str!("surface_v2.rs"),
        include_str!("human_bo3_v1/season_v48.rs"),
        include_str!("human_bo3_v1/limited_common_v1.rs"),
        include_str!("human_bo3_v1/season_v48/facts.rs"),
        include_str!("human_bo3_v1/season_v48/season_facts.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("human_bo3_v1/akroma_families_v40.rs"),
        include_str!("divine_actions_v44.rs"),
        include_str!("rl_variable_cast_v1.rs"),
        include_str!("policy_observation_variable_cast_v1.rs"),
        include_str!("faebloom_observation_v43.rs"),
        include_str!("engine/omniscience_cast_v1.rs"),
        include_str!("resolution_cast_v1.rs"),
        include_str!("effect/harvester_v1.rs"),
        include_str!("leyline_axe_v1.rs"),
        include_str!("london_mulligan_v1.rs"),
        include_str!("life_loss_turn_v1.rs"),
        include_str!("soul_shackled_zombie_v1.rs"),
        include_str!("progenitus_v1.rs"),
        include_str!("controlled_creature_grants_v1.rs"),
        include_str!("source_power_activation_v1.rs"),
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
    ] {
        h.update([0]);
        h.update(s.as_bytes());
    }
    for s in [
        crate::ajani_public_v42::producer_sha256(),
        crate::akroma_public_v1::producer_sha256_v1(),
        crate::faebloom_public_v43::producer_sha256(),
        crate::high_society_hunter_public_v45::producer_sha256(),
        crate::eaten_public_v46::producer_sha256(),
        crate::panda_public_v47::producer_sha256(),
        crate::doubling_season_public_v48::producer_sha256(),
    ] {
        h.update([1]);
        h.update(s.as_bytes());
    }
    h.update(b"legacy-graveyard-return-guard/v61\0");
    h.update(crate::graveyard_return_public_v61::producer_sha256().as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
pub fn commitment(value: &SeasonObservationV48) -> Result<String, RlContractError> {
    let mut v = value.clone();
    v.projection_sha256.clear();
    let b = serde_json::to_vec(&v).map_err(|e| RlContractError(e.to_string()))?;
    Ok(Sha256::digest(b)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("Season V48 requires the actual Foundations catalog".into());
    }
    crate::controlled_creature_grants_v1::validate_color_history(state)?;
    crate::akroma_public_v1::roots().map_err(|e| e.0)?;
    crate::akroma_family_public_v40::validate_state_for_divine_v44(state)?;
    crate::high_society_hunter_custody_v45::validate(state)?;
    crate::ajani_public_v42::facts(state).map_err(|e| e.0)?;
    crate::eaten_public_v46::validate_state(state)?;
    crate::fiendish_panda_v1::validate_state(state)?;
    crate::doubling_season_v48::validate_token_journals(state)?;
    Ok(())
}
pub fn observe_v48(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &crate::policy_surface_v5::PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<SeasonObservationV48, RlContractError> {
    observe_parts_v48(state, surface, actor_and_origin, step, physical, substep).map(|(o, _)| o)
}
pub(crate) fn observe_parts_v48(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &crate::policy_surface_v5::PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<SeasonPublicPartsV48, RlContractError> {
    let (actor, origin) = actor_and_origin;
    let (substep_index, substep_count) = substep;
    let roots = crate::akroma_public_v1::roots()?;
    let (grants, color_departures) = crate::akroma_public_v1::facts(state)?;
    let base = crate::rl::common_family_observation_storage_scoped(
        (crate::rl::CommonProjectionScopeV42::SeasonV48, state),
        surface,
        actor,
        step,
        physical,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let mut base = storage.base;
    let mut parts = crate::akroma_family_public_v40::build_for_scope(
        state,
        surface,
        actor,
        step,
        physical,
        substep,
        crate::rl::CommonProjectionScopeV42::SeasonV48,
    )?;
    base.projection = crate::akroma_family_public_v40::merge_projection(&base.projection, &parts)?;
    let mut observation = SeasonObservationV48 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
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
        grants,
        families: parts.iter().map(|part| part.facts()).collect(),
        color_departures,
        roots,
        projection_sha256: String::new(),
        ajani: crate::ajani_public_v42::facts(state)?,
        faebloom: crate::faebloom_public_v43::capture(state)?,
        creature_target_traits: crate::faebloom_observation_v43::target_traits(state)?,
        divine: crate::divine_public_v44::facts(state, origin, actor)?,
        hunter: crate::high_society_hunter_public_v45::capture(state)?,
        eaten: crate::eaten_public_v46::capture(state, actor)?,
        panda: crate::panda_public_v47::capture(state)?,
        season: crate::doubling_season_public_v48::capture(state)?,
        trigger_order: surface
            .hunter_order_context_v45(state)
            .map_err(RlContractError)?,
    };
    observation.projection_sha256 = commitment(&observation)?;
    for part in &mut parts {
        part.set_projection(&observation.projection);
    }
    Ok((observation, parts))
}

impl SeasonObservationV48 {
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.season.schema != crate::doubling_season_public_v48::SCHEMA
            || self.season.producer_sha256 != crate::doubling_season_public_v48::producer_sha256()
            || self.season.card_db_hash != self.card_db_hash
            || self.season.meanings != crate::doubling_season_public_v48::meanings()?
            || self.schema != SCHEMA
            || self.producer_sha256 != producer_sha256()
            || self.kernel_version != crate::KERNEL_VERSION
            || self.roots != crate::akroma_public_v1::roots()?
            || self.divine.meanings != crate::divine_public_v44::meanings()?
            || self.ajani.meanings != crate::ajani_public_v42::meanings()?
            || self.faebloom.schema != crate::faebloom_public_v43::SCHEMA
            || self.faebloom.producer_sha256 != crate::faebloom_public_v43::producer_sha256()
            || self.faebloom.card_db_hash != self.card_db_hash
            || self.eaten.schema != crate::eaten_public_v46::SCHEMA
            || self.eaten.producer_sha256 != crate::eaten_public_v46::producer_sha256()
            || self.eaten.meanings != crate::eaten_public_v46::meanings()?
            || self.panda.schema != crate::panda_public_v47::SCHEMA
            || self.panda.producer_sha256 != crate::panda_public_v47::producer_sha256()
            || self.panda.card_db_hash != self.card_db_hash
            || self.panda.meanings != catalog_meanings()?
            || self.hunter.schema != crate::high_society_hunter_public_v45::SCHEMA
            || self.hunter.producer_sha256
                != crate::high_society_hunter_public_v45::producer_sha256()
            || self.hunter.card_db_hash != self.card_db_hash
            || self.substep_count == 0
            || self.substep_index >= self.substep_count
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError(
                "Season V48 observation identity mismatch".into(),
            ));
        }
        for meaning in &self.ajani.meanings {
            crate::rules_meaning_ajani_v42::validate(meaning)
                .map_err(|_| RlContractError("Ajani precise root differs".into()))?;
        }
        for meaning in &self.hunter.meanings {
            crate::rules_meaning_high_society_hunter_v1::validate(meaning)
                .map_err(|_| RlContractError("Hunter precise root changed".into()))?;
        }
        for meaning in &self.panda.meanings {
            crate::rules_meaning_fiendish_panda_v1::validate(meaning).map_err(RlContractError)?;
        }
        let selector =
            crate::rules_meaning_faebloom_v43::live_selector().map_err(RlContractError)?;
        for meaning in &self.faebloom.roots {
            crate::rules_meaning_faebloom_v43::validate(&selector, meaning)
                .map_err(RlContractError)?;
        }
        Ok(())
    }
}
fn catalog_meanings(
) -> Result<Vec<crate::rules_meaning_fiendish_panda_v1::MeaningV1>, RlContractError> {
    use crate::rules_meaning_fiendish_panda_v1 as meaning;
    let mut roots = Vec::new();
    for (index, _) in crate::card_def::CARD_DEFS.iter().enumerate() {
        let id = u16::try_from(index)
            .map_err(|_| RlContractError("Panda catalog exceeds u16".into()))?;
        if crate::fiendish_panda_v1::owns_definition(id) {
            for root in [
                meaning::RootV1::CreatureSpell,
                meaning::RootV1::ControllerLifeGain,
                meaning::RootV1::SourceDeath,
            ] {
                roots.push(meaning::printed_meaning(id, root).map_err(RlContractError)?);
            }
        }
    }
    Ok(roots)
}
