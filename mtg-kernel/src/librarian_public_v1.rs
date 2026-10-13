//! Distinct Librarian transport from the actual shared typed public producer.
use crate::event::CommittedEvent;
use crate::ids::PlayerId;
use crate::policy_observation_v8::PolicyObservationExtensionsV8;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, CardStableRefV1, KnownLibraryCardV4, PlayerSeatV1,
    PublicObservationProjectionV5, RlContractError,
};
use crate::state::{GameState, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/librarian-public/v1";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleV1 {
    Paid,
    Resolved,
    Countered,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationV1 {
    pub source_at_payment: CardStableRefV1,
    pub captured_controller: PlayerSeatV1,
    pub once_consumed_on_payment: bool,
    pub lifecycle: LifecycleV1,
    pub actual_draw_attempt: bool,
    pub original_source_received_upgrade_and_counters: bool,
    pub current_source: Option<CardStableRefV1>,
    pub current_once_used: Option<bool>,
    pub current_plus_one_counters: Option<i32>,
    pub current_creature_subtype_ids: Option<Vec<u16>>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootV1 {
    pub card_id: u16,
    pub root: crate::rules_meaning_librarian_v1::RootV1,
    pub root_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibrarianObservationV1 {
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
    pub activations: Vec<ActivationV1>,
    pub roots: Vec<RootV1>,
    pub projection_sha256: String,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    let mut hash = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("librarian_public_v1.rs"),
        include_str!("librarian_activation_v1.rs"),
        include_str!("mild_mannered_librarian_v1.rs"),
        include_str!("rules_meaning_librarian_v1.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
        include_str!("mana.rs"),
        include_str!("policy_surface_v5.rs"),
        include_str!("surface_v2.rs"),
        include_str!("rl.rs"),
        include_str!("rl_session/librarian_v20.rs"),
        include_str!("human_bo3_v1/librarian_v20.rs"),
        include_str!("human_bo3_v1.rs"),
        include_str!("human_bo3_v1/raid_v9.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("human_bo3_v1/visible/standard.rs"),
        include_str!("human_bo3_v1/life_ward_v8.rs"),
        include_str!("sower_restriction_capture_v1.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("effect.rs"),
        include_str!("rl_session.rs"),
        include_str!("state.rs"),
        include_str!("policy_observation_v8.rs"),
        include_str!("policy_observation_v7.rs"),
        include_str!("policy_observation_v6.rs"),
        include_str!("rl_session/flat_action_v3.rs"),
        include_str!("rl_session/flat_action_v4.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
fn roots() -> Result<Vec<RootV1>, RlContractError> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Ok(vec![]);
    }
    let Some(card) = crate::card_def::card_id_by_name("Mild-Mannered Librarian") else {
        return Ok(vec![]);
    };
    let selector = crate::rules_meaning_librarian_v1::live_selector()
        .map_err(|_| RlContractError("Librarian catalog unsupported".into()))?;
    [
        crate::rules_meaning_librarian_v1::RootV1::PrintedCreatureSpell,
        crate::rules_meaning_librarian_v1::RootV1::PrintedActivation { ability_index: 0 },
    ]
    .into_iter()
    .map(|root| {
        let meaning = crate::rules_meaning_librarian_v1::meaning(&selector, card, root)
            .map_err(|_| RlContractError("Librarian printed root unsupported".into()))?;
        Ok(RootV1 {
            card_id: card,
            root,
            root_sha256: meaning.root_sha256,
        })
    })
    .collect()
}
fn commitment(observation: &LibrarianObservationV1) -> Result<String, RlContractError> {
    let mut value = serde_json::to_value(observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("Librarian object absent".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&value)?))
}
impl LibrarianObservationV1 {
    /// Artifact integrity does not authorize execution; the owner rederives it.
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.kernel_version != crate::KERNEL_VERSION
            || self.surface_version != crate::surface_v2::H2_PREDICATE_VERSION
            || self.policy_surface_version != crate::policy_surface_v5::POLICY_SURFACE_VERSION
            || self.substep_count == 0
            || self.substep_index >= self.substep_count
            || self.producer_sha256 != producer_sha256_v1()
            || self.roots != roots()?
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong Librarian artifact identity".into()));
        }
        Ok(())
    }
}
pub(crate) fn activations(state: &GameState) -> Result<Vec<ActivationV1>, RlContractError> {
    crate::librarian_activation_v1::validate_state(state).map_err(RlContractError)?;
    state.engine.event_history.iter().filter_map(|event| {
        let CommittedEvent::LibrarianActivationCommittedV1 { item } = event else { return None; };
        Some((|| {
            let source = item.v4.ability_source_contract.ok_or_else(|| RlContractError("paid source absent".into()))?;
            let operation = state.engine.event_history.iter().find_map(|event| match event {
                CommittedEvent::LibrarianActivationOperationV1 { item: original, operation }
                    if original.as_ref() == item.as_ref() => Some(operation), _ => None,
            });
            let (lifecycle, actual_draw_attempt, original_source_received_upgrade_and_counters) = match operation {
                None => (LifecycleV1::Paid, false, false),
                Some(crate::librarian_activation_v1::OperationV1::Resolved { counter_history_index, .. }) =>
                    (LifecycleV1::Resolved, true, counter_history_index.is_some()),
                Some(crate::librarian_activation_v1::OperationV1::Countered { .. }) => (LifecycleV1::Countered, false, false),
            };
            let current = state.objects.try_get(source.source).filter(|live| live.zone == Zone::Battlefield && live.zone_change_count == source.zone_change_count);
            Ok(ActivationV1 {
                source_at_payment: CardStableRefV1 { arena_id: source.source.0, card_db_id: source.card_def,
                    owner: source.owner.into(), controller: source.controller.into(), zone: Zone::Battlefield,
                    zone_change_count: source.zone_change_count },
                captured_controller: source.controller.into(), once_consumed_on_payment: true,
                lifecycle, actual_draw_attempt, original_source_received_upgrade_and_counters,
                current_source: current.map(|live| crate::sower_restriction_capture_v1::masked_ref(source.source, live)).transpose()?,
                current_once_used: current.map(|live| live.v4.creature_upgrade.as_ref().is_some_and(|upgrade| upgrade.once_activated.contains(&0))),
                current_plus_one_counters: current.map(|live| live.counters.plus1_plus1),
                current_creature_subtype_ids: current.map(|_| crate::engine::effective_subtype_ids(state, source.source)),
            })
        })())
    }).collect()
}
// V40 consumes these private typed parts; standalone predecessors retain their refusal path.
pub(crate) fn parts_for_akroma_v40(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
) -> Result<LibrarianObservationV1, RlContractError> {
    parts_for_common_scope(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        substep,
        crate::rl::CommonProjectionScopeV42::ExistingV40,
    )
}
pub(crate) fn parts_for_ajani_v42(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
) -> Result<LibrarianObservationV1, RlContractError> {
    parts_for_common_scope(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        substep,
        crate::rl::CommonProjectionScopeV42::FullAjaniV42,
    )
}
pub(crate) fn parts_for_faebloom_v43(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
) -> Result<LibrarianObservationV1, RlContractError> {
    parts_for_common_scope(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        substep,
        crate::rl::CommonProjectionScopeV42::FullFaebloomV43,
    )
}
pub(crate) fn parts_for_common_scope(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
    scope: crate::rl::CommonProjectionScopeV42,
) -> Result<LibrarianObservationV1, RlContractError> {
    let (substep_index, substep_count) = substep;

    let activations = activations(state)?;
    let base = crate::rl::common_family_observation_storage_scoped(
        (scope, state),
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let base = storage.base;
    let observation = LibrarianObservationV1 {
        schema: String::new(),
        producer_sha256: String::new(),
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
        activations,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    Ok(observation)
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
) -> Result<LibrarianObservationV1, RlContractError> {
    let activations = activations(state)?;
    let base = crate::rl::librarian_observation_storage_v1(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let base = storage.base;
    let mut observation = LibrarianObservationV1 {
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
        activations,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    observation.projection_sha256 = commitment(&observation)?;
    Ok(observation)
}
#[cfg(test)]
pub(crate) mod tests;
