//! Privileged original-world records for the appended soul-counter primitive.
//! These establish placement replay consistency, not printed-ability ownership.
use crate::{
    doubling_season_v48::ReplacementInputV48,
    event::CommittedEvent,
    ids::PlayerId,
    state::{AbilitySourceContractV4, GameState, Zone},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementInputV1 {
    pub index: u64,
    pub world: Box<GameState>,
    pub source: AbilitySourceContractV4,
    pub actor: PlayerId,
}

impl PlacementInputV1 {
    pub(crate) fn capture(
        state: &GameState,
        source: AbilitySourceContractV4,
        actor: PlayerId,
    ) -> Result<Self, String> {
        let mut world = state.clone();
        world.engine.event_history.clear();
        world.engine.event_log.clear();
        // Match the existing counter journal's canonical restore boundary.
        let bytes = serde_json::to_vec(&world).map_err(|error| error.to_string())?;
        let world = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        Ok(Self {
            index: state.engine.event_history.len() as u64,
            world: Box::new(world),
            source,
            actor,
        })
    }
}

pub(crate) fn has_history(state: &GameState) -> bool {
    state.engine.event_history.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::SoulCounterInputV1 { .. }
                | CommittedEvent::SoulCounterAddedV1 { .. }
                | CommittedEvent::RavenousPaymentV1 { .. }
        )
    })
}

pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    let history = &state.engine.event_history;
    for (index, event) in history.iter().enumerate() {
        match event {
            CommittedEvent::SoulCounterInputV1 { input } => {
                if input.index != index as u64
                    || !input.world.engine.event_history.is_empty()
                    || !input.world.engine.event_log.is_empty()
                    || !matches!(history.get(index + 1),
                        Some(CommittedEvent::SoulCounterAddedV1 { input_history_index, .. })
                        if *input_history_index == input.index)
                {
                    return Err("soul placement lost its original world/completion".into());
                }
            }
            CommittedEvent::SoulCounterAddedV1 {
                input_history_index,
                source,
                player,
                replacement,
                before,
                count,
                after,
            } => {
                let input_at = usize::try_from(*input_history_index)
                    .map_err(|_| "soul input index overflow")?;
                let Some(CommittedEvent::SoulCounterInputV1 { input }) = history.get(input_at)
                else {
                    return Err("soul placement original world absent".into());
                };
                let live = input
                    .world
                    .objects
                    .try_get(source.source)
                    .ok_or("soul original recipient absent")?;
                if input_at.checked_add(1) != Some(index)
                    || input.index != *input_history_index
                    || input.source != *source
                    || input.actor != *player
                    || source.controller != *player
                    || source.zone != Zone::Battlefield
                    || live.zone != Zone::Battlefield
                    || live.owner != source.owner
                    || live.zone_change_count != source.zone_change_count
                    || *before < 0
                    || live.counters.soul != *before
                    || input.world.objects.iter().any(|(_, object)| {
                        usize::from(object.card_def) >= crate::card_def::CARD_DEFS.len()
                    })
                    || *replacement != ReplacementInputV48::capture(&input.world, live.controller)
                {
                    return Err("soul placement actor/recipient input changed".into());
                }
                let original =
                    crate::standard_cards_v1::scale_counters_checked_v48(&input.world, *player, 1)?;
                let expected = replacement.replaced_count(original)?;
                if expected != *count || before.checked_add(expected) != Some(*after) {
                    return Err("soul placement arithmetic changed".into());
                }
            }
            _ => {}
        }
    }
    let mut published = Vec::new();
    for event in &state.engine.event_log {
        match event {
            CommittedEvent::SoulCounterInputV1 { .. }
            | CommittedEvent::RavenousPaymentV1 { .. } => {
                return Err("private soul input replayed as trigger event".into());
            }
            CommittedEvent::SoulCounterAddedV1 {
                input_history_index,
                ..
            } => {
                let at = usize::try_from(*input_history_index)
                    .ok()
                    .and_then(|at| at.checked_add(1));
                if at.and_then(|at| history.get(at)) != Some(event)
                    || published.contains(input_history_index)
                {
                    return Err("issued soul event lost its unique historical placement".into());
                }
                published.push(*input_history_index);
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        effect::ExecCtx,
        event::{add_soul_counter_to_ability_source_v1, propose_and_commit, ProposedEvent},
        policy_observation_v6::tests::{put, ready_state_for},
    };

    fn placed(actor: PlayerId) -> (GameState, crate::ids::ObjectId) {
        let mut state = ready_state_for(actor);
        let source = put(&mut state, actor, "Forest", Zone::Battlefield);
        let original = AbilitySourceContractV4::capture(&state, source);
        crate::standard_cards_v1::gain_control(&mut state, source, actor.opponent());
        let mut ctx = ExecCtx::no_targets(source, actor);
        ctx.ability_source_contract = Some(original);
        add_soul_counter_to_ability_source_v1(&mut state, &ctx).unwrap();
        (state, source)
    }

    #[test]
    fn original_placement_replays_after_departure_reentry_and_restore_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, source) = placed(actor);
            validate(&state).unwrap();
            propose_and_commit(&mut state, ProposedEvent::zone_change(source, Zone::Hand));
            propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Battlefield),
            );
            assert_eq!(state.objects.get(source).counters.soul, 0);
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&state, &restored] {
                validate(branch).unwrap();
                crate::doubling_season_v48::validate_token_journals(branch).unwrap();
            }
            assert_eq!(state, restored);
        }
    }

    #[test]
    fn changed_arithmetic_world_links_and_replayed_inputs_refuse_readonly_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, source) = placed(actor);
            let input_at = state.engine.event_history.len() - 2;
            let event_at = input_at + 1;
            for mutation in 0..13 {
                let mut bad = state.clone();
                match mutation {
                    0..=4 => {
                        let CommittedEvent::SoulCounterAddedV1 {
                            input_history_index,
                            player,
                            before,
                            count,
                            after,
                            ..
                        } = &mut bad.engine.event_history[event_at]
                        else {
                            unreachable!()
                        };
                        match mutation {
                            0 => *input_history_index = event_at as u64,
                            1 => *player = actor.opponent(),
                            2 => *before = -1,
                            3 => *count += 1,
                            _ => *after += 1,
                        }
                    }
                    5..=9 => {
                        let CommittedEvent::SoulCounterInputV1 { input } =
                            &mut bad.engine.event_history[input_at]
                        else {
                            unreachable!()
                        };
                        match mutation {
                            5 => input.index += 1,
                            6 => input.actor = actor.opponent(),
                            7 => input.world.objects.get_mut(source).counters.soul = 4,
                            8 => input.world.objects.get_mut(source).zone = Zone::Hand,
                            _ => input
                                .world
                                .engine
                                .event_history
                                .push(state.engine.event_history[event_at].clone()),
                        }
                    }
                    10 => {
                        bad.engine.event_history.remove(input_at);
                    }
                    11 => bad
                        .engine
                        .event_log
                        .push(state.engine.event_history[input_at].clone()),
                    _ => bad
                        .engine
                        .event_log
                        .push(state.engine.event_history[event_at].clone()),
                }
                let before = bad.clone();
                assert!(validate(&bad).is_err(), "mutation {mutation}");
                assert!(crate::doubling_season_v48::validate_token_journals(&bad).is_err());
                assert_eq!(bad, before);
            }
        }
    }
}
