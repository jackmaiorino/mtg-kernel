//! Actual V9 transport plus public loss flags and original end-step controller.
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

pub const SCHEMA: &str = "mtg-kernel/stromkirk-public/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LossContextV1 {
    pub source_at_end_step: CardStableRefV1,
    pub captured_controller: PlayerSeatV1,
    pub pending_trigger_ordinal: Option<u32>,
    pub live_stack_index: Option<u32>,
    pub opponent_lost_life_if_resolved_now: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootV1 {
    pub card_id: u16,
    pub ability_index: u16,
    pub root_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StromkirkObservationV1 {
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
    /// None means this selected catalog has no authenticated loss recorder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lost_life_this_turn: Option<[bool; 2]>,
    pub loss_contexts: Vec<LossContextV1>,
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
        include_str!("stromkirk_public_v1.rs"),
        include_str!("stromkirk_bloodthief_v1.rs"),
        include_str!("stromkirk_dispatch_v1.rs"),
        include_str!("trigger.rs"),
        include_str!("effect.rs"),
        include_str!("event.rs"),
        include_str!("effect/temporary_control_v1.rs"),
        include_str!("rules_meaning_stromkirk_v1.rs"),
        include_str!("engine.rs"),
        include_str!("policy_surface_v5.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("standard_statics_v1.rs"),
        include_str!("standard_keywords_v1.rs"),
        include_str!("standard_legends_v1.rs"),
        include_str!("life_loss_turn_v1.rs"),
        include_str!("sower_of_chaos_v1.rs"),
        include_str!("sower_restriction_capture_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("state.rs"),
        include_str!("../build.rs"),
        include_str!("rl.rs"),
        include_str!("policy_observation_v8.rs"),
        include_str!("policy_observation_v9.rs"),
        include_str!("policy_observation_v7.rs"),
        include_str!("rl_session.rs"),
        include_str!("rl_session/stromkirk_v28.rs"),
        include_str!("rl_session/flat_action_v3.rs"),
        include_str!("rl_session/flat_action_v4.rs"),
        include_str!("rl_session/human_v8.rs"),
        include_str!("human_bo3_v1/stromkirk_v28.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/life_ward_v8.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn commitment(observation: &StromkirkObservationV1) -> Result<String, RlContractError> {
    let mut value = serde_json::to_value(observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("Stromkirk projection is not an object".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&value)?))
}
impl StromkirkObservationV1 {
    /// Checks transport identity only. Execution always rederives the entire
    /// observation and private ordered candidate binding from the owning session.
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.producer_sha256 != producer_sha256_v1()
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong Stromkirk artifact identity".into()));
        }
        let expected = roots()?;
        if self.roots != expected {
            return Err(RlContractError("wrong Stromkirk printed root".into()));
        }
        Ok(())
    }
}
fn roots() -> Result<Vec<RootV1>, RlContractError> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Ok(Vec::new());
    }
    let Some(id) = crate::card_def::card_id_by_name("Stromkirk Bloodthief") else {
        return Ok(Vec::new());
    };
    let selector = crate::rules_meaning_stromkirk_v1::live_selector()
        .map_err(|_| RlContractError("Stromkirk catalog not supported".into()))?;
    let meaning = crate::rules_meaning_stromkirk_v1::meaning(&selector, id, 0)
        .map_err(|_| RlContractError("Stromkirk root not supported".into()))?;
    Ok(vec![RootV1 {
        card_id: id,
        ability_index: 0,
        root_sha256: meaning.root_sha256,
    }])
}

/// Authenticates the actual selected root and original end-step observer/loss history.
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    roots().map_err(|error| error.0)?;
    if state
        .legend_pending_v1
        .iter()
        .flatten()
        .chain(
            state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|pending| pending.waiting_triggers()),
        )
        .chain(
            state
                .standard_v1
                .iter()
                .filter_map(|standard| standard.resolution_play.as_ref())
                .flat_map(|resolution| resolution.deferred_triggers.iter()),
        )
        .any(|pending| {
            matches!(
                pending.effect,
                crate::effect::EffectOp::Conditional {
                    cond: crate::effect::EffectCond::OpponentLostLifeThisTurn,
                    ..
                }
            )
        })
    {
        return Err("Stromkirk held legend/resolution transport is not admitted".into());
    }
    crate::effect::validate_pending_effect_choice(state)?;
    for pending in &state.engine.pending_triggers {
        crate::engine::validate_pending_trigger_identity(state, pending)?;
        if pending.placement_ordered {
            crate::engine::validate_pending_trigger(state, pending)?;
        }
    }
    for item in &state.stack {
        crate::engine::validated_stack_item_target_spec(item, state)?;
    }
    Ok(())
}
fn loss_contexts(
    state: &GameState,
    projection: &PublicObservationProjectionV5,
) -> Result<Vec<LossContextV1>, RlContractError> {
    let mut rows = Vec::new();
    for (index, pending) in state.engine.pending_triggers.iter().enumerate() {
        if !matches!(
            pending.effect,
            crate::effect::EffectOp::Conditional {
                cond: crate::effect::EffectCond::OpponentLostLifeThisTurn,
                ..
            }
        ) {
            continue;
        }
        let source = pending
            .source_contract
            .ok_or_else(|| RlContractError("Stromkirk pending source missing".into()))?;
        let reference = projection
            .surface
            .engine_context
            .pending_triggers
            .get(index)
            .and_then(|row| row.source.clone())
            .ok_or_else(|| RlContractError("Stromkirk pending public position missing".into()))?;
        rows.push(loss_context(
            state,
            source,
            reference,
            Some(
                u32::try_from(index)
                    .map_err(|_| RlContractError("Stromkirk pending position overflow".into()))?,
            ),
            None,
        )?);
    }
    for (index, item) in state.stack.iter().enumerate() {
        if !crate::stromkirk_bloodthief_v1::owns_item(item) {
            continue;
        }
        let source = item
            .v4
            .ability_source_contract
            .ok_or_else(|| RlContractError("Stromkirk stack source missing".into()))?;
        let reference = projection
            .surface
            .stack
            .get(index)
            .map(|row| row.source.clone())
            .ok_or_else(|| RlContractError("Stromkirk public stack position missing".into()))?;
        rows.push(loss_context(
            state,
            source,
            reference,
            None,
            Some(
                u32::try_from(index)
                    .map_err(|_| RlContractError("Stromkirk stack position overflow".into()))?,
            ),
        )?);
    }
    Ok(rows)
}
fn loss_context(
    state: &GameState,
    source: crate::state::AbilitySourceContractV4,
    reference: CardStableRefV1,
    pending: Option<u32>,
    stack: Option<u32>,
) -> Result<LossContextV1, RlContractError> {
    if reference.arena_id != source.source.0
        || reference.card_db_id != source.card_def
        || reference.zone_change_count != source.zone_change_count
    {
        return Err(RlContractError(
            "Stromkirk public source relation changed".into(),
        ));
    }
    Ok(LossContextV1 {
        source_at_end_step: reference,
        captured_controller: source.controller.into(),
        pending_trigger_ordinal: pending,
        live_stack_index: stack,
        opponent_lost_life_if_resolved_now: crate::life_loss_turn_v1::opponent_lost_life(
            state,
            source.controller,
        )
        .map_err(RlContractError)?,
    })
}
// V40 consumes these private typed parts; standalone predecessors retain their refusal path.
pub(crate) fn parts_for_akroma_v40(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
) -> Result<StromkirkObservationV1, RlContractError> {
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
) -> Result<StromkirkObservationV1, RlContractError> {
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
) -> Result<StromkirkObservationV1, RlContractError> {
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
) -> Result<StromkirkObservationV1, RlContractError> {
    let (substep_index, substep_count) = substep;

    validate_state(state).map_err(RlContractError)?;
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
    let loss_contexts = loss_contexts(state, &base.projection)?;
    let observation = StromkirkObservationV1 {
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
        lost_life_this_turn: if crate::life_loss_turn_v1::enabled() {
            Some(crate::life_loss_turn_v1::flags(state).map_err(RlContractError)?)
        } else {
            None
        },
        loss_contexts,
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
) -> Result<StromkirkObservationV1, RlContractError> {
    validate_state(state).map_err(RlContractError)?;
    let base = crate::policy_observation_v9::observe_policy_v9(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        substep_index,
        substep_count,
    )?;
    let loss_contexts = loss_contexts(state, &base.projection)?;
    let mut observation = StromkirkObservationV1 {
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
        extensions: base.extensions.predecessor,
        attacked_this_turn: base.extensions.attacked_this_turn,
        lost_life_this_turn: if crate::life_loss_turn_v1::enabled() {
            Some(crate::life_loss_turn_v1::flags(state).map_err(RlContractError)?)
        } else {
            None
        },
        loss_contexts,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    observation.projection_sha256 = commitment(&observation)?;
    Ok(observation)
}
