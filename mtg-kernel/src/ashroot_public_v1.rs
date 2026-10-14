//! Actual V9 projection plus original attack-source power. Frozen producers remain unchanged.
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

pub const SCHEMA: &str = "mtg-kernel/ashroot-public/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerContextV1 {
    pub source_at_attack: CardStableRefV1,
    pub captured_controller: PlayerSeatV1,
    pub pending_trigger_ordinal: Option<u32>,
    pub live_stack_index: Option<u32>,
    pub power_if_resolved_now: i32,
    pub uses_actual_departure_power: bool,
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
pub struct AshrootObservationV1 {
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
    pub power_contexts: Vec<PowerContextV1>,
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
        include_str!("ashroot_public_v1.rs"),
        include_str!("ashroot_animist_v1.rs"),
        include_str!("ashroot_dispatch_v2.rs"),
        include_str!("trigger.rs"),
        include_str!("effect.rs"),
        include_str!("event.rs"),
        include_str!("effect/temporary_control_v1.rs"),
        include_str!("rules_meaning_ashroot_v1.rs"),
        include_str!("engine.rs"),
        include_str!("policy_surface_v5.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("standard_statics_v1.rs"),
        include_str!("standard_keywords_v1.rs"),
        include_str!("standard_legends_v1.rs"),
        include_str!("creature_stats_lki_v1.rs"),
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
        include_str!("rl_session/ashroot_v27.rs"),
        include_str!("rl_session/flat_action_v3.rs"),
        include_str!("rl_session/flat_action_v4.rs"),
        include_str!("rl_session/human_v8.rs"),
        include_str!("human_bo3_v1/ashroot_v27.rs"),
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
fn commitment(observation: &AshrootObservationV1) -> Result<String, RlContractError> {
    let mut value = serde_json::to_value(observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("Ashroot projection is not an object".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&value)?))
}
impl AshrootObservationV1 {
    /// Checks transport identity only. Execution always rederives the entire
    /// observation and private ordered candidate binding from the owning session.
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.producer_sha256 != producer_sha256_v1()
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong Ashroot artifact identity".into()));
        }
        let expected = roots()?;
        if self.roots != expected {
            return Err(RlContractError("wrong Ashroot printed root".into()));
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
    let Some(id) = crate::card_def::card_id_by_name("Ashroot Animist") else {
        return Ok(Vec::new());
    };
    let selector = crate::rules_meaning_ashroot_v1::live_selector()
        .map_err(|_| RlContractError("Ashroot catalog not supported".into()))?;
    let meaning = crate::rules_meaning_ashroot_v1::meaning(&selector, id, 0)
        .map_err(|_| RlContractError("Ashroot root not supported".into()))?;
    Ok(vec![RootV1 {
        card_id: id,
        ability_index: 0,
        root_sha256: meaning.root_sha256,
    }])
}

/// Authenticates selected roots, actual declaration and departing-power provenance.
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    roots().map_err(|error| error.0)?;
    crate::ashroot_animist_v1::validate_state(state)?;
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
                crate::effect::EffectOp::AshrootAttackBoostV1 { .. }
                    | crate::effect::EffectOp::AshrootBoostFromCaptureV2 { .. }
            )
        })
    {
        return Err("Ashroot held legend/resolution transport is not admitted".into());
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
fn power_contexts(
    state: &GameState,
    projection: &PublicObservationProjectionV5,
) -> Result<Vec<PowerContextV1>, RlContractError> {
    let mut rows = Vec::new();
    for (index, pending) in state.engine.pending_triggers.iter().enumerate() {
        if !matches!(
            pending.effect,
            crate::effect::EffectOp::AshrootAttackBoostV1 { .. }
                | crate::effect::EffectOp::AshrootBoostFromCaptureV2 { .. }
        ) {
            continue;
        }
        let source = pending
            .source_contract
            .ok_or_else(|| RlContractError("Ashroot pending source missing".into()))?;
        let reference = projection
            .surface
            .engine_context
            .pending_triggers
            .get(index)
            .and_then(|row| row.source.clone())
            .ok_or_else(|| RlContractError("Ashroot pending public position missing".into()))?;
        rows.push(power_context(
            state,
            source,
            reference,
            Some(
                u32::try_from(index)
                    .map_err(|_| RlContractError("Ashroot pending position overflow".into()))?,
            ),
            None,
        )?);
    }
    for (index, item) in state.stack.iter().enumerate() {
        if !crate::ashroot_animist_v1::owns_item(item) {
            continue;
        }
        let source = item
            .v4
            .ability_source_contract
            .ok_or_else(|| RlContractError("Ashroot stack source missing".into()))?;
        let reference = projection
            .surface
            .stack
            .get(index)
            .map(|row| row.source.clone())
            .ok_or_else(|| RlContractError("Ashroot public stack position missing".into()))?;
        rows.push(power_context(
            state,
            source,
            reference,
            None,
            Some(
                u32::try_from(index)
                    .map_err(|_| RlContractError("Ashroot stack position overflow".into()))?,
            ),
        )?);
    }
    Ok(rows)
}
fn power_context(
    state: &GameState,
    source: crate::state::AbilitySourceContractV4,
    reference: CardStableRefV1,
    pending: Option<u32>,
    stack: Option<u32>,
) -> Result<PowerContextV1, RlContractError> {
    if reference.arena_id != source.source.0
        || reference.card_db_id != source.card_def
        || reference.zone_change_count != source.zone_change_count
    {
        return Err(RlContractError(
            "Ashroot public source relation changed".into(),
        ));
    }
    Ok(PowerContextV1 {
        source_at_attack: reference,
        captured_controller: source.controller.into(),
        pending_trigger_ordinal: pending,
        live_stack_index: stack,
        power_if_resolved_now: crate::ashroot_animist_v1::sample_power(state, source)
            .map_err(RlContractError)?,
        uses_actual_departure_power: state.objects.try_get(source.source).is_none_or(|live| {
            live.zone != source.zone || live.zone_change_count != source.zone_change_count
        }),
    })
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
) -> Result<AshrootObservationV1, RlContractError> {
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
    let power_contexts = power_contexts(state, &base.projection)?;
    let mut observation = AshrootObservationV1 {
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
        power_contexts,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    observation.projection_sha256 = commitment(&observation)?;
    Ok(observation)
}
