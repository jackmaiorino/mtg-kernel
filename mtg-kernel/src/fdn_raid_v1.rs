//! Raid entry replacement counters, applied before entry triggers and SBAs.
use crate::card_def::CARD_DEFS;
use crate::ids::ObjectId;
use crate::state::{GameState, Zone};

pub(crate) fn entry_counter_count_for(name: &str) -> Option<u8> {
    match name {
        "Goblin Boarders" => Some(1),
        _ => None,
    }
}

pub(crate) fn apply_entry_counters(state: &mut GameState, id: ObjectId) {
    let object = state.objects.get(id);
    let definition = &CARD_DEFS[object.card_def as usize];
    if object.zone != Zone::Battlefield || !definition.is_executable() {
        return;
    }
    let Some(count) = entry_counter_count_for(definition.name) else {
        return;
    };
    if state.player_attacked_this_turn_v1(object.controller)
        && crate::continuous_characteristics_v1::printed_abilities_active(state, id)
    {
        state.objects.get_mut(id).counters.plus1_plus1 += i32::from(count);
    }
}

#[cfg(all(test, feature = "limited-fdn-fixtures"))]
mod printed_entry_predicate_tests {
    use super::*;
    use crate::engine::{self, Action};
    use crate::event::CommittedEvent;
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::{ObjectLinkV4, Step};

    #[test]
    fn registered_raid_entry_predicates_refuse_disabled_printed_abilities_and_restore() {
        for player in [PlayerId::P0, PlayerId::P1] {
            for name in ["Skyship Buccaneer", "Goblin Boarders"] {
                let mut state = ready_state();
                state.active_player = player;
                state.priority_player = player;
                state.step = Step::DeclareAttackers;
                let attacker = put(&mut state, player, "Guttersnipe", Zone::Battlefield);
                engine::step(&mut state, Action::DeclareAttackers(vec![attacker])).unwrap();
                state.engine.event_log.clear();
                let source = put(&mut state, player, name, Zone::Battlefield);
                let aura = put(&mut state, player, "Witness Protection", Zone::Battlefield);
                state.objects.get_mut(source).attachments.push(aura);
                let generation = state.objects.get(source).zone_change_count;
                state.objects.get_mut(aura).v4.attached_to = Some(ObjectLinkV4 {
                    object: source,
                    zone_change_count: generation,
                });
                state.objects.get_mut(aura).v4.layer_timestamp = Some(1);
                assert!(
                    !crate::continuous_characteristics_v1::printed_abilities_active(&state, source)
                );
                let mut replay: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                for current in [&mut state, &mut replay] {
                    // This exercises the production entry-characteristic predicate
                    // directly; the preattached Aura is a primitive fixture,
                    // not a claim of a naturally cast pre-entry attachment.
                    apply_entry_counters(current, source);
                    assert_eq!(current.objects.get(source).counters.plus1_plus1, 0);
                    current.engine.event_log.push(CommittedEvent::ZoneChange {
                        object: source,
                        from: Zone::Hand,
                        to: Zone::Battlefield,
                        controller_before: player,
                    });
                    assert!(crate::trigger::collect_and_process(current).is_empty());
                }
                assert_eq!(
                    state.diagnostic_state_hash(),
                    replay.diagnostic_state_hash()
                );
            }
        }
    }
}
