//! Scoped Divine cast facts. All original contracts and payment plans stay private.
use crate::ids::PlayerId;
use crate::policy_observation_variable_cast_v1::{variable_cast_context, VariableCastContextV1};
use crate::policy_surface_v5::PolicyDecisionV5;
use crate::rl::{CardStableRefV1, PlayerSeatV1, RlContractError};
use crate::rules_meaning_divine_v44::{self as meaning, MeaningV44};
use crate::state::{GameState, StackItem, Target};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuoteV44 {
    pub floating_mana_used: [u8; 6],
    pub tapped_sources: Vec<(CardStableRefV1, crate::mana::ManaColor)>,
    pub restricted_mana_units_used: u64,
    pub life_paid: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingV44 {
    pub controller: PlayerSeatV1,
    pub source: CardStableRefV1,
    pub kicked: Option<bool>,
    pub selected: Vec<CardStableRefV1>,
    pub minimum: Option<u64>,
    pub maximum: Option<u64>,
    pub context: Option<VariableCastContextV1>,
    pub unkicked_quote: Option<QuoteV44>,
    pub kicked_quote: Option<QuoteV44>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalizedV44 {
    pub stack_ordinal: u32,
    pub source: CardStableRefV1,
    pub controller: PlayerSeatV1,
    pub kicked: bool,
    pub targets: Vec<CardStableRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV44 {
    pub meanings: Vec<MeaningV44>,
    pub pending: Option<PendingV44>,
    pub finalized: Vec<FinalizedV44>,
}
/// Project exact target incarnations with the existing authenticated disguise history.
/// The original private target contract remains in the owning binding.
pub(crate) fn target_ref_v44(
    state: &GameState,
    contract: crate::state::StackTargetContractV4,
) -> Result<CardStableRefV1, RlContractError> {
    let mut reference = crate::policy_observation_variable_cast_v1::captured_target_ref(contract)?;
    let id = crate::ids::ObjectId(reference.arena_id);
    let live = state
        .objects
        .try_get(id)
        .ok_or_else(|| RlContractError("Divine target object absent".into()))?;
    if live.zone_change_count == reference.zone_change_count {
        reference.card_db_id =
            crate::rl::projected_card_def(state, id, reference.zone_change_count);
    } else {
        // A newer hidden incarnation cannot disclose or redefine the historical
        // recipient. Use only an authenticated original battlefield departure.
        crate::controlled_creature_grants_v1::validate_color_history(state)
            .map_err(RlContractError)?;
        let mut snapshots = state.engine.event_history.iter().filter_map(|e| {
            let crate::event::CommittedEvent::ProtectionSourceColorDepartureV1 { snapshot, .. } = e
            else {
                return None;
            };
            (snapshot.source.source == id
                && snapshot.source.zone_change_count == reference.zone_change_count
                && snapshot.source.card_def == reference.card_db_id
                && PlayerSeatV1::from(snapshot.source.owner) == reference.owner)
                .then_some(snapshot)
        });
        let snapshot = snapshots.next().ok_or_else(|| {
            RlContractError("Divine departed target has no original visibility occurrence".into())
        })?;
        if snapshots.next().is_some() {
            return Err(RlContractError(
                "Divine target visibility occurrence duplicated".into(),
            ));
        }
        let original: crate::state::GameObject =
            serde_json::from_str(&snapshot.private_snapshot_json)
                .map_err(|e| RlContractError(e.to_string()))?;
        if let Some(face) = original.v4.face_down_v1 {
            reference.card_db_id = crate::card_def::card_id_by_name(if face.disguised {
                "Face-down creature"
            } else {
                "Face-down card"
            })
            .ok_or_else(|| RlContractError("Divine hidden target sentinel absent".into()))?;
        }
    }
    Ok(reference)
}
fn context_v44(
    state: &GameState,
    decision: &PolicyDecisionV5,
) -> Result<Option<VariableCastContextV1>, RlContractError> {
    let mut context = variable_cast_context(state, decision)?;
    if let Some(VariableCastContextV1::Targets {
        selected,
        legal_choices,
        ..
    }) = &mut context
    {
        let pending = state.engine.pending_cast.as_ref().ok_or_else(|| {
            RlContractError("Divine context lost original pending authority".into())
        })?;
        *selected = pending
            .target_contracts
            .iter()
            .copied()
            .map(|c| target_ref_v44(state, c))
            .collect::<Result<Vec<_>, _>>()?;
        for reference in legal_choices {
            let contract = crate::state::StackTargetContractV4::capture(
                state,
                Target::Object(crate::ids::ObjectId(reference.arena_id)),
            );
            *reference = target_ref_v44(state, contract)?;
        }
    }
    Ok(context)
}
fn owns(id: u16) -> bool {
    crate::card_def::CARD_DEFS
        .get(usize::from(id))
        .is_some_and(|def| {
            def.target_spec == crate::card_def::TargetSpec::KickerControlledCreatures
        })
}
pub(crate) fn meanings() -> Result<Vec<MeaningV44>, RlContractError> {
    let selector = meaning::live_selector().map_err(RlContractError)?;
    crate::card_def::CARD_DEFS
        .iter()
        .enumerate()
        .filter(|(_, def)| {
            def.target_spec == crate::card_def::TargetSpec::KickerControlledCreatures
        })
        .map(|(id, _)| meaning::meaning(&selector, id as u16).map_err(RlContractError))
        .collect()
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    let selector = meaning::live_selector()?;
    if let Some(pending) = &state.engine.pending_cast {
        crate::engine::validate_pending_cast(state, pending)?;
        if pending.target_spec == crate::card_def::TargetSpec::KickerControlledCreatures
            || owns(pending.source_contract.card_def)
        {
            meaning::meaning(&selector, pending.source_contract.card_def)?;
            if pending.origin_zone != crate::state::Zone::Hand {
                return Err("Divine V44 supports the genuine ordinary hand cast only".into());
            }
        }
    }
    for item in state.stack.iter().chain(
        state
            .engine
            .pending_effect
            .as_ref()
            .map(|p| &p.resolving_item),
    ) {
        if item.v4.target_spec == Some(crate::card_def::TargetSpec::KickerControlledCreatures)
            || item
                .v4
                .source_contract
                .is_some_and(|source| owns(source.card_def))
        {
            let source = item
                .v4
                .source_contract
                .ok_or("Divine finalized source absent")?;
            meaning::meaning(&selector, source.card_def)?;
            if item.kind != crate::state::StackItemKind::Spell
                || item.is_copy
                || item.inline_effect.is_some()
                || item.v4.cast_method != Some(crate::state::CastMethodV4::Normal)
                || item.v4.granted_by.is_some()
                || source.zone != crate::state::Zone::Stack
            {
                return Err("Divine V44 refuses foreign/copied variable producers".into());
            }
            crate::engine::validated_stack_item_target_spec(item, state)?;
            for contract in &item.v4.target_contracts {
                target_ref_v44(state, *contract).map_err(|e| e.0)?;
            }
        }
    }
    Ok(())
}
fn quote(state: &GameState, kicked: bool) -> Result<Option<QuoteV44>, RlContractError> {
    crate::engine::divine_pending_quote_v44(state, kicked)
        .map_err(RlContractError)?
        .map(|plan| {
            let tapped_sources = plan
                .taps
                .iter()
                .map(|(id, color)| {
                    let object = state
                        .objects
                        .try_get(*id)
                        .ok_or_else(|| RlContractError("Divine quote source absent".into()))?;
                    Ok((
                        crate::sower_restriction_capture_v1::masked_ref(*id, object)?,
                        *color,
                    ))
                })
                .collect::<Result<Vec<_>, RlContractError>>()?;
            Ok(QuoteV44 {
                floating_mana_used: plan.pool_used,
                tapped_sources,
                restricted_mana_units_used: u64::try_from(plan.restricted_pool_used.len())
                    .map_err(|_| RlContractError("Divine quote count overflow".into()))?,
                life_paid: plan.life_paid,
            })
        })
        .transpose()
}
pub(crate) fn finalized_target_refs_v44(
    state: &GameState,
    item: &StackItem,
) -> Result<Vec<crate::rl::TargetRefV1>, RlContractError> {
    validate_state(state).map_err(RlContractError)?;
    crate::engine::validated_stack_item_target_spec(item, state).map_err(RlContractError)?;
    item.v4
        .target_contracts
        .iter()
        .copied()
        .map(|contract| {
            target_ref_v44(state, contract).map(|object| crate::rl::TargetRefV1::Object { object })
        })
        .collect()
}
fn finalized(
    state: &GameState,
    item: &StackItem,
    ordinal: usize,
) -> Result<FinalizedV44, RlContractError> {
    crate::engine::validated_stack_item_target_spec(item, state).map_err(RlContractError)?;
    let source = crate::rl::card_ref(state, item.source)?;
    let targets = item
        .v4
        .target_contracts
        .iter()
        .copied()
        .map(|contract| target_ref_v44(state, contract))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FinalizedV44 {
        stack_ordinal: u32::try_from(ordinal)
            .map_err(|_| RlContractError("Divine stack ordinal overflow".into()))?,
        source,
        controller: item.controller.into(),
        kicked: item.kicked,
        targets,
    })
}
pub(crate) fn facts(
    state: &GameState,
    decision: &PolicyDecisionV5,
    observer: PlayerId,
) -> Result<FactsV44, RlContractError> {
    validate_state(state).map_err(RlContractError)?;
    let selector = meaning::live_selector().map_err(RlContractError)?;
    let meanings = crate::card_def::CARD_DEFS
        .iter()
        .enumerate()
        .filter(|(_, def)| {
            def.target_spec == crate::card_def::TargetSpec::KickerControlledCreatures
        })
        .map(|(id, _)| meaning::meaning(&selector, id as u16).map_err(RlContractError))
        .collect::<Result<Vec<_>, _>>()?;
    let pending = state
        .engine
        .pending_cast
        .as_ref()
        .filter(|pending| owns(pending.source_contract.card_def) && pending.controller == observer)
        .map(|pending| {
            let selected = pending
                .target_contracts
                .iter()
                .copied()
                .map(|contract| target_ref_v44(state, contract))
                .collect::<Result<Vec<_>, _>>()?;
            if selected.len() != pending.targets_chosen.len()
                || selected.iter().enumerate().any(|(i, target)| {
                    selected[..i]
                        .iter()
                        .any(|old| old.arena_id == target.arena_id)
                })
                || pending
                    .targets_chosen
                    .iter()
                    .any(|target| !matches!(target, Target::Object(_)))
            {
                return Err(RlContractError(
                    "Divine exact selected roster differs".into(),
                ));
            }
            let source = CardStableRefV1 {
                arena_id: pending.source_contract.source.0,
                card_db_id: pending.source_contract.card_def,
                owner: pending.source_contract.owner.into(),
                controller: pending.source_contract.controller.into(),
                zone: pending.source_contract.zone,
                zone_change_count: pending.source_contract.zone_change_count,
            };
            Ok(PendingV44 {
                controller: pending.controller.into(),
                source,
                kicked: pending.kicked,
                selected,
                minimum: pending.kicked.map(|k| if k { 0 } else { 1 }),
                maximum: pending.kicked.and_then(|k| if k { None } else { Some(1) }),
                context: context_v44(state, decision)?,
                unkicked_quote: quote(state, false)?,
                kicked_quote: quote(state, true)?,
            })
        })
        .transpose()?;
    let finalized = state
        .stack
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item.v4
                .source_contract
                .is_some_and(|source| owns(source.card_def))
        })
        .map(|(ordinal, item)| finalized(state, item, ordinal))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FactsV44 {
        meanings,
        pending,
        finalized,
    })
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests {
    use super::*;
    use crate::engine::{self, Action};
    use crate::policy_observation_v6::tests::{put, ready_state_for};
    use crate::policy_surface_v5::PolicySurfaceV5;
    use crate::state::Zone;
    #[test]
    fn actual_divine_kicker_quotes_roster_zero_finish_and_both_view_privacy() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            let source = put(&mut state, actor, "Divine Resilience", Zone::Hand);
            let first = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let second = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
            state.players[actor.index()].mana_pool[crate::mana::ManaColor::C.pool_index()] = 2;
            state.players[actor.index()].mana_pool[crate::mana::ManaColor::W.pool_index()] = 2;
            engine::step(&mut state, Action::CastSpell(source)).unwrap();
            let mut surface = PolicySurfaceV5::new_for_session();
            let origin = surface.next_decision(&mut state).unwrap();
            let own = facts(&state, &origin, actor).unwrap();
            let pending = own.pending.as_ref().unwrap();
            assert!(matches!(
                pending.context,
                Some(VariableCastContextV1::Kicker { .. })
            ));
            assert_eq!(
                pending.unkicked_quote.as_ref().unwrap().floating_mana_used
                    [crate::mana::ManaColor::W.pool_index()],
                1
            );
            assert_eq!(
                pending.kicked_quote.as_ref().unwrap().floating_mana_used
                    [crate::mana::ManaColor::W.pool_index()],
                2
            );
            assert!(facts(&state, &origin, actor.opponent())
                .unwrap()
                .pending
                .is_none());
            let before = serde_json::to_vec(&state).unwrap();
            assert_eq!(facts(&state, &origin, actor).unwrap(), own);
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            engine::step(&mut state, Action::ChooseKicker(true)).unwrap();
            let origin = surface.next_decision(&mut state).unwrap();
            let own = facts(&state, &origin, actor).unwrap();
            let Some(VariableCastContextV1::Targets {
                legal_choices,
                minimum,
                maximum,
                can_finish,
                ..
            }) = &own.pending.unwrap().context
            else {
                panic!("real kicked roster");
            };
            assert_eq!((*minimum, *maximum, *can_finish), (0, None, true));
            assert_eq!(
                legal_choices
                    .iter()
                    .map(|row| row.arena_id)
                    .collect::<Vec<_>>(),
                vec![first.0, second.0]
            );
            engine::step(&mut state, Action::FinishEffectSelection).unwrap();
            let origin = surface.next_decision(&mut state).unwrap();
            assert!(state.engine.pending_cast.is_none());
            let own = facts(&state, &origin, actor).unwrap();
            assert_eq!(own.finalized.len(), 1);
            assert!(own.finalized[0].kicked && own.finalized[0].targets.is_empty());
            assert_eq!(own.finalized[0].source.zone, Zone::Stack);
            let replay: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(facts(&replay, &origin, actor).unwrap(), own);
        }
    }
}
