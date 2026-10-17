//! Complete three-printing interactions use the existing Call scope. Initial
//! lands and hands are native setup. Every cast, attack, counter, response,
//! trigger and numeric action below uses the genuine engine and sealed owner.
use super::*;
use crate::{
    engine::{self, Action, Decision},
    graveyard_return_public_v61::{self as public, tests as native},
    state::Target,
};

fn equal(pair: &Pair) {
    assert_eq!(pair.model.fixture_state(), pair.native.call_fixture_state());
    assert_eq!(
        pair.model.current_response(),
        pair.native.current_response()
    );
    assert_eq!(
        pair.model.fixture_counts(),
        (
            pair.native.policy_step_count(),
            pair.native.physical_decision_count()
        )
    );
}
fn checkpoint(pair: &mut Pair) {
    let decision = pair.decision();
    let world = pair.model.fixture_state().clone();
    let counts = pair.model.fixture_counts();
    let bytes = serde_json::to_vec(&world).unwrap();
    let restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
    crate::call_observation_v62::validate_state(&restored).unwrap();
    for viewer in [PlayerId::P0, PlayerId::P1] {
        assert_eq!(
            crate::valkyries_call_public_v62::capture(&restored, viewer).unwrap(),
            crate::valkyries_call_public_v62::capture(&world, viewer).unwrap()
        );
        assert_eq!(
            public::capture_complete_call_v62(&restored, viewer).unwrap(),
            public::capture_complete_call_v62(&world, viewer).unwrap()
        );
    }
    let projection = decision.backend.candidates_v62();
    let decoded: crate::call_candidates_v62::CallCandidatesV62 =
        serde_json::from_slice(&serde_json::to_vec(projection).unwrap()).unwrap();
    crate::call_candidates_v62::validate_artifact(&decoded).unwrap();
    assert_eq!(&decoded, projection);
    let snapshot = pair.model.snapshot();
    let reference = pair.native.snapshot_v1();
    pair.model.restore(&snapshot).unwrap();
    pair.native.call_restore_v62(&reference).unwrap();
    equal(pair);
    assert_eq!(pair.model.fixture_state(), &world);
    assert_eq!(pair.model.fixture_counts(), counts);
    assert_eq!(pair.decision().packet(), decision.packet());
    assert_eq!(pair.model.apply(&decision, 0), Err(Error::StaleBinding));
    assert_eq!(pair.model.fixture_state(), &world);
    assert_eq!(pair.model.fixture_counts(), counts);
}
fn paid_cast(
    pair: &mut Pair,
    actor: PlayerId,
    spell: ObjectId,
    target: Option<ObjectId>,
    cost: usize,
) {
    let taps = |state: &GameState| {
        state.players[actor.index()]
            .battlefield
            .iter()
            .filter(|&&id| {
                state.objects.get(id).tapped
                    && crate::card_def::CARD_DEFS[usize::from(state.objects.get(id).card_def)]
                        .is_land
            })
            .count()
    };
    let before = taps(pair.model.fixture_state());
    pair.cast(spell, target);
    let state = pair.model.fixture_state();
    assert_eq!(taps(state) - before, cost);
    let row = state.stack.last().unwrap();
    assert_eq!(row.source, spell);
    assert_eq!(row.controller, actor);
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
    equal(pair);
}

#[test]
fn genuine_paid_three_printing_death_call_return_and_stale_familiar_schedule_numeric_replay_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, "Forest");
        let call = hand(&mut state, actor, "Valkyrie's Call");
        let familiar = hand(&mut state, actor, "Nine-Lives Familiar");
        let alesha = hand(&mut state, actor, "Alesha, Who Laughs at Fate");
        let bolt = hand(&mut state, actor.opponent(), "Lightning Bolt");
        let mut pair = Pair::new(state);
        for (spell, cost) in [(call, 5), (familiar, 3), (alesha, 3)] {
            paid_cast(&mut pair, actor, spell, None, cost);
            pair.drain();
        }
        assert_eq!(
            pair.model
                .fixture_state()
                .objects
                .get(familiar)
                .counters
                .revival,
            8
        );
        let original_generation = pair
            .model
            .fixture_state()
            .objects
            .get(familiar)
            .zone_change_count;
        paid_cast(&mut pair, actor.opponent(), bolt, Some(familiar), 1);
        let mut seen_death_program = false;
        let mut seen_call_return = false;
        for _ in 0..100 {
            let decision = pair.decision();
            let observation = decision.backend.observation();
            seen_death_program |= observation.graveyard_return.triggers.iter().any(|row| {
                matches!(
                    row.program,
                    public::ProgramV61::NineLivesDeath {
                        departure_revival: 8,
                        return_base_before_replacements: 7
                    }
                )
            });
            seen_call_return |= !observation.call.returns.is_empty();
            let state = pair.model.fixture_state();
            if state.stack.is_empty()
                && state.engine.pending_effect.is_none()
                && state.engine.pending_triggers.is_empty()
                && state.objects.get(familiar).zone == Zone::Battlefield
            {
                break;
            }
            pair.quiet();
        }
        assert!(seen_death_program && seen_call_return);
        let decision = pair.decision();
        assert_eq!(
            decision
                .backend
                .observation()
                .graveyard_return
                .nine_lives_schedules
                .len(),
            1
        );
        let state = pair.model.fixture_state();
        let returned = state.objects.get(familiar);
        assert_eq!(returned.zone, Zone::Battlefield);
        assert_eq!(returned.zone_change_count, original_generation + 2);
        assert_eq!(returned.counters.plus1_plus1, 1);
        assert_eq!(returned.counters.revival, 0); // Call's return is not a cast.
        assert!(crate::graveyard_return_observation_v61::validate_state(state).is_err());
        let saved = pair.model.snapshot();
        let reference = pair.native.snapshot_v1();
        checkpoint(&mut pair);
        let mut expected = None;
        for replay in [false, true] {
            if replay {
                pair.model.restore(&saved).unwrap();
                pair.native.call_restore_v62(&reference).unwrap();
                equal(&pair);
            }
            let mut seen_delayed = false;
            for _ in 0..200 {
                let decision = pair.decision();
                let facts = &decision.backend.observation().graveyard_return;
                seen_delayed |= facts.triggers.iter().any(|row| {
                    matches!(
                        row.program,
                        public::ProgramV61::NineLivesDelayedReturn { .. }
                    )
                });
                if seen_delayed
                    && facts.nine_lives_schedules.is_empty()
                    && pair.model.fixture_state().stack.is_empty()
                {
                    break;
                }
                pair.quiet();
            }
            assert!(seen_delayed);
            let state = pair.model.fixture_state();
            let live = state.objects.get(familiar);
            assert_eq!(live.zone, Zone::Battlefield);
            assert_eq!(live.zone_change_count, original_generation + 2);
            assert_eq!(live.counters.plus1_plus1, 1);
            assert_eq!(live.counters.revival, 0);
            assert!(pair
                .decision()
                .backend
                .observation()
                .graveyard_return
                .nine_lives_schedules
                .is_empty());
            let actual = (
                serde_json::to_vec(state).unwrap(),
                pair.model.fixture_counts(),
            );
            if let Some(expected) = &expected {
                assert_eq!(&actual, expected);
            } else {
                expected = Some(actual);
            }
            equal(&pair);
        }
    }
}

fn paid_three_printing_raid(actor: PlayerId) -> (GameState, ObjectId, ObjectId, ObjectId) {
    let mut state = ready(actor, "Forest");
    let call = native::paid(&mut state, actor, "Valkyrie's Call", None);
    let alesha = native::paid(&mut state, actor, "Alesha, Who Laughs at Fate", None);
    let familiar = hand(&mut state, actor, "Nine-Lives Familiar");
    let counter = hand(&mut state, actor.opponent(), "Counterspell");
    let response = hand(&mut state, actor.opponent(), "Abrade");
    native::announce(&mut state, actor, familiar, None);
    native::announce(&mut state, actor.opponent(), counter, Some(familiar));
    native::settle(&mut state);
    assert_eq!(state.objects.get(familiar).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(familiar).counters.revival, 0);
    assert!(crate::valkyries_call_public_v62::capture(&state, actor)
        .unwrap()
        .deaths
        .is_empty());
    let turn = state.turn;
    let mut attacked = false;
    for _ in 0..400 {
        let decision = engine::advance_until_decision(&mut state);
        if matches!(decision, Decision::DeclareAttackers { player, .. }
            if player == actor && state.turn > turn)
        {
            engine::step(&mut state, Action::DeclareAttackers(vec![alesha])).unwrap();
            attacked = true;
            break;
        }
        native::quiet(&mut state, decision);
    }
    assert!(attacked);
    native::settle(&mut state);
    assert_eq!(engine::effective_power(&state, alesha), 3);
    for _ in 0..240 {
        let decision = engine::advance_until_decision(&mut state);
        if matches!(&decision, Decision::ChooseTargets { player, legal_targets, .. }
            if *player == actor && legal_targets.contains(&Target::Object(familiar)))
        {
            assert_eq!(state.objects.get(call).zone, Zone::Battlefield);
            return (state, alesha, familiar, response);
        }
        native::quiet(&mut state, decision);
    }
    panic!("genuine three-printing Raid target prompt absent");
}

#[test]
fn genuine_paid_raid_call_reentered_source_departure_lki_costless_choices_full_numeric_binding_and_restore_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for accept in [false, true] {
            let (state, alesha, familiar, response) = paid_three_printing_raid(actor);
            let original_generation = state.objects.get(alesha).zone_change_count;
            let mut pair = Pair::new(state);
            let d = pair.decision();
            assert!(d.backend.candidates_v62().candidates.iter().any(|row| {
                row.selected_graveyard_target
                    .as_ref()
                    .is_some_and(|target| target.original.object == familiar.0)
            }));
            checkpoint(&mut pair);
            pair.act(|a| {
                matches!(a, A::ChooseTarget { target: TargetRefV1::Object { object }, .. }
                if object.arena_id == familiar.0)
            });
            paid_cast(&mut pair, actor.opponent(), response, Some(alesha), 2);
            for _ in 0..100 {
                if pair
                    .decision()
                    .backend
                    .actions()
                    .iter()
                    .any(|a| matches!(a, A::ChooseEffectBoolean { .. }))
                {
                    break;
                }
                pair.quiet();
            }
            let decision = pair.decision();
            let facts = &decision.backend.observation().graveyard_return;
            assert!(facts.triggers.iter().any(|row| {
                row.source.original_source.object == alesha.0
                    && row.source.original_source.generation == original_generation
                    && matches!(
                        &row.program,
                        public::ProgramV61::AleshaRaid {
                            power: public::PowerV61 {
                                amount: 3,
                                source: public::PowerSourceV61::ExactDepartureLki
                            },
                            optional_choice: public::AleshaChoiceV61::AwaitingCostlessReturnChoice,
                            ..
                        }
                    )
            }));
            assert_eq!(
                pair.model.fixture_state().objects.get(alesha).zone,
                Zone::Battlefield
            );
            assert_eq!(
                pair.model
                    .fixture_state()
                    .objects
                    .get(alesha)
                    .zone_change_count,
                original_generation + 2
            );
            assert_eq!(decision.backend.actions().len(), 2);
            let before = pair.model.fixture_state().clone();
            let counts = pair.model.fixture_counts();
            // Change only the complete numeric packet, never the sealed backend.
            let mut changed = decision.clone();
            changed.packet.actions.swap(0, 1);
            assert_eq!(pair.model.apply(&changed, 0), Err(Error::StaleBinding));
            let mut changed = decision.clone();
            let mut state = serde_json::to_value(decision.backend.candidates_v62()).unwrap();
            state["observation"]["graveyard_return"]["alesha_departures"][0]["power"] = 2.into();
            let actions = decision
                .backend
                .candidates_v62()
                .candidates
                .iter()
                .map(|row| serde_json::to_value(row).unwrap())
                .collect();
            changed.packet = normalize::encode(Source::CallV62, state, actions).unwrap();
            assert_eq!(pair.model.apply(&changed, 0), Err(Error::StaleBinding));
            assert_eq!(pair.model.fixture_state(), &before);
            assert_eq!(pair.model.fixture_counts(), counts);
            checkpoint(&mut pair);
            let saved = pair.model.snapshot();
            let reference = pair.native.snapshot_v1();
            let mut result = None;
            for replay in [false, true] {
                if replay {
                    pair.model.restore(&saved).unwrap();
                    pair.native.call_restore_v62(&reference).unwrap();
                }
                pair.act(|a| matches!(a, A::ChooseEffectBoolean { value, .. } if *value == accept));
                pair.drain();
                let state = pair.model.fixture_state();
                assert_eq!(
                    state.objects.get(familiar).zone,
                    if accept {
                        Zone::Battlefield
                    } else {
                        Zone::Graveyard
                    }
                );
                assert_eq!(state.objects.get(familiar).counters.revival, 0);
                let actual = (
                    serde_json::to_vec(state).unwrap(),
                    pair.model.fixture_counts(),
                );
                if let Some(result) = &result {
                    assert_eq!(&actual, result);
                } else {
                    result = Some(actual);
                }
                equal(&pair);
            }
        }
    }
}

#[test]
fn independent_prepaid_three_printing_worlds_complete_counter_operands_and_foreign_numeric_authority_both_seats(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut pairs = Vec::new();
        for (library, concealed) in [
            ("Forest", "Alesha, Who Laughs at Fate"),
            ("Counterspell", "Nine-Lives Familiar"),
        ] {
            // Independent initial libraries/hidden hand differ before any real cast.
            let mut state = ready(actor, library);
            hand(&mut state, actor.opponent(), concealed);
            let cards = [
                "Valkyrie's Call",
                "Alesha, Who Laughs at Fate",
                "Nine-Lives Familiar",
            ]
            .map(|name| hand(&mut state, actor, name));
            let mut pair = Pair::new(state);
            for (spell, cost) in cards.into_iter().zip([5, 3, 3]) {
                paid_cast(&mut pair, actor, spell, None, cost);
                pair.drain();
            }
            assert_eq!(
                pair.decision().backend.expected().acting_player,
                actor.into()
            );
            pairs.push(pair);
        }
        let [first, second]: [Pair; 2] = pairs.try_into().ok().unwrap();
        let (mut first, second) = (first, second);
        assert_ne!(
            first.model.fixture_state().state_hash(),
            second.model.fixture_state().state_hash()
        );
        let a = first.decision();
        let b = second.decision();
        assert_eq!(a.packet(), b.packet());
        assert_eq!(a.backend.observation().graveyard_return.meanings.len(), 2);
        assert_eq!(a.backend.observation().call.meaning.card_id, 459);
        for seat in [PlayerId::P0, PlayerId::P1] {
            let rows = &a.backend.observation().call.battlefield[seat.index()];
            assert_eq!(
                rows.len(),
                first.model.fixture_state().players[seat.index()]
                    .battlefield
                    .len()
            );
            for row in rows {
                let fields = serde_json::to_value(&row.counters).unwrap();
                assert_eq!(fields.as_object().unwrap().len(), 17);
                assert_eq!(
                    serde_json::from_value::<crate::valkyries_call_public_v62::CountersV62>(fields)
                        .unwrap(),
                    row.counters
                );
            }
        }
        let before = first.model.fixture_state().clone();
        let counts = first.model.fixture_counts();
        assert_eq!(first.model.apply(&b, 0), Err(Error::StaleBinding));
        assert_eq!(
            first.model.restore(&second.model.snapshot()),
            Err(Error::ForeignSnapshot)
        );
        assert_eq!(first.model.apply(&a, u32::MAX), Err(Error::ActionRange));
        assert_eq!(first.model.fixture_state(), &before);
        assert_eq!(first.model.fixture_counts(), counts);
        let mut erased = a.clone();
        erased.packet.actions.clear();
        assert!(first.model.apply(&erased, 0).is_err());
        let mut source = a.clone();
        source.packet.source = Source::GraveyardReturnV61;
        assert_eq!(first.model.apply(&source, 0), Err(Error::StaleBinding));
        assert_eq!(first.model.fixture_state(), &before);
        assert_eq!(first.model.fixture_counts(), counts);
        checkpoint(&mut first);
        for name in [
            "Alesha, Who Laughs at Fate",
            "Nine-Lives Familiar",
            "Valkyrie's Call",
        ] {
            let id = crate::card_def::card_id_by_name(name).unwrap();
            assert_eq!(
                crate::card_def::CARD_DEFS[usize::from(id)].capability,
                crate::card_def::CardCapability::Partial
            );
        }
    }
}
