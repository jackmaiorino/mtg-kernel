use super::*;
use crate::engine::{self, Action, Decision};
use crate::event::ravenous_amulet_paid_future_tests::{cast_amulet, finish, paid_priority};
use crate::ids::ObjectId;
use crate::mana::ManaColor;
use crate::policy_observation_v6::tests::{advance_to_next_main_for, put};
use crate::ravenous_public_v51::OutcomeV51;
use crate::state::Zone;

fn current(
    state: &GameState,
) -> (
    GameState,
    PolicySurfaceV5,
    crate::policy_surface_v5::PolicyDecisionV5,
    RavenousObservationV51,
) {
    // Take an actual engine decision on a disposable clone. This does not
    // invent a priority window or install any executable journal ownership.
    let mut checkpoint = state.clone();
    let mut surface = PolicySurfaceV5::new_with_engine_priority_v1();
    let origin = surface.next_decision(&mut checkpoint).unwrap();
    let actor = origin.actor(&checkpoint).unwrap();
    let observation = observe_v51(
        &checkpoint,
        &surface,
        (actor, &origin),
        7,
        11,
        origin.substep(),
    )
    .unwrap();
    (checkpoint, surface, origin, observation)
}

pub(crate) fn assert_current(state: &GameState) {
    let (checkpoint, surface, origin, observation) = current(state);
    let actor = origin.actor(&checkpoint).unwrap();
    observation.validate_artifact().unwrap();
    let restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&checkpoint).unwrap()).unwrap();
    let artifact: RavenousObservationV51 =
        serde_json::from_slice(&serde_json::to_vec(&observation).unwrap()).unwrap();
    assert_eq!(artifact, observation);
    validate_current(
        &restored,
        &surface,
        (actor, &origin),
        7,
        11,
        origin.substep(),
        &artifact,
    )
    .unwrap();
    for (seat, cards) in observation.battlefield.iter().enumerate() {
        assert_eq!(cards.len(), checkpoint.players[seat].battlefield.len());
        for card in cards {
            let live = checkpoint.objects.get(ObjectId(card.card.stable.arena_id));
            assert_eq!(
                (card.soul, card.plus_one_counters, card.marked_damage),
                (live.counters.soul, live.counters.plus1_plus1, live.damage)
            );
        }
    }
    let json = serde_json::to_value(&artifact).unwrap();
    let text = serde_json::to_string(&json).unwrap();
    for private in [
        "original_world",
        "output_world",
        "event_history",
        "payment_history_start",
        "tap_history_index",
        "input_history_index",
        "inline_effect",
        "replay_borrow",
    ] {
        assert!(!text.contains(private), "private operation field {private}");
    }
    for activation in json["ravenous"]["activations"].as_array().unwrap() {
        assert_eq!(activation["sacrificed"].as_object().unwrap().len(), 2);
        assert!(activation["sacrificed"]["object"].is_number());
        assert!(activation["sacrificed"]["generation"].is_number());
        assert!(activation["outcome"]["Draw"].get("card").is_none());
    }
    let before = checkpoint.clone();
    // Recommit tampering so these tests exercise current owned-state rebuild,
    // rather than merely catching an outdated commitment.
    for case in 0..6 {
        let mut changed = artifact.clone();
        match case {
            0 => {
                changed
                    .battlefield
                    .iter_mut()
                    .flatten()
                    .next()
                    .unwrap()
                    .soul ^= 1
            }
            1 => {
                changed
                    .battlefield
                    .iter_mut()
                    .flatten()
                    .next()
                    .unwrap()
                    .marked_damage ^= 1
            }
            2 => {
                changed
                    .battlefield
                    .iter_mut()
                    .find(|cards| !cards.is_empty())
                    .unwrap()
                    .pop();
            }
            3 => changed.fake.card_db_hash ^= 1,
            4 => changed.ravenous.activations.last_mut().unwrap().occurrence += 1,
            5 => changed.physical_decision_id += 1,
            _ => unreachable!(),
        }
        changed.projection_sha256 = commitment(&changed).unwrap();
        assert!(
            validate_current(
                &checkpoint,
                &surface,
                (actor, &origin),
                7,
                11,
                origin.substep(),
                &changed
            )
            .is_err(),
            "case {case}"
        );
        assert_eq!(checkpoint, before);
    }
    let mut wrong_actor = origin.clone();
    // Actor/substep checks use the real decision as supplied, without creating
    // another decision or changing any state.
    assert!(observe_v51(
        &checkpoint,
        &surface,
        (actor.opponent(), &wrong_actor),
        7,
        11,
        origin.substep()
    )
    .is_err());
    wrong_actor = origin.clone();
    assert!(observe_v51(&checkpoint, &surface, (actor, &wrong_actor), 7, 11, (0, 2)).is_err());
}

fn pay_draw(state: &mut GameState, actor: PlayerId, amulet: ObjectId) -> ObjectId {
    let chosen = put(state, actor, "Llanowar Elves", Zone::Battlefield);
    let spare = put(state, actor, "Llanowar Elves", Zone::Battlefield);
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
    engine::step(state, Action::ActivateAbility(amulet, 0)).unwrap();
    assert!(
        matches!(engine::advance_until_decision(state), Decision::ChooseCostTargets { ref candidates, .. } if candidates.contains(&chosen) && candidates.contains(&spare))
    );
    engine::step(state, Action::ChooseCostTarget(chosen)).unwrap();
    paid_priority(state, actor);
    chosen
}

#[test]
fn genuine_paid_live_draw_countered_loss_restore_and_legacy_refusal_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, amulet) = cast_amulet(actor);
        put(&mut state, actor, "Island", Zone::Hand);
        put(&mut state, actor.opponent(), "Mountain", Zone::Hand);
        let chosen = pay_draw(&mut state, actor, amulet);
        assert_current(&state);
        let live = current(&state).3;
        assert_eq!(
            live.ravenous.activations.last().unwrap().outcome,
            OutcomeV51::Published
        );
        let id = state.stack.last().unwrap().v4.stack_item_id;
        let mut countered = state.clone();
        assert!(engine::counter_stack_item_by_id(&mut countered, id)
            .unwrap()
            .is_some());
        assert_current(&countered);
        assert_eq!(
            current(&countered)
                .3
                .ravenous
                .activations
                .last()
                .unwrap()
                .outcome,
            OutcomeV51::Countered
        );
        assert_eq!(countered.objects.get(chosen).zone, Zone::Graveyard);
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut restored] {
            finish(branch);
            assert_current(branch);
            let (checkpoint, surface, origin, observation) = current(branch);
            let owner = origin.actor(&checkpoint).unwrap();
            assert!(matches!(
                observation.ravenous.activations.last().unwrap().outcome,
                OutcomeV51::Draw {
                    drew_card: true,
                    ..
                }
            ));
            assert_eq!(branch.objects.get(amulet).counters.soul, 1);
            assert!(crate::fake_observation_v50::observe_v50(
                &checkpoint,
                &surface,
                (owner, &origin),
                7,
                11,
                origin.substep()
            )
            .is_err());
            assert!(
                crate::rl::observe_v2(&checkpoint, surface.harness_surface(), owner, 7).is_err()
            );
        }
        assert_eq!(state, restored);
        advance_to_next_main_for(&mut state, actor);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
        engine::step(&mut state, Action::ActivateAbility(amulet, 1)).unwrap();
        paid_priority(&mut state, actor);
        assert_current(&state);
        let loss_live = current(&state).3;
        assert_eq!(
            loss_live
                .ravenous
                .activations
                .last()
                .unwrap()
                .self_sacrifice_soul_count,
            Some(1)
        );
        let mut restored: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut restored] {
            finish(branch);
            assert_current(branch);
            assert_eq!(
                current(branch)
                    .3
                    .ravenous
                    .activations
                    .last()
                    .unwrap()
                    .outcome,
                OutcomeV51::Loss {
                    opponent: actor.opponent().into(),
                    amount: 1
                }
            );
        }
        assert_eq!(state, restored);
    }
}

#[test]
fn genuine_paid_observation_private_noninterference_and_exact_widths_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, amulet) = cast_amulet(actor);
        put(&mut state, actor, "Island", Zone::Hand);
        let hidden_hand = put(&mut state, actor.opponent(), "Mountain", Zone::Hand);
        pay_draw(&mut state, actor, amulet);
        finish(&mut state);
        let (checkpoint, surface, origin, expected) = current(&state);
        assert_eq!(origin.actor(&checkpoint), Some(actor));
        let mut variant = checkpoint.clone();
        let alternate = crate::card_def::card_id_by_name("Lightning Bolt").unwrap();
        variant.objects.get_mut(hidden_hand).card_def = alternate;
        for seat in 0..2 {
            let hidden = *variant.players[seat].library.last().unwrap();
            variant.objects.get_mut(hidden).card_def = alternate;
        }
        assert_eq!(
            observe_v51(
                &variant,
                &surface,
                (actor, &origin),
                7,
                11,
                origin.substep()
            )
            .unwrap(),
            expected
        );
        // Exact-width fixture operands on an artifact do not claim another
        // printing producer. They test values beyond both predecessor slots.
        variant = checkpoint.clone();
        variant.objects.get_mut(amulet).counters.plus1_plus1 = i32::from(i16::MAX) + 1;
        variant.objects.get_mut(amulet).damage = u32::from(u16::MAX) + 1;
        let wide = observe_v51(
            &variant,
            &surface,
            (actor, &origin),
            7,
            11,
            origin.substep(),
        )
        .unwrap();
        wide.validate_artifact().unwrap();
        let card = wide
            .battlefield
            .iter()
            .flatten()
            .find(|card| card.card.stable.arena_id == amulet.0)
            .unwrap();
        assert_eq!(card.plus_one_counters, i32::from(i16::MAX) + 1);
        assert_eq!(card.marked_damage, u32::from(u16::MAX) + 1);
        assert_eq!(card.soul, 1);
        let mut json = serde_json::to_value(&wide).unwrap();
        let seat = actor.index();
        let index = wide.battlefield[seat]
            .iter()
            .position(|card| card.card.stable.arena_id == amulet.0)
            .unwrap();
        for field in ["soul", "plus_one_counters", "marked_damage"] {
            let mut missing = json.clone();
            missing["battlefield"][seat][index]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(serde_json::from_value::<RavenousObservationV51>(missing).is_err());
        }
        json["ravenous"]["activations"][0]["original_world"] = serde_json::json!({});
        assert!(serde_json::from_value::<RavenousObservationV51>(json).is_err());
    }
}
