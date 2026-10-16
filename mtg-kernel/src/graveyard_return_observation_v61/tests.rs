use super::*;
use crate::{
    engine::{self, Action},
    graveyard_return_public_v61::tests::{announce, paid, quiet, ready, settle},
    policy_observation_v6::tests::put,
    state::{GameState, Target, Zone},
};

fn current(
    state: &mut GameState,
) -> (
    PolicySurfaceV5,
    PolicyDecisionV5,
    GraveyardReturnObservationV61,
) {
    let mut surface = PolicySurfaceV5::new_with_engine_priority_v1();
    let origin = surface.next_decision(state).unwrap();
    let actor = origin.actor(state).unwrap();
    let before = serde_json::to_vec(state).unwrap();
    let value = observe_v61(state, &surface, (actor, &origin), 0, 0, origin.substep()).unwrap();
    validate_current(
        state,
        &surface,
        (actor, &origin),
        0,
        0,
        origin.substep(),
        &value,
    )
    .unwrap();
    assert_eq!(serde_json::to_vec(state).unwrap(), before);
    let restored: GameState = serde_json::from_slice(&before).unwrap();
    assert_eq!(
        observe_v61(
            &restored,
            &surface,
            (actor, &origin),
            0,
            0,
            origin.substep()
        )
        .unwrap(),
        value
    );
    let roundtrip: GraveyardReturnObservationV61 =
        serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(roundtrip, value);
    assert!(value.complete_ordinary_observation && !value.owning_consumer_admitted);
    (surface, origin, value)
}

#[test]
fn paid_familiar_full_projection_keeps_all_counters_and_frozen_refusals_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let familiar = paid(&mut state, actor, "Nine-Lives Familiar", None);
        let (surface, origin, value) = current(&mut state);
        let row = value.graveyard_return.battlefield_counters[actor.index()]
            .iter()
            .find(|r| r.card.object == familiar.0)
            .unwrap();
        assert_eq!(row.counters.revival, 8);
        assert_eq!(row.counters.plus_one, 0);
        assert!(crate::drake_public_v53::capture(&state).is_err());
        assert!(crate::prepared_observation_v57::validate_state(&state).is_err());
        assert!(crate::next_five_observation_v59::validate_state(&state).is_err());
        assert!(crate::elenda_observation_v52::observe_v52(
            &state,
            &surface,
            (origin.actor(&state).unwrap(), &origin),
            0,
            0,
            origin.substep()
        )
        .is_err());
        assert!(value.common.common.validate_artifact().is_err());
        let mut forged = value.clone();
        forged.graveyard_return.battlefield_counters[actor.index()][0]
            .counters
            .revival = -1;
        forged.projection_sha256 = commitment(&forged).unwrap();
        assert!(validate_artifact(&forged).is_err());
        let mut missing = serde_json::to_value(&value).unwrap();
        missing["graveyard_return"]["battlefield_counters"][actor.index()][0]["counters"]
            .as_object_mut()
            .unwrap()
            .remove("revival");
        assert!(serde_json::from_value::<GraveyardReturnObservationV61>(missing).is_err());
        let native = state.clone();
        let plain = put(&mut state, actor.opponent(), "Forest", Zone::Battlefield);
        // Explicit native counter-storage operand, not a fabricated Forest ability.
        state.objects.get_mut(plain).counters.revival = i32::MAX;
        let (_, _, wide) = current(&mut state);
        assert_eq!(
            wide.graveyard_return.battlefield_counters[actor.opponent().index()]
                .iter()
                .find(|r| r.card.object == plain.0)
                .unwrap()
                .counters
                .revival,
            i32::MAX
        );
        assert_eq!(state.objects.get(familiar), native.objects.get(familiar));
    }
}

#[test]
fn paid_familiar_death_and_next_end_step_keep_actual_held_stack_and_schedule_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let familiar = paid(&mut state, actor, "Nine-Lives Familiar", None);
        let removal = put(&mut state, actor, "Abrade", Zone::Hand);
        announce(&mut state, actor, removal, Some(familiar));
        for _ in 0..40 {
            let decision = engine::advance_until_decision(&mut state);
            if state.objects.get(familiar).zone == Zone::Graveyard {
                break;
            }
            quiet(&mut state, decision);
        }
        let (_, _, value) = current(&mut state);
        assert_eq!(
            value.graveyard_return.nine_lives_deaths[0].departure_revival,
            8
        );
        assert!(!value.graveyard_return.triggers.is_empty());
        settle(&mut state);
        let (_, _, value) = current(&mut state);
        assert_eq!(
            value.graveyard_return.nine_lives_schedules[0].return_base_before_replacements,
            7
        );
        for _ in 0..200 {
            let decision = engine::advance_until_decision(&mut state);
            if state.objects.get(familiar).zone == Zone::Battlefield {
                break;
            }
            quiet(&mut state, decision);
        }
        let (_, _, value) = current(&mut state);
        assert_eq!(state.objects.get(familiar).counters.revival, 7);
        assert!(value.graveyard_return.nine_lives_schedules.is_empty());
    }
}

#[test]
fn paid_alesha_and_familiar_projection_uses_native_target_source_and_complete_roster_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let alesha = paid(&mut state, actor, "Alesha, Who Laughs at Fate", None);
        let familiar = paid(&mut state, actor, "Nine-Lives Familiar", None);
        let (surface, origin, value) = current(&mut state);
        assert_eq!(value.graveyard_return.current.len(), 2);
        assert_eq!(value.graveyard_return.meanings.len(), 2);
        assert_eq!(
            value.common.common.projection.surface.battlefield[actor.index()].len(),
            value.graveyard_return.battlefield_counters[actor.index()].len()
        );
        let before = serde_json::to_vec(&state).unwrap();
        let mut changed = value.clone();
        changed.graveyard_return.current[0].card.generation += 1;
        changed.projection_sha256 = commitment(&changed).unwrap();
        assert!(validate_current(
            &state,
            &surface,
            (origin.actor(&state).unwrap(), &origin),
            0,
            0,
            origin.substep(),
            &changed
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        assert_eq!(state.objects.get(alesha).controller, actor);
        assert_eq!(state.objects.get(familiar).controller, actor);
    }
}

#[test]
fn independent_concealed_worlds_match_full_projection_and_visible_call_artist_refuse_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let build = |secret: &str| {
            let mut state = ready(actor);
            paid(&mut state, actor, "Nine-Lives Familiar", None);
            put(&mut state, actor.opponent(), secret, Zone::Hand);
            put(&mut state, actor.opponent(), secret, Zone::Library);
            let (_, _, value) = current(&mut state);
            (state, value)
        };
        let (one, a) = build("Valkyrie's Call");
        let (two, b) = build("Perforating Artist");
        assert_ne!(one, two);
        assert_eq!(
            serde_json::to_vec(&a).unwrap(),
            serde_json::to_vec(&b).unwrap()
        );
        for name in ["Valkyrie's Call", "Perforating Artist"] {
            let mut state = ready(actor);
            if name == "Valkyrie's Call" {
                for _ in 0..2 {
                    put(&mut state, actor, "Plains", Zone::Battlefield);
                }
            }
            let foreign = paid(&mut state, actor, name, None);
            assert!(require_supported(&state, Some(actor)).is_err());
            if name == "Perforating Artist" {
                paid(&mut state, actor, "Snap", Some(foreign));
                // It is now a later hand incarnation. Its original normal cast
                // was public, and remains outside V61's closed family history.
                assert_eq!(state.objects.get(foreign).zone, Zone::Hand);
                assert!(require_supported(&state, Some(actor.opponent())).is_err());
            }
        }
    }
}

#[test]
fn paid_raid_prompt_published_original_and_departure_lki_keep_exact_sources_both_seats() {
    use crate::graveyard_return_public_v61::{PowerSourceV61, ProgramV61};
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, alesha, target) =
            crate::graveyard_return_public_v61::tests::raid_ready(actor);
        let (_, _, prompt) = current(&mut state);
        assert!(prompt.graveyard_return.triggers.iter().any(|r| matches!(&r.program, ProgramV61::AleshaRaid { power, .. } if power.amount == 3 && power.source == PowerSourceV61::CurrentOriginalIncarnation)));
        let held = state
            .engine
            .pending_triggers
            .iter()
            .find(|p| p.source == alesha)
            .unwrap();
        assert!(pending_source_reference(&state, held).unwrap().is_some());
        let mut wrong = held.clone();
        wrong.controller = actor.opponent();
        assert!(pending_source_reference(&state, &wrong).is_err());
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        let item = state.stack.iter().find(|i| i.source == alesha).unwrap();
        assert_eq!(
            stack_source_reference(&state, item)
                .unwrap()
                .unwrap()
                .controller,
            actor.into()
        );
        let mut wrong = item.clone();
        wrong.targets.clear();
        assert!(stack_source_reference(&state, &wrong).is_err());
        let removal = put(&mut state, actor.opponent(), "Abrade", Zone::Hand);
        announce(&mut state, actor.opponent(), removal, Some(alesha));
        for _ in 0..12 {
            let decision = engine::advance_until_decision(&mut state);
            if state.objects.get(alesha).zone == Zone::Graveyard {
                break;
            }
            quiet(&mut state, decision);
        }
        assert_eq!(state.objects.get(alesha).zone, Zone::Graveyard);
        let (_, _, lki) = current(&mut state);
        assert!(lki.graveyard_return.triggers.iter().any(|r| matches!(&r.program, ProgramV61::AleshaRaid { power, .. } if power.amount == 3 && power.source == PowerSourceV61::ExactDepartureLki) && r.targets.len() == 1 && r.targets[0].legal_now));
        settle(&mut state);
        assert_eq!(state.objects.get(target).zone, Zone::Battlefield);
        current(&mut state);
    }
}
