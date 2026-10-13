//! Separate public Gourmand transport. This is not V8/V9 or tensor/model admission.
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

pub const SCHEMA: &str = "mtg-kernel/gourmand-public/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestrictionV1 {
    /// Public identity at installation, masked if it was face down.
    pub recipient: CardStableRefV1,
    pub timestamp: u64,
    pub current_recipient: Option<CardStableRefV1>,
    /// True only while this exact incarnation remains on the battlefield.
    /// Later control or ability removal does not remove the restriction.
    pub effective: bool,
    pub duration: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootV1 {
    pub card_id: u16,
    pub trigger_index: u16,
    pub root_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GourmandObservationV1 {
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
    pub restrictions: Vec<RestrictionV1>,
    pub resolutions: Vec<ResolutionV1>,
    pub sacrifice_menu: Option<SacrificeMenuV1>,
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
        include_str!("gourmand_public_v1.rs"),
        include_str!("effect/vampire_gourmand_resolution_v1.rs"),
        include_str!("vampire_gourmand_v1.rs"),
        include_str!("vampire_gourmand_dispatch_v2.rs"),
        include_str!("effect/standard_selection_v1.rs"),
        include_str!("effect.rs"),
        include_str!("sower_of_chaos_v1.rs"),
        include_str!("sower_restriction_capture_v1.rs"),
        include_str!("trigger.rs"),
        include_str!("rules_meaning_gourmand_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("rl.rs"),
        include_str!("rl_session/gourmand_v29.rs"),
        include_str!("human_bo3_v1/gourmand_v29.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("human_bo3_v1/life_ward_v8.rs"),
        include_str!("engine.rs"),
        include_str!("event.rs"),
        include_str!("rl_session.rs"),
        include_str!("state.rs"),
        include_str!("policy_surface_v5.rs"),
        include_str!("surface_v2.rs"),
        include_str!("standard_creatures_v1.rs"),
        include_str!("standard_statics_v1.rs"),
        include_str!("standard_keywords_v1.rs"),
        include_str!("standard_legends_v1.rs"),
        include_str!("policy_observation_v8.rs"),
        include_str!("policy_observation_v7.rs"),
        include_str!("rl_session/flat_action_v3.rs"),
        include_str!("rl_session/flat_action_v4.rs"),
    ] {
        hash.update([0]);
        hash.update(source.as_bytes());
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn commitment(observation: &GourmandObservationV1) -> Result<String, RlContractError> {
    let mut value = serde_json::to_value(observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("Gourmand projection is not an object".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&value)?))
}
impl GourmandObservationV1 {
    /// Checks transport identity only. Execution always rederives the entire
    /// observation and private ordered candidate binding from the owning session.
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.producer_sha256 != producer_sha256_v1()
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong Gourmand artifact identity".into()));
        }
        let expected = roots()?;
        if self.roots != expected {
            return Err(RlContractError("wrong Gourmand printed root".into()));
        }
        Ok(())
    }
}
fn roots() -> Result<Vec<RootV1>, RlContractError> {
    // Ordinary Default/Standard sessions still have a real successor identity,
    // but never fabricate the missing Limited definition or claim its meaning.
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Ok(Vec::new());
    }
    let Some(id) = crate::card_def::card_id_by_name("Vampire Gourmand") else {
        return Ok(Vec::new());
    };
    let selector = crate::rules_meaning_gourmand_v1::live_selector()
        .map_err(|_| RlContractError("Gourmand catalog not supported".into()))?;
    let meaning = crate::rules_meaning_gourmand_v1::meaning(&selector, id, 0)
        .map_err(|_| RlContractError("Gourmand root not supported".into()))?;
    Ok(vec![RootV1 {
        card_id: id,
        trigger_index: 0,
        root_sha256: meaning.root_sha256,
    }])
}
// V40 consumes these private typed parts; standalone predecessors retain their refusal path.
pub(crate) fn parts_for_akroma_v40(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
) -> Result<GourmandObservationV1, RlContractError> {
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
) -> Result<GourmandObservationV1, RlContractError> {
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
) -> Result<GourmandObservationV1, RlContractError> {
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
fn parts_for_common_scope(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep: (u32, u32),
    scope: crate::rl::CommonProjectionScopeV42,
) -> Result<GourmandObservationV1, RlContractError> {
    let (substep_index, substep_count) = substep;

    validate_projection_authority(state)?;
    let restrictions = restrictions(state)?;
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
    let observation = GourmandObservationV1 {
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
        restrictions,
        resolutions: resolutions(state)?,
        sacrifice_menu: sacrifice_menu(state)?,
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
) -> Result<GourmandObservationV1, RlContractError> {
    validate_projection_authority(state)?;
    let restrictions = restrictions(state)?;
    let base = crate::rl::gourmand_observation_storage_v1(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let base = storage.base;
    let mut observation = GourmandObservationV1 {
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
        restrictions,
        resolutions: resolutions(state)?,
        sacrifice_menu: sacrifice_menu(state)?,
        roots: roots()?,
        projection_sha256: String::new(),
    };
    observation.projection_sha256 = commitment(&observation)?;
    Ok(observation)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionV1 {
    pub resolution_order: u32,
    pub source_at_attack: CardStableRefV1,
    pub captured_payer: PlayerSeatV1,
    pub pending_trigger_ordinal: Option<u32>,
    pub live_stack_index: Option<u32>,
    pub phase: String,
    pub paid: bool,
    pub draw_attempted: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SacrificeMenuV1 {
    pub source_at_attack: CardStableRefV1,
    pub payer: PlayerSeatV1,
    pub minimum: u16,
    pub maximum: u16,
    pub can_finish: bool,
    pub legal: Vec<CardStableRefV1>,
    pub selected: Vec<CardStableRefV1>,
}
fn source_ref(source: crate::state::AbilitySourceContractV4) -> CardStableRefV1 {
    CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: source.card_def,
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
        zone_change_count: source.zone_change_count,
    }
}
pub(crate) fn validate_projection_authority(state: &GameState) -> Result<(), RlContractError> {
    crate::effect::validate_pending_effect_choice(state).map_err(RlContractError)?;
    crate::vampire_gourmand_dispatch_v2::validate(state, None).map_err(RlContractError)?;
    let owned = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| {
            if let crate::event::CommittedEvent::VampireGourmandCostOperationV2 { operation } = e {
                operation.result.installed.as_ref()
            } else {
                None
            }
        })
        .filter(|marker| state.engine.until_end_of_turn.contains(marker))
        .count();
    let markers = state
        .engine
        .until_end_of_turn
        .iter()
        .filter(|e| {
            matches!(
                e,
                crate::engine::UntilEndOfTurnEffect::ResolvedObjectCantBeBlockedV1 { .. }
            )
        })
        .count();
    if owned != markers {
        return Err(RlContractError(
            "Gourmand successor refuses foreign unclassified evasion".into(),
        ));
    }
    Ok(())
}
fn restrictions(state: &GameState) -> Result<Vec<RestrictionV1>, RlContractError> {
    let mut rows = Vec::new();
    for event in &state.engine.event_history {
        let crate::event::CommittedEvent::VampireGourmandCostOperationV2 { operation } = event
        else {
            continue;
        };
        let Some(marker) = operation.result.installed.as_ref() else {
            continue;
        };
        if !state.engine.until_end_of_turn.contains(marker) {
            continue;
        }
        let crate::engine::UntilEndOfTurnEffect::ResolvedObjectCantBeBlockedV1 {
            object_id,
            object_zone_change_count,
            timestamp,
            ..
        } = marker
        else {
            return Err(RlContractError("wrong Gourmand rule".into()));
        };
        let source = operation
            .before
            .ctx
            .ability_source_contract
            .ok_or_else(|| RlContractError("Gourmand source absent".into()))?;
        let current = state
            .objects
            .try_get(*object_id)
            .filter(|o| {
                o.zone == Zone::Battlefield && o.zone_change_count == *object_zone_change_count
            })
            .map(|o| crate::sower_restriction_capture_v1::masked_ref(*object_id, o))
            .transpose()?;
        rows.push(RestrictionV1 {
            recipient: source_ref(source),
            timestamp: *timestamp,
            current_recipient: current.clone(),
            effective: current.is_some(),
            duration: "until_next_cleanup_including_repeated_cleanup".into(),
        });
    }
    rows.sort_by_key(|r| r.timestamp);
    Ok(rows)
}
fn sacrifice_menu(state: &GameState) -> Result<Option<SacrificeMenuV1>, RlContractError> {
    let Some(pending) = state.engine.pending_effect.as_ref() else {
        return Ok(None);
    };
    let Some(source) = pending.ctx.ability_source_contract else {
        return Ok(None);
    };
    if !crate::vampire_gourmand_dispatch_v2::owns_definition(source.card_def) {
        return Ok(None);
    }
    let Some(crate::effect::PendingEffectChoice::SelectTargets {
        player,
        legal,
        selected,
        min_targets,
        max_targets,
        purpose,
        ..
    }) = pending.choice.as_ref()
    else {
        return Ok(None);
    };
    let crate::effect::EffectTargetSelectionPurpose::SelectObjectsV1 { rule, .. } = purpose else {
        return Err(RlContractError("Gourmand cost purpose changed".into()));
    };
    if (crate::effect::EffectOp::SelectObjectsV1 { rule: *rule })
        != crate::vampire_gourmand_v1::program()
    {
        return Err(RlContractError("Gourmand cost rule changed".into()));
    }
    let refs = |rows: &[crate::effect::EffectTargetCandidate]| {
        rows.iter()
            .map(|row| {
                let binding = row.expected_object.ok_or_else(|| {
                    RlContractError("Gourmand candidate incarnation absent".into())
                })?;
                let object = state.objects.get(binding.object);
                if object.zone != Zone::Battlefield
                    || object.zone_change_count != binding.expected_zone_change_count
                {
                    return Err(RlContractError(
                        "Gourmand candidate became hidden or stale".into(),
                    ));
                }
                crate::sower_restriction_capture_v1::masked_ref(binding.object, object)
            })
            .collect::<Result<Vec<_>, RlContractError>>()
    };
    Ok(Some(SacrificeMenuV1 {
        source_at_attack: source_ref(source),
        payer: (*player).into(),
        minimum: *min_targets,
        maximum: *max_targets,
        can_finish: selected.len() >= usize::from(*min_targets),
        legal: refs(legal)?,
        selected: refs(selected)?,
    }))
}
fn resolutions(state: &GameState) -> Result<Vec<ResolutionV1>, RlContractError> {
    use crate::event::CommittedEvent;
    let mut rows = Vec::new();
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::VampireGourmandAttackCaptureV2 { source, .. } = event else {
            continue;
        };
        let published = state.engine.event_history.iter().find_map(|e| {
            if let CommittedEvent::VampireGourmandPublishedV2 {
                capture_history_index,
                item,
            } = e
            {
                (*capture_history_index as usize == at).then_some(item.as_ref())
            } else {
                None
            }
        });
        let cost = published.and_then(|item| {
            state.engine.event_history.iter().find_map(|e| {
                if let CommittedEvent::VampireGourmandCostOperationV2 { operation } = e {
                    (operation.before.resolving_item.v4.stack_item_id == item.v4.stack_item_id)
                        .then_some(operation.as_ref())
                } else {
                    None
                }
            })
        });
        let current = published.and_then(|item| {
            state
                .engine
                .pending_effect
                .as_ref()
                .filter(|p| p.resolving_item.v4.stack_item_id == item.v4.stack_item_id)
        });
        let countered=published.is_some_and(|item|state.engine.event_history.iter().any(|e|matches!(e,
            CommittedEvent::VampireGourmandCounterRemovedV2 {item:removed,..} if removed.v4.stack_item_id==item.v4.stack_item_id)));
        let pending = state.engine.pending_triggers.iter().position(|p| {
            p.source_contract == Some(*source) && p.effect == crate::vampire_gourmand_v1::program()
        });
        let phase = if countered {
            "countered"
        } else if let Some(cost) = cost {
            if cost.result.move_history_index.is_some() {
                "paid"
            } else if cost.result.selected.is_some() {
                "sacrifice_prevented"
            } else {
                "declined"
            }
        } else if current.is_some_and(|p| p.choice.is_some()) {
            "optional_sacrifice"
        } else if current.is_some() {
            "accepted_answer"
        } else if published.is_some() {
            "awaiting_priority"
        } else if pending.is_some() {
            "pending_placement"
        } else {
            "waiting_capture"
        };
        rows.push(ResolutionV1 {
            resolution_order: rows
                .len()
                .try_into()
                .map_err(|_| RlContractError("Gourmand history exceeds u32".into()))?,
            source_at_attack: source_ref(*source),
            captured_payer: source.controller.into(),
            pending_trigger_ordinal: if published.is_none() {
                pending
                    .map(u32::try_from)
                    .transpose()
                    .map_err(|_| RlContractError("Gourmand pending exceeds u32".into()))?
            } else {
                None
            },
            live_stack_index: published
                .and_then(|item| {
                    state
                        .stack
                        .iter()
                        .position(|i| i.v4.stack_item_id == item.v4.stack_item_id)
                })
                .map(u32::try_from)
                .transpose()
                .map_err(|_| RlContractError("Gourmand stack exceeds u32".into()))?,
            phase: phase.into(),
            paid: cost.is_some_and(|c| c.result.move_history_index.is_some()),
            draw_attempted: cost.is_some_and(|c| c.result.draw_history_index.is_some()),
        });
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_observation_v6::tests::{put, ready_state};
    #[test]
    fn actual_common_v8_projection_and_hidden_hand_substitution_match_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = crate::policy_observation_v6::tests::ready_state_for(actor);
            put(&mut state, actor, "Forest", Zone::Hand);
            let hidden = put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
            let surface = PolicySurfaceV5::new();
            let original = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
            let old = crate::rl::observe_policy_v8(&state, &surface, actor, 0, 0, 0, 1).unwrap();
            assert_eq!(original.projection, old.projection);
            assert_eq!(original.extensions, old.extensions);
            assert_eq!(original.known_face_down_cards, old.known_face_down_cards);
            original.validate_artifact().unwrap();
            let definition = crate::card_def::card_id_by_name("Mountain").unwrap();
            let object = state.objects.get_mut(hidden);
            object.card_def = definition;
            object.name = "Mountain".into();
            object.v4 = crate::state::ObjectStateV4::from_card_def(definition);
            assert_eq!(
                observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap(),
                original
            );
        }
    }
    #[test]
    fn successor_refuses_foreign_unclassified_rule_before_export() {
        let mut state = ready_state();
        let source = put(
            &mut state,
            PlayerId::P0,
            "Llanowar Elves",
            Zone::Battlefield,
        );
        state.engine.until_end_of_turn.push(
            crate::engine::UntilEndOfTurnEffect::ResolvedObjectCantBeBlockedV1 {
                object_id: source,
                object_zone_change_count: 0,
                timestamp: 1,
                duration: crate::engine::EffectDuration::EndOfTurn,
            },
        );
        let bytes = serde_json::to_vec(&state).unwrap();
        assert!(observe_v1(&state, &PolicySurfaceV5::new(), PlayerId::P0, 0, 0, 0, 1).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
    }
}
