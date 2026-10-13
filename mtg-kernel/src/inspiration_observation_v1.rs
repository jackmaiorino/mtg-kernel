//! Exact Inspiration choice transport, with scoped chooser-only mill visibility.
use crate::effect::{EffectTargetSelectionPurpose, PendingEffectChoice};
use crate::ids::{ObjectId, PlayerId};
use crate::policy_observation_v8::PolicyObservationExtensionsV8;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, CardStableRefV1, KnownLibraryCardV4, PlayerSeatV1,
    PublicObservationProjectionV5, RlContractError,
};
use crate::rules_meaning_inspiration_v1::{self as meaning, CastRootV1, MeaningV1};
use crate::state::{GameState, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/inspiration-observation/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhaseV1 {
    MillOrder,
    ReturnCard,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceV1 {
    pub source: CardStableRefV1,
    pub controller: PlayerSeatV1,
    pub cast_root: CastRootV1,
    pub phase: PhaseV1,
    pub selected: Vec<CardStableRefV1>,
    pub legal: Vec<CardStableRefV1>,
    pub minimum: u16,
    pub maximum: u16,
    pub ordered: bool,
    pub can_finish: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspirationObservationV1 {
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
    pub meanings: Vec<MeaningV1>,
    pub choice: Option<ChoiceV1>,
    pub projection_sha256: String,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    let mut h = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("inspiration_observation_v1.rs"),
        include_str!("rules_meaning_inspiration_v1.rs"),
        include_str!("effect/inspiration_resolution_v1.rs"),
        include_str!("effect/standard_selection_v1.rs"),
        include_str!("effect.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("state.rs"),
        include_str!("rl.rs"),
        include_str!("rl_session.rs"),
        include_str!("rl_session/inspiration_v37.rs"),
        include_str!("human_bo3_v1/inspiration_v37.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("human_bo3_v1/life_ward_v8.rs"),
        include_str!("policy_observation_v8.rs"),
        include_str!("policy_observation_v7.rs"),
        include_str!("rl_session/flat_action_v3.rs"),
        include_str!("rl_session/flat_action_v4.rs"),
        include_str!("policy_surface_v5.rs"),
        include_str!("surface_v2.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("standard_statics_v1.rs"),
        include_str!("standard_keywords_v1.rs"),
        include_str!("standard_legends_v1.rs"),
    ] {
        h.update([0]);
        h.update(source.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
fn meanings() -> Result<Vec<MeaningV1>, RlContractError> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Ok(vec![]);
    }
    let Some(id) = crate::card_def::card_id_by_name("Inspiration from Beyond") else {
        return Ok(vec![]);
    };
    let selector = meaning::live_selector()
        .map_err(|_| RlContractError("unsupported Inspiration selector".into()))?;
    [CastRootV1::Normal, CastRootV1::Flashback]
        .into_iter()
        .map(|root| {
            meaning::meaning(&selector, id, root)
                .map_err(|_| RlContractError("wrong Inspiration printed root".into()))
        })
        .collect()
}
fn selected(id: u16) -> bool {
    crate::card_def::CARD_DEFS
        .get(usize::from(id))
        .is_some_and(|d| d.name == "Inspiration from Beyond")
}
pub(crate) fn requires_successor(state: &GameState) -> bool {
    state.stack.iter().any(|i| {
        i.v4.source_contract.is_some_and(|c| selected(c.card_def))
            || state
                .objects
                .try_get(i.source)
                .is_some_and(|o| selected(o.card_def))
    }) || state.engine.pending_effect.as_ref().is_some_and(|p| {
        state
            .objects
            .try_get(p.ctx.source)
            .is_some_and(|o| selected(o.card_def))
    })
}
pub(crate) fn validate_projection_authority(state: &GameState) -> Result<(), RlContractError> {
    crate::effect::validate_pending_effect_choice(state).map_err(RlContractError)?;
    if !requires_successor(state) {
        return Ok(());
    }
    if meanings()?.len() != 2 {
        return Err(RlContractError(
            "selected catalog has no exact Inspiration roots".into(),
        ));
    }
    if let Some(p) = state.engine.pending_effect.as_ref() {
        if state
            .objects
            .try_get(p.ctx.source)
            .is_some_and(|o| selected(o.card_def))
        {
            let item = &p.resolving_item;
            crate::engine::validated_stack_item_target_spec(item, state)
                .map_err(RlContractError)?;
            let live: Vec<_> = state
                .stack
                .iter()
                .filter(|i| i.v4.stack_item_id == item.v4.stack_item_id)
                .collect();
            if live.len() != 1
                || live[0] != item
                || item.controller != p.ctx.controller
                || item.source != p.ctx.source
            {
                return Err(RlContractError(
                    "Inspiration lost exact live resolving owner".into(),
                ));
            }
            let receipt =
                state
                    .engine
                    .event_history
                    .iter()
                    .rev()
                    .find_map(|e| match e {
                        crate::event::CommittedEvent::InspirationResolutionBoundaryV1 {
                            receipt,
                        } if receipt.stack_item_id == item.v4.stack_item_id => Some(receipt),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        RlContractError("Inspiration lacks committed private boundary".into())
                    })?;
            if receipt.pending.as_deref() != Some(p)
                || receipt.source.source != item.source
                || receipt.source.controller != p.ctx.controller
            {
                return Err(RlContractError(
                    "Inspiration differs from committed private boundary".into(),
                ));
            }
        }
    }
    Ok(())
}
/// Separate private authority. It grants no persistent library knowledge and
/// is called only by selected V37 storage/actions after authenticating the owner.
pub(crate) fn mill_card_ref(
    state: &GameState,
    actor: PlayerId,
    id: ObjectId,
) -> Result<CardStableRefV1, RlContractError> {
    validate_projection_authority(state)?;
    let p = state
        .engine
        .pending_effect
        .as_ref()
        .ok_or_else(|| RlContractError("missing private mill owner".into()))?;
    let Some(PendingEffectChoice::SelectTargets {
        player,
        selected: chosen,
        legal,
        purpose: EffectTargetSelectionPurpose::OrderMilledIntoGraveyard,
        ..
    }) = p.choice.as_ref()
    else {
        return Err(RlContractError("not an owned private mill choice".into()));
    };
    if *player != actor
        || p.ctx.controller != actor
        || !state
            .objects
            .try_get(p.ctx.source)
            .is_some_and(|o| selected(o.card_def))
    {
        return Err(RlContractError(
            "private mill identities belong only to the original chooser".into(),
        ));
    }
    let started = p
        .resolving_item
        .v4
        .inspiration_started_v1
        .0
        .ok_or_else(|| RlContractError("private mill lost original started row".into()))?;
    let inputs = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            crate::event::CommittedEvent::InspirationResolutionInputV1 { input }
                if input.previous == started
                    && input.kind == crate::effect::InspirationInputKindV1::Drive
                    && input.pending.resolving_item == p.resolving_item
                    && input.pending.ctx == p.ctx =>
            {
                input.mill.as_ref()
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [original] = inputs.as_slice() else {
        return Err(RlContractError(
            "private mill lacks unique genuine initial prefix".into(),
        ));
    };
    let current = chosen
        .iter()
        .chain(legal)
        .map(|c| {
            c.expected_object
                .ok_or_else(|| RlContractError("private mill card lacks bound incarnation".into()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if current.len() != original.len()
        || original.len() > 3
        || original
            .iter()
            .any(|binding| current.iter().filter(|b| *b == binding).count() != 1)
    {
        return Err(RlContractError(
            "private mill partition differs from original owned input".into(),
        ));
    }
    let binding = chosen
        .iter()
        .chain(legal)
        .find_map(|c| c.expected_object.filter(|b| b.object == id))
        .ok_or_else(|| RlContractError("card is outside actual mill prefix".into()))?;
    let o = state
        .objects
        .try_get(id)
        .ok_or_else(|| RlContractError("mill card absent".into()))?;
    if o.owner != actor
        || o.zone != Zone::Library
        || binding.expected_zone != Zone::Library
        || o.zone_change_count != binding.expected_zone_change_count
    {
        return Err(RlContractError(
            "private mill card incarnation changed".into(),
        ));
    }
    crate::rl::card_ref(state, id)
}
pub(crate) fn current_choice(
    state: &GameState,
    actor: PlayerId,
) -> Result<Option<ChoiceV1>, RlContractError> {
    validate_projection_authority(state)?;
    let Some(p) = state.engine.pending_effect.as_ref() else {
        return Ok(None);
    };
    if !state
        .objects
        .try_get(p.ctx.source)
        .is_some_and(|o| selected(o.card_def))
    {
        return Ok(None);
    }
    let Some(PendingEffectChoice::SelectTargets {
        player,
        selected: chosen,
        legal,
        min_targets,
        max_targets,
        ordered,
        purpose,
        ..
    }) = p.choice.as_ref()
    else {
        return Ok(None);
    };
    let phase = match purpose {
        EffectTargetSelectionPurpose::OrderMilledIntoGraveyard => PhaseV1::MillOrder,
        EffectTargetSelectionPurpose::SelectObjectsV1 { rule, .. }
            if rule.filter == crate::effect::ObjectSelectionFilterV1::InstantOrSorcery =>
        {
            PhaseV1::ReturnCard
        }
        _ => {
            return Err(RlContractError(
                "unsupported Inspiration choice stage".into(),
            ))
        }
    };
    if phase == PhaseV1::MillOrder && *player != actor {
        return Err(RlContractError(
            "private Inspiration mill view is chooser-only".into(),
        ));
    }
    let refs = |cards: &[crate::effect::EffectTargetCandidate]| {
        cards
            .iter()
            .map(|c| {
                let crate::state::Target::Object(id) = c.target else {
                    return Err(RlContractError("Inspiration chose a noncard".into()));
                };
                if phase == PhaseV1::MillOrder {
                    mill_card_ref(state, actor, id)
                } else {
                    crate::rl::card_ref(state, id)
                }
            })
            .collect::<Result<Vec<_>, _>>()
    };
    Ok(Some(ChoiceV1 {
        source: crate::rl::card_ref(state, p.ctx.source)?,
        controller: (*player).into(),
        cast_root: if p.resolving_item.is_flashback {
            CastRootV1::Flashback
        } else {
            CastRootV1::Normal
        },
        phase,
        selected: refs(chosen)?,
        legal: refs(legal)?,
        minimum: *min_targets,
        maximum: *max_targets,
        ordered: *ordered,
        can_finish: chosen.len() >= usize::from(*min_targets),
    }))
}
fn commitment(o: &InspirationObservationV1) -> Result<String, RlContractError> {
    let mut v = serde_json::to_value(o)?;
    v.as_object_mut()
        .ok_or_else(|| RlContractError("wrong Inspiration object".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&v)?))
}
impl InspirationObservationV1 {
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.producer_sha256 != producer_sha256_v1()
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.meanings != meanings()?
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong Inspiration artifact".into()));
        }
        Ok(())
    }
}
// V40 consumes these private typed parts; standalone predecessors retain their refusal path.
pub(crate) fn parts_for_akroma_v40(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
) -> Result<InspirationObservationV1, RlContractError> {
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
) -> Result<InspirationObservationV1, RlContractError> {
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
fn parts_for_common_scope(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
    scope: crate::rl::CommonProjectionScopeV42,
) -> Result<InspirationObservationV1, RlContractError> {
    let (substep_index, substep_count) = substep;

    let choice = current_choice(state, actor)?;
    let base = crate::rl::common_family_observation_storage_scoped(
        (scope, state),
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let b = storage.base;
    let o = InspirationObservationV1 {
        schema: String::new(),
        producer_sha256: String::new(),
        kernel_version: b.kernel_version,
        surface_version: b.surface_version,
        policy_surface_version: b.policy_surface_version,
        card_db_hash: b.card_db_hash,
        acting_player: b.acting_player,
        step_index: b.step_index,
        physical_decision_id: b.physical_decision_id,
        substep_index: b.substep_index,
        substep_count: b.substep_count,
        projection: b.projection,
        own_hand: b.own_hand,
        known_library_cards: b.known_library_cards,
        known_hand_cards: b.known_hand_cards,
        known_face_down_cards: b.known_face_down_cards,
        extensions: storage.extensions,
        attacked_this_turn: [
            state.player_attacked_this_turn_v1(PlayerId::P0),
            state.player_attacked_this_turn_v1(PlayerId::P1),
        ],
        meanings: meanings()?,
        choice,
        projection_sha256: String::new(),
    };
    Ok(o)
}
pub fn observe_v1(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
) -> Result<InspirationObservationV1, RlContractError> {
    let choice = current_choice(state, actor)?;
    let base = crate::rl::inspiration_observation_storage_v1(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let b = storage.base;
    let mut o = InspirationObservationV1 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256_v1(),
        kernel_version: b.kernel_version,
        surface_version: b.surface_version,
        policy_surface_version: b.policy_surface_version,
        card_db_hash: b.card_db_hash,
        acting_player: b.acting_player,
        step_index: b.step_index,
        physical_decision_id: b.physical_decision_id,
        substep_index: b.substep_index,
        substep_count: b.substep_count,
        projection: b.projection,
        own_hand: b.own_hand,
        known_library_cards: b.known_library_cards,
        known_hand_cards: b.known_hand_cards,
        known_face_down_cards: b.known_face_down_cards,
        extensions: storage.extensions,
        attacked_this_turn: [
            state.player_attacked_this_turn_v1(PlayerId::P0),
            state.player_attacked_this_turn_v1(PlayerId::P1),
        ],
        meanings: meanings()?,
        choice,
        projection_sha256: String::new(),
    };
    o.projection_sha256 = commitment(&o)?;
    Ok(o)
}

#[cfg(test)]
pub(crate) mod tests;
