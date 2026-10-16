//! Distinct Ravenous V51 mixed-family observation. Executable session admission is separate.
use crate::akroma_family_public_v40::diagnose_observation_error;
use crate::ids::PlayerId;
use crate::policy_observation_v8::PolicyObservationExtensionsV8;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, KnownLibraryCardV4, PlayerSeatV1, PublicObservationProjectionV5, RlContractError,
};
use crate::state::GameState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/ravenous-observation/v51";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RavenousObservationV51 {
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
    pub conqueror: crate::conqueror_public_v49::FactsV49,
    pub fake: crate::fake_your_own_death_v50::public_v50::FactsV50,
    /// Exact mandatory values for every public battlefield card, in the same
    /// seat/order as the common projection. Narrow common slots are retained
    /// only for the predecessor family producers and never supersede these.
    pub battlefield: [Vec<crate::ravenous_public_v51::CardV51>; 2],
    pub ravenous: crate::ravenous_public_v51::FactsV51,
    pub trigger_order: Option<crate::policy_surface_v5::TriggerOrderContextV45>,
    pub ajani: crate::ajani_public_v42::FactsV42,
    pub faebloom: crate::faebloom_public_v43::FactsV43,
    pub creature_target_traits: Vec<crate::faebloom_observation_v43::CreatureTargetTraitsV43>,
}
pub(crate) type RavenousPublicPartsV51 = (
    RavenousObservationV51,
    Vec<crate::akroma_family_public_v40::FamilyProjectionV40>,
);
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut h = Sha256::new();
        for s in [
            SCHEMA,
            include_str!("ravenous_observation_v51.rs"),
            include_str!("ravenous_amulet_v1.rs"),
            include_str!("rl_session/ravenous_v51.rs"),
            include_str!("soul_counters_v1.rs"),
            include_str!("state.rs"),
            include_str!("linked_aura_operation_v1.rs"),
            include_str!("season_token_attachment_v48.rs"),
            include_str!("divine_public_v44.rs"),
            include_str!("rules_meaning_divine_v44.rs"),
            include_str!("engine.rs"),
            include_str!("spell_costs_v1.rs"),
            include_str!("rl.rs"),
            include_str!("akroma_family_public_v40.rs"),
            include_str!("crystal_public_v1.rs"),
            include_str!("twinflame_observation_v1.rs"),
            include_str!("planeswalker_v1.rs"),
            include_str!("rl_session/conqueror_v49.rs"),
            include_str!("rl_session/fake_v50.rs"),
            include_str!("rl_session/limited_common_v1.rs"),
            include_str!("rl_session.rs"),
            include_str!("rl_session/flat_action_v3.rs"),
            include_str!("rl_session/flat_action_v4.rs"),
            include_str!("policy_surface_v5.rs"),
            include_str!("policy_surface_v5/hunter_order_v45.rs"),
            include_str!("surface_v2.rs"),
            include_str!("human_bo3_v1/conqueror_v49.rs"),
            include_str!("human_bo3_v1/fake_v50.rs"),
            include_str!("human_bo3_v1/ravenous_v51.rs"),
            include_str!("human_bo3_v1/ravenous_v51/exact_state.rs"),
            include_str!("human_bo3_v1/ravenous_v51/ravenous_facts.rs"),
            include_str!("human_bo3_v1/fake_v50/fake_facts.rs"),
            include_str!("human_bo3_v1/fake_v50/facts.rs"),
            include_str!("human_bo3_v1/fake_v50/season_facts.rs"),
            include_str!("human_bo3_v1/fake_v50/conqueror_facts.rs"),
            include_str!("human_bo3_v1/limited_common_v1.rs"),
            include_str!("human_bo3_v1/conqueror_v49/facts.rs"),
            include_str!("human_bo3_v1/conqueror_v49/season_facts.rs"),
            include_str!("human_bo3_v1/conqueror_v49/conqueror_facts.rs"),
            include_str!("conqueror_public_v49.rs"),
            include_str!("human_bo3_v1/visible.rs"),
            include_str!("human_bo3_v1/akroma_families_v40.rs"),
            include_str!("divine_actions_v44.rs"),
            include_str!("rl_variable_cast_v1.rs"),
            include_str!("policy_observation_variable_cast_v1.rs"),
            include_str!("faebloom_observation_v43.rs"),
            include_str!("sower_public_v1.rs"),
            include_str!("frenzied_public_v1.rs"),
            include_str!("wildwood_public_v1.rs"),
            include_str!("authority_public_v1.rs"),
            include_str!("employment_public_v1.rs"),
            include_str!("cerberus_public_v1.rs"),
            include_str!("librarian_public_v1.rs"),
            include_str!("extra_turn_public_v1.rs"),
            include_str!("juggernaut_public_v1.rs"),
            include_str!("ashroot_public_v1.rs"),
            include_str!("banishing_public_v1.rs"),
            include_str!("stromkirk_public_v1.rs"),
            include_str!("gourmand_public_v1.rs"),
            include_str!("garruk_public_v1.rs"),
            include_str!("arahbo_public_v1.rs"),
            include_str!("strongbox_public_v1.rs"),
            include_str!("fiery_observation_v1.rs"),
            include_str!("krenko_observation_v1.rs"),
            include_str!("zul_ashur_observation_v1.rs"),
            include_str!("banner_observation_v1.rs"),
            include_str!("inspiration_observation_v1.rs"),
            include_str!("fishing_pole_observation_v39.rs"),
            include_str!("bloodthirsty_conqueror_v49/custody.rs"),
            include_str!("event_history_v1.rs"),
        ] {
            h.update([0]);
            h.update(s.as_bytes());
        }
        for s in [
            crate::ravenous_public_v51::producer_sha256(),
            crate::ajani_public_v42::producer_sha256(),
            crate::akroma_public_v1::producer_sha256_v1(),
            crate::faebloom_public_v43::producer_sha256(),
            crate::high_society_hunter_public_v45::producer_sha256(),
            crate::eaten_public_v46::producer_sha256(),
            crate::panda_public_v47::producer_sha256(),
            crate::doubling_season_public_v48::producer_sha256(),
            crate::conqueror_public_v49::producer_sha256(),
            crate::fake_your_own_death_v50::public_v50::producer_sha256(),
        ] {
            h.update([1]);
            h.update(s.as_bytes());
        }
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}
pub fn commitment(value: &RavenousObservationV51) -> Result<String, RlContractError> {
    let mut v = value.clone();
    v.projection_sha256.clear();
    let b = serde_json::to_vec(&v).map_err(|e| RlContractError(e.to_string()))?;
    Ok(Sha256::digest(b)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    crate::engine::validate_battlefield_pt(state)
        .map_err(|id| format!("native PT envelope exceeded for {id:?}"))?;
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("Ravenous V51 requires the actual Foundations catalog".into());
    }
    crate::ravenous_public_v51::battlefield(state).map_err(|e| e.0)?;
    crate::controlled_creature_grants_v1::validate_color_history(state)?;
    crate::akroma_public_v1::roots().map_err(|e| e.0)?;
    crate::akroma_family_public_v40::validate_state_for_divine_v44(state)?;
    crate::high_society_hunter_custody_v45::validate(state)?;
    crate::ajani_public_v42::facts(state).map_err(|e| e.0)?;
    crate::eaten_public_v46::validate_state(state)?;
    crate::fiendish_panda_v1::validate_state(state)?;
    crate::doubling_season_v48::validate_token_journals(state)?;
    crate::bloodthirsty_conqueror_v49::validate(state)?;
    crate::fake_your_own_death_v50::public_v50::capture(state).map_err(|e| e.0)?;
    Ok(())
}
pub(crate) fn validate_state_complete_prepared_v57(state: &GameState) -> Result<(), String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("Ravenous V51 requires the actual Foundations catalog".into());
    }
    crate::ravenous_public_v51::capture(state).map_err(|e| e.0)?;
    crate::controlled_creature_grants_v1::validate_color_history(state)?;
    crate::akroma_public_v1::roots().map_err(|e| e.0)?;
    crate::akroma_family_public_v40::validate_state_for_divine_v44(state)?;
    crate::high_society_hunter_custody_v45::validate(state)?;
    crate::ajani_public_v42::facts(state).map_err(|e| e.0)?;
    crate::eaten_public_v46::validate_state(state)?;
    crate::fiendish_panda_v1::validate_state(state)?;
    crate::doubling_season_v48::validate_token_journals(state)?;
    crate::bloodthirsty_conqueror_v49::validate(state)?;
    crate::fake_your_own_death_v50::public_v50::capture(state).map_err(|e| e.0)?;
    Ok(())
}
pub fn observe_v51(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &crate::policy_surface_v5::PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<RavenousObservationV51, RlContractError> {
    observe_parts_v51(state, surface, actor_and_origin, step, physical, substep).map(|(o, _)| o)
}

/// Rebuild the complete value from current owned state and the supplied decision
/// context. An artifact commitment alone never grants permission to act.
pub fn validate_current(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &crate::policy_surface_v5::PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
    value: &RavenousObservationV51,
) -> Result<(), RlContractError> {
    let expected = observe_v51(state, surface, actor_and_origin, step, physical, substep)?;
    if expected != *value {
        return Err(RlContractError(
            "Ravenous V51 observation differs from current owned state and decision".into(),
        ));
    }
    Ok(())
}

pub(crate) fn observe_parts_v51(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &crate::policy_surface_v5::PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<RavenousPublicPartsV51, RlContractError> {
    crate::elenda_public_v52::require_legacy_visible(state, Some(actor_and_origin.0))
        .map_err(RlContractError)?;
    observe_parts_complete_base_v52(state, surface, actor_and_origin, step, physical, substep)
}

/// Complete inherited storage for the distinct V52 envelope. This bypasses
/// only the Elenda predecessor refusal; every V51 family validator remains.
pub(crate) fn observe_parts_complete_base_v52(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &crate::policy_surface_v5::PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<RavenousPublicPartsV51, RlContractError> {
    observe_parts_complete_base_v52_using_family_builder(
        state,
        surface,
        actor_and_origin,
        step,
        physical,
        substep,
        crate::akroma_family_public_v40::build_for_scope,
    )
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) fn observe_parts_complete_base_v52_fully_rebuilt(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &crate::policy_surface_v5::PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<RavenousPublicPartsV51, RlContractError> {
    observe_parts_complete_base_v52_using_family_builder(
        state,
        surface,
        actor_and_origin,
        step,
        physical,
        substep,
        crate::akroma_family_public_v40::build_for_scope_fully_rebuilt,
    )
}

fn observe_parts_complete_base_v52_using_family_builder(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &crate::policy_surface_v5::PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
    build_families: crate::akroma_family_public_v40::FamilyPartsBuilderV40,
) -> Result<RavenousPublicPartsV51, RlContractError> {
    let (actor, origin) = actor_and_origin;
    surface
        .validate_observation_origin_substeps_v45(state, actor, origin, substep)
        .map_err(|error| diagnose_observation_error("inherited.origin_substeps", error))
        .map_err(RlContractError)?;
    let (substep_index, substep_count) = substep;
    let ravenous = crate::ravenous_public_v51::battlefield(state)
        .map_err(|error| diagnose_observation_error("inherited.ravenous_battlefield", error))?;
    let roots = crate::akroma_public_v1::roots()
        .map_err(|error| diagnose_observation_error("inherited.akroma_roots", error))?;
    let (grants, color_departures) = crate::akroma_public_v1::facts(state)
        .map_err(|error| diagnose_observation_error("inherited.akroma_facts", error))?;
    let base = crate::rl::common_family_observation_storage_scoped(
        (crate::rl::CommonProjectionScopeV42::RavenousV51, state),
        surface,
        actor,
        step,
        physical,
        (substep_index, substep_count),
    )
    .map_err(|error| diagnose_observation_error("inherited.common_storage", error))?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)
        .map_err(|error| diagnose_observation_error("inherited.extensions", error))?;
    let mut base = storage.base;
    let mut parts = build_families(
        state,
        surface,
        actor,
        step,
        physical,
        substep,
        crate::rl::CommonProjectionScopeV42::RavenousV51,
    )
    .map_err(|error| diagnose_observation_error("inherited.family_parts", error))?;
    base.projection =
        crate::akroma_family_public_v40::merge_projection(&base.projection, &parts)
            .map_err(|error| diagnose_observation_error("inherited.family_merge", error))?;
    let mut observation = RavenousObservationV51 {
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
        ajani: crate::ajani_public_v42::facts(state)
            .map_err(|error| diagnose_observation_error("inherited.ajani", error))?,
        faebloom: crate::faebloom_public_v43::capture(state)
            .map_err(|error| diagnose_observation_error("inherited.faebloom", error))?,
        creature_target_traits: crate::faebloom_observation_v43::target_traits(state)
            .map_err(|error| diagnose_observation_error("inherited.target_traits", error))?,
        divine: crate::divine_public_v44::facts(state, origin, actor)
            .map_err(|error| diagnose_observation_error("inherited.divine", error))?,
        hunter: crate::high_society_hunter_public_v45::capture(state)
            .map_err(|error| diagnose_observation_error("inherited.hunter", error))?,
        eaten: crate::eaten_public_v46::capture(state, actor)
            .map_err(|error| diagnose_observation_error("inherited.eaten", error))?,
        panda: crate::panda_public_v47::capture(state)
            .map_err(|error| diagnose_observation_error("inherited.panda", error))?,
        season: crate::doubling_season_public_v48::capture(state)
            .map_err(|error| diagnose_observation_error("inherited.season", error))?,
        conqueror: crate::conqueror_public_v49::capture(state)
            .map_err(|error| diagnose_observation_error("inherited.conqueror", error))?,
        fake: crate::fake_your_own_death_v50::public_v50::capture(state)
            .map_err(|error| diagnose_observation_error("inherited.fake", error))?,
        battlefield: ravenous.cards,
        ravenous: ravenous.facts,
        trigger_order: surface
            .hunter_order_context_v45(state)
            .map_err(|error| diagnose_observation_error("inherited.trigger_order", error))
            .map_err(RlContractError)?,
    };
    observation
        .validate_battlefield_artifact()
        .map_err(|error| diagnose_observation_error("inherited.battlefield_artifact", error))?;
    observation.projection_sha256 = commitment(&observation)
        .map_err(|error| diagnose_observation_error("inherited.commitment", error))?;
    for part in &mut parts {
        part.set_projection(&observation.projection);
    }
    Ok((observation, parts))
}

pub(crate) fn observe_parts_complete_prepared_v57(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &crate::policy_surface_v5::PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<RavenousPublicPartsV51, RlContractError> {
    observe_parts_complete_prepared_v57_using_family_builder(
        state,
        surface,
        actor_and_origin,
        step,
        physical,
        substep,
        crate::akroma_family_public_v40::build_for_scope,
    )
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) fn observe_parts_complete_prepared_v57_fully_rebuilt(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &crate::policy_surface_v5::PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<RavenousPublicPartsV51, RlContractError> {
    observe_parts_complete_prepared_v57_using_family_builder(
        state,
        surface,
        actor_and_origin,
        step,
        physical,
        substep,
        crate::akroma_family_public_v40::build_for_scope_fully_rebuilt,
    )
}

fn observe_parts_complete_prepared_v57_using_family_builder(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor_and_origin: (PlayerId, &crate::policy_surface_v5::PolicyDecisionV5),
    step: u64,
    physical: u64,
    substep: (u32, u32),
    build_families: crate::akroma_family_public_v40::FamilyPartsBuilderV40,
) -> Result<RavenousPublicPartsV51, RlContractError> {
    let (actor, origin) = actor_and_origin;
    surface
        .validate_observation_origin_substeps_v45(state, actor, origin, substep)
        .map_err(|error| diagnose_observation_error("inherited.origin_substeps", error))
        .map_err(RlContractError)?;
    let (substep_index, substep_count) = substep;
    let ravenous = crate::ravenous_public_v51::battlefield_prepared_v57(state)
        .map_err(|error| diagnose_observation_error("inherited.prepared_battlefield", error))?;
    let roots = crate::akroma_public_v1::roots()
        .map_err(|error| diagnose_observation_error("inherited.akroma_roots", error))?;
    let (grants, color_departures) = crate::akroma_public_v1::facts(state)
        .map_err(|error| diagnose_observation_error("inherited.akroma_facts", error))?;
    let base = crate::rl::common_family_observation_storage_scoped(
        (crate::rl::CommonProjectionScopeV42::PreparedV57, state),
        surface,
        actor,
        step,
        physical,
        (substep_index, substep_count),
    )
    .map_err(|error| diagnose_observation_error("inherited.common_storage", error))?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)
        .map_err(|error| diagnose_observation_error("inherited.extensions", error))?;
    let mut base = storage.base;
    let mut parts = build_families(
        state,
        surface,
        actor,
        step,
        physical,
        substep,
        crate::rl::CommonProjectionScopeV42::PreparedV57,
    )
    .map_err(|error| diagnose_observation_error("inherited.family_parts", error))?;
    base.projection =
        crate::akroma_family_public_v40::merge_projection(&base.projection, &parts)
            .map_err(|error| diagnose_observation_error("inherited.family_merge", error))?;
    let mut observation = RavenousObservationV51 {
        schema: crate::prepared_observation_v57::COMMON_SCHEMA.into(),
        producer_sha256: crate::prepared_observation_v57::producer_sha256(),
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
        ajani: crate::ajani_public_v42::facts(state)
            .map_err(|error| diagnose_observation_error("inherited.ajani", error))?,
        faebloom: crate::faebloom_public_v43::capture(state)
            .map_err(|error| diagnose_observation_error("inherited.faebloom", error))?,
        creature_target_traits: crate::faebloom_observation_v43::target_traits(state)
            .map_err(|error| diagnose_observation_error("inherited.target_traits", error))?,
        divine: crate::divine_public_v44::facts(state, origin, actor)
            .map_err(|error| diagnose_observation_error("inherited.divine", error))?,
        hunter: crate::high_society_hunter_public_v45::capture(state)
            .map_err(|error| diagnose_observation_error("inherited.hunter", error))?,
        eaten: crate::eaten_public_v46::capture(state, actor)
            .map_err(|error| diagnose_observation_error("inherited.eaten", error))?,
        panda: crate::panda_public_v47::capture(state)
            .map_err(|error| diagnose_observation_error("inherited.panda", error))?,
        season: crate::doubling_season_public_v48::capture(state)
            .map_err(|error| diagnose_observation_error("inherited.season", error))?,
        conqueror: crate::conqueror_public_v49::capture(state)
            .map_err(|error| diagnose_observation_error("inherited.conqueror", error))?,
        fake: crate::fake_your_own_death_v50::public_v50::capture(state)
            .map_err(|error| diagnose_observation_error("inherited.fake", error))?,
        battlefield: ravenous.cards,
        ravenous: ravenous.facts,
        trigger_order: surface
            .hunter_order_context_v45(state)
            .map_err(|error| diagnose_observation_error("inherited.trigger_order", error))
            .map_err(RlContractError)?,
    };
    observation.projection_sha256 = commitment(&observation)
        .map_err(|error| diagnose_observation_error("inherited.commitment", error))?;
    for part in &mut parts {
        part.set_projection(&observation.projection);
    }
    Ok((observation, parts))
}

impl RavenousObservationV51 {
    fn validate_battlefield_artifact(&self) -> Result<(), RlContractError> {
        use crate::ravenous_public_v51::{IncarnationV51, SoulCountV51};
        let mut seen = std::collections::BTreeSet::new();
        let mut soul_counts = Vec::new();
        for (seat, cards) in self.battlefield.iter().enumerate() {
            let common = &self.projection.surface.battlefield[seat];
            if common.len() != cards.len() {
                return Err(diagnose_observation_error(
                    "ravenous_artifact.ravenous_v51_battlefield_roster_differs",
                    RlContractError("Ravenous V51 battlefield roster differs".into()),
                ));
            }
            for (exact, base) in cards.iter().zip(common) {
                if exact.card != *base
                    || exact.card.stable.zone != crate::state::Zone::Battlefield
                    || exact.card.stable.controller
                        != PlayerSeatV1::from(if seat == 0 {
                            PlayerId::P0
                        } else {
                            PlayerId::P1
                        })
                    || !seen.insert(exact.card.stable.arena_id)
                    || exact.soul < 0
                    || exact.plus_one_counters < 0
                    || exact.card.damage != u16::try_from(exact.marked_damage).unwrap_or(0)
                    || exact.card.counters.plus1_plus1
                        != i16::try_from(exact.plus_one_counters).unwrap_or(0)
                {
                    return Err(diagnose_observation_error(
                        "ravenous_artifact.ravenous_v51_exact_battlefield_card_differs",
                        RlContractError("Ravenous V51 exact battlefield card differs".into()),
                    ));
                }
                if exact.soul != 0 {
                    soul_counts.push(SoulCountV51 {
                        permanent: IncarnationV51 {
                            object: exact.card.stable.arena_id,
                            generation: exact.card.stable.zone_change_count,
                        },
                        count: exact.soul,
                    });
                }
            }
        }
        soul_counts.sort_by_key(|entry| (entry.permanent.object, entry.permanent.generation));
        if soul_counts != self.ravenous.soul_counts {
            return Err(diagnose_observation_error(
                "ravenous_artifact.ravenous_v51_soul_facts_roster_disagree",
                RlContractError("Ravenous V51 soul facts/roster disagree".into()),
            ));
        }
        Ok(())
    }
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        self.validate_artifact_identity(SCHEMA, &producer_sha256())
    }
    /// A distinct Prepared compatibility contract. Never accepts this value as
    /// a frozen V51 artifact; every inherited data/meaning check is shared.
    pub(crate) fn validate_prepared_compatibility_v57(&self) -> Result<(), RlContractError> {
        self.validate_artifact_identity(
            crate::prepared_observation_v57::COMMON_SCHEMA,
            &crate::prepared_observation_v57::producer_sha256(),
        )
    }
    fn validate_artifact_identity(
        &self,
        schema: &str,
        producer: &str,
    ) -> Result<(), RlContractError> {
        self.validate_battlefield_artifact()?;
        if self.ravenous.schema != crate::ravenous_public_v51::SCHEMA
            || self.ravenous.producer_sha256 != crate::ravenous_public_v51::producer_sha256()
            || self.ravenous.card_db_hash != self.card_db_hash
        {
            return Err(diagnose_observation_error(
                "ravenous_artifact.ravenous_v51_public_identity_mismatch",
                RlContractError("Ravenous V51 public identity mismatch".into()),
            ));
        }
        let id = crate::card_def::card_id_by_name("Ravenous Amulet")
            .ok_or_else(|| RlContractError("Ravenous actual printing absent".into()))?;
        use crate::rules_meaning_ravenous_amulet_v51::{printed_meaning, RootV51};
        let meanings = [RootV51::DrawAndSoul, RootV51::SoulLoss]
            .into_iter()
            .map(|root| printed_meaning(id, root).map_err(RlContractError))
            .collect::<Result<Vec<_>, _>>()?;
        if self.ravenous.meanings != meanings {
            return Err(diagnose_observation_error(
                "ravenous_artifact.ravenous_v51_ordered_meanings_differ",
                RlContractError("Ravenous V51 ordered meanings differ".into()),
            ));
        }
        if self.fake.schema != crate::fake_your_own_death_v50::public_v50::SCHEMA
            || self.fake.producer_sha256
                != crate::fake_your_own_death_v50::public_v50::producer_sha256()
            || self.fake.card_db_hash != self.card_db_hash
            || self.fake.meanings != crate::fake_your_own_death_v50::public_v50::meanings()?
            || self.conqueror.schema != crate::conqueror_public_v49::SCHEMA
            || self.conqueror.producer_sha256 != crate::conqueror_public_v49::producer_sha256()
            || self.conqueror.card_db_hash != self.card_db_hash
            || self.conqueror.meanings != crate::conqueror_public_v49::meanings()?
            || self.season.schema != crate::doubling_season_public_v48::SCHEMA
            || self.season.producer_sha256 != crate::doubling_season_public_v48::producer_sha256()
            || self.season.card_db_hash != self.card_db_hash
            || self.season.meanings != crate::doubling_season_public_v48::meanings()?
            || self.schema != schema
            || self.producer_sha256 != producer
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
            return Err(diagnose_observation_error(
                "ravenous_artifact.ravenous_v51_observation_identity_mismatch",
                RlContractError("Ravenous V51 observation identity mismatch".into()),
            ));
        }
        for root in &self.fake.meanings {
            crate::rules_meaning_fake_your_own_death_v50::validate(root)
                .map_err(RlContractError)?;
        }
        for root in &self.conqueror.meanings {
            crate::rules_meaning_bloodthirsty_conqueror_v49::validate(&root.selector, root)
                .map_err(RlContractError)?;
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

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod tests;
