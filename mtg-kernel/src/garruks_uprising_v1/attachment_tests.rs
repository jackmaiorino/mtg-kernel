//! Genuine entry/ETB/linked-return engine boundaries; no fabricated journals.
use super::future_tests::{cast, next, ready, restore, settle};
use super::*;
use crate::engine::{self, Action, Decision};
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::policy_observation_v6::tests::put;
use crate::state::Target;

fn exile(state: &mut GameState, controller: PlayerId, target: ObjectId) -> ObjectId {
    // Physical registered-permanent entry substrate; actual printed ETB target
    // placement and resolution create all source/target/link authority.
    let source = put(state, controller, "Banishing Light", Zone::Hand);
    event::propose_and_commit(state, ProposedEvent::zone_change(source, Zone::Battlefield));
    assert!(
        matches!(next(state),Decision::ChooseTargets {player,legal_targets,..} if player==controller && legal_targets.contains(&Target::Object(target)))
    );
    engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    settle(state);
    assert_eq!(state.objects.get(target).zone, Zone::Exile);
    source
}
pub(crate) fn pending_returns(
    actor: PlayerId,
    suppress: bool,
    reverse: bool,
) -> (GameState, ObjectId, ObjectId, ObjectId, ObjectId, ObjectId) {
    let mut state = ready(actor);
    let eagles = [0, 1].map(|_| put(&mut state, actor, "Empyrean Eagle", Zone::Battlefield));
    let uprising = cast(&mut state, actor, "Garruk's Uprising", 1, 2);
    let creature = put(&mut state, actor, "Vampire Nighthawk", Zone::Hand);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(creature, Zone::Battlefield),
    );
    settle(&mut state);
    assert_eq!(engine::effective_power(&state, creature), 4);
    let creature_source = exile(&mut state, actor.opponent(), creature);
    let uprising_source = exile(&mut state, actor.opponent(), uprising);
    let spare = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
    let own_aura = put(
        &mut state,
        actor,
        if suppress {
            "Witness Protection"
        } else {
            "Twinblade Blessing"
        },
        Zone::Battlefield,
    );
    state.attach_object_exact(own_aura, 0, spare, 0).unwrap();
    let own_aura_source = exile(&mut state, actor.opponent(), own_aura);
    let other_aura = put(
        &mut state,
        actor.opponent(),
        "Twinblade Blessing",
        Zone::Battlefield,
    );
    state.attach_object_exact(other_aura, 0, spare, 0).unwrap();
    let other_aura_source = exile(&mut state, actor, other_aura);
    let mut events = [
        creature_source,
        uprising_source,
        own_aura_source,
        other_aura_source,
    ]
    .map(|source| ProposedEvent::zone_change(source, Zone::Hand))
    .to_vec();
    if reverse {
        events.reverse();
    }
    event::propose_and_commit_batch(&mut state, events);
    assert!(
        matches!(next(&mut state),Decision::ChooseEffectTargets {player,legal_targets,..} if player==actor && legal_targets.contains(&Target::Object(eagles[0])) && !legal_targets.contains(&Target::Object(creature)))
    );
    for object in [creature, uprising, own_aura, other_aura] {
        assert_eq!(state.objects.get(object).zone, Zone::Exile);
    }
    engine::step(
        &mut state,
        Action::ChooseEffectTarget(Target::Object(eagles[0])),
    )
    .unwrap();
    assert!(
        matches!(next(&mut state),Decision::ChooseEffectTargets {player,..} if player==actor.opponent())
    );
    for object in [creature, uprising, own_aura, other_aura] {
        assert_eq!(state.objects.get(object).zone, Zone::Exile);
    }
    (state, uprising, creature, own_aura, other_aura, eagles[0])
}

#[test]
fn actual_apnap_return_batch_applies_all_attachments_before_power_and_ferocious_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for suppress in [false, true] {
            for reverse in [false, true] {
                let (mut state, uprising, creature, own_aura, other_aura, host) =
                    pending_returns(actor, suppress, reverse);
                let before = state.players[actor.index()].hand.len();
                let start = state.engine.event_history.len();
                let mut replay = restore(&state);
                for branch in [&mut state, &mut replay] {
                    engine::step(branch, Action::ChooseEffectTarget(Target::Object(host))).unwrap();
                    for object in [uprising, creature, own_aura, other_aura] {
                        assert_eq!(branch.objects.get(object).zone, Zone::Battlefield);
                    }
                    for aura in [own_aura, other_aura] {
                        assert_eq!(
                            branch.objects.get(aura).v4.attached_to.unwrap().object,
                            host
                        );
                    }
                    let expected = if suppress { 3 } else { 4 };
                    assert_eq!(engine::effective_power(branch, creature), expected);
                    let observation = branch.engine.event_history[start..]
                        .iter()
                        .find_map(|event| match event {
                            CommittedEvent::GarrukEntryObservationV1 {
                                entry,
                                effective_power,
                                ..
                            } if entry.object == creature => Some((entry, *effective_power)),
                            _ => None,
                        })
                        .unwrap();
                    assert_eq!(observation.1, Some(expected));
                    for aura in [own_aura, other_aura] {
                        let snapshot = branch.engine.event_history[start..]
                            .iter()
                            .find_map(|event| match event {
                                CommittedEvent::GarrukEntryObservationV1 { entry, .. }
                                    if entry.object == aura =>
                                {
                                    Some(&entry.snapshot)
                                }
                                _ => None,
                            })
                            .unwrap();
                        assert_eq!(snapshot.v4.attached_to.unwrap().object, host);
                    }
                    settle(branch);
                    assert_eq!(branch.players[actor.index()].hand.len(),before+if suppress {0} else {2},"own ETB intervening condition and creature event-time condition both see the final entry board");
                    validate_state(branch).unwrap();
                    crate::banishing_light_v1::validate_state(branch).unwrap();
                }
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&replay).unwrap()
                );
            }
        }
    }
}
