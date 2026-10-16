use super::*;
use crate::{
    card_def::{self, CardCapability},
    engine::{self, Action, Decision},
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    state::{GameState, Target, Zone},
};

fn ready(actor: PlayerId) -> GameState {
    let mut state = crate::extra_turn_v1::tests::ready(actor);
    crate::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
    for player in [PlayerId::P0, PlayerId::P1] {
        for _ in 0..6 {
            put(&mut state, player, "Swamp", Zone::Battlefield);
            put(&mut state, player, "Mountain", Zone::Battlefield);
            put(&mut state, player, "Island", Zone::Battlefield);
            put(&mut state, player, "Forest", Zone::Battlefield);
        }
    }
    state
}

fn pass(state: &mut GameState, decision: Decision) {
    let action = match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        other => panic!("unexpected choice while settling Nine-Lives: {other:?}"),
    };
    engine::step(state, action).unwrap();
    assert!(state.engine.halted.is_none());
}

fn settle(state: &mut GameState) {
    for _ in 0..100 {
        let decision = engine::advance_until_decision(state);
        if state.stack.is_empty()
            && state.engine.pending_effect.is_none()
            && state.engine.pending_cast.is_none()
            && state.engine.pending_triggers.is_empty()
        {
            assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
            return;
        }
        pass(state, decision);
    }
    panic!("Nine-Lives fixture did not settle");
}

fn paid_cast(state: &mut GameState, actor: PlayerId, spell: ObjectId, target: Option<ObjectId>) {
    let decision = (0..12)
        .find_map(|_| {
            let decision = engine::advance_until_decision(state);
            if matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor) {
                Some(decision)
            } else {
                pass(state, decision);
                None
            }
        })
        .expect("the caster receives priority");
    assert!(
        matches!(decision, Decision::CastSpellOrPass { castable_spells, .. }
        if castable_spells.contains(&spell))
    );
    engine::step(state, Action::CastSpell(spell)).unwrap();
    if let Some(target) = target {
        let choice = engine::advance_until_decision(state);
        assert!(
            matches!(choice, Decision::ChooseTargets { legal_targets, .. }
            if legal_targets.contains(&Target::Object(target)))
        );
        engine::step(state, Action::ChooseTarget(Target::Object(target))).unwrap();
    }
    settle(state);
}

fn advance_to_nine_lives_return(state: &mut GameState, familiar: ObjectId) {
    let original_generation = state.objects.get(familiar).zone_change_count;
    for _ in 0..600 {
        if state.objects.get(familiar).zone == Zone::Battlefield
            && state.objects.get(familiar).zone_change_count == original_generation + 1
        {
            return;
        }
        let decision = engine::advance_until_decision(state);
        pass(state, decision);
    }
    panic!("Nine-Lives delayed return did not resolve at a future end step");
}

#[test]
fn paid_cast_death_and_delayed_season_return_restore_both_seats() {
    let id = card_def::card_id_by_name("Nine-Lives Familiar").unwrap();
    assert_eq!(id, 458);
    let definition = &card_def::CARD_DEFS[usize::from(id)];
    assert_eq!(definition.capability, CardCapability::Partial);
    assert_eq!((definition.power, definition.toughness), (Some(1), Some(1)));
    assert!(definition.subtypes.contains(&crate::card_def::Subtype::Cat));

    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let season = put(&mut state, actor, "Doubling Season", Zone::Hand);
        paid_cast(&mut state, actor, season, None);
        let familiar = put(&mut state, actor, "Nine-Lives Familiar", Zone::Hand);
        paid_cast(&mut state, actor, familiar, None);
        assert_eq!(state.objects.get(familiar).counters.revival, 16);
        let first_generation = state.objects.get(familiar).zone_change_count;

        let abrade = put(&mut state, actor, "Abrade", Zone::Hand);
        paid_cast(&mut state, actor, abrade, Some(familiar));
        assert_eq!(state.objects.get(familiar).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(familiar).owner, actor);
        assert_eq!(state.objects.get(familiar).counters.revival, 0);
        let lki = state.counter_lki_for(familiar, first_generation).unwrap();
        assert_eq!(lki.revival, 16);
        let delayed = state.nine_lives_delayed_v1.as_ref().unwrap();
        assert_eq!(delayed.len(), 1);
        assert_eq!(delayed[0].source.controller, actor);
        assert_eq!(delayed[0].graveyard.zone_change_count, first_generation + 1);
        assert_eq!(delayed[0].revival_counters, 15);

        let snapshot = serde_json::to_vec(&state).unwrap();
        let mut restored: GameState = serde_json::from_slice(&snapshot).unwrap();
        assert_eq!(restored.state_hash(), state.state_hash());
        advance_to_nine_lives_return(&mut state, familiar);
        advance_to_nine_lives_return(&mut restored, familiar);
        assert_eq!(state.state_hash(), restored.state_hash());
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        assert_eq!(state.objects.get(familiar).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(familiar).owner, actor);
        assert_eq!(state.objects.get(familiar).controller, actor);
        assert_eq!(
            state.objects.get(familiar).zone_change_count,
            first_generation + 2
        );
        assert_eq!(state.objects.get(familiar).counters.revival, 30);
        assert!(state.nine_lives_delayed_v1.is_none());
    }
}

#[test]
fn legacy_observation_refuses_paid_native_revival_counters_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let familiar = put(&mut state, actor, "Nine-Lives Familiar", Zone::Hand);
        paid_cast(&mut state, actor, familiar, None);
        assert_eq!(state.objects.get(familiar).counters.revival, 8);
        let error = crate::rl::observe_v1(&state, actor, 0).unwrap_err();
        assert!(error.0.contains("revival counters"), "{error}");
    }
}

#[test]
fn genuine_reanimation_has_no_cast_counters_and_paid_controlled_death_returns_to_controller() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let reanimated = put(&mut state, actor, "Nine-Lives Familiar", Zone::Graveyard);
        let zombify = put(&mut state, actor, "Zombify", Zone::Hand);
        paid_cast(&mut state, actor, zombify, Some(reanimated));
        assert_eq!(state.objects.get(reanimated).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(reanimated).counters.revival, 0);
        let noncast_abrade = put(&mut state, actor, "Abrade", Zone::Hand);
        paid_cast(&mut state, actor, noncast_abrade, Some(reanimated));
        assert_eq!(state.objects.get(reanimated).zone, Zone::Graveyard);
        assert!(state.nine_lives_delayed_v1.is_none());

        let familiar = put(&mut state, actor, "Nine-Lives Familiar", Zone::Hand);
        paid_cast(&mut state, actor, familiar, None);
        assert_eq!(state.objects.get(familiar).counters.revival, 8);
        let battlefield_generation = state.objects.get(familiar).zone_change_count;
        let opponent = actor.opponent();
        crate::policy_observation_v6::tests::advance_to_next_main_for(&mut state, opponent);
        let employment = put(&mut state, opponent, "Involuntary Employment", Zone::Hand);
        paid_cast(&mut state, opponent, employment, Some(familiar));
        assert_eq!(state.objects.get(familiar).controller, opponent);
        let abrade = put(&mut state, actor, "Abrade", Zone::Hand);
        paid_cast(&mut state, actor, abrade, Some(familiar));
        assert_eq!(state.objects.get(familiar).owner, actor);
        assert_eq!(state.objects.get(familiar).zone, Zone::Graveyard);
        assert_eq!(
            state.nine_lives_delayed_v1.as_ref().unwrap()[0]
                .source
                .controller,
            opponent
        );
        assert_eq!(
            state.nine_lives_delayed_v1.as_ref().unwrap()[0]
                .graveyard
                .object,
            familiar
        );
        advance_to_nine_lives_return(&mut state, familiar);
        assert_eq!(state.objects.get(familiar).owner, actor);
        assert_eq!(state.objects.get(familiar).controller, opponent);
        assert_eq!(state.objects.get(familiar).counters.revival, 7);
    }
}

#[test]
fn delayed_return_does_not_follow_card_from_its_captured_graveyard_incarnation() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let familiar = put(&mut state, actor, "Nine-Lives Familiar", Zone::Hand);
        paid_cast(&mut state, actor, familiar, None);
        let first_generation = state.objects.get(familiar).zone_change_count;
        let abrade = put(&mut state, actor, "Abrade", Zone::Hand);
        paid_cast(&mut state, actor, abrade, Some(familiar));
        assert_eq!(
            state.nine_lives_delayed_v1.as_ref().unwrap()[0]
                .graveyard
                .zone_change_count,
            first_generation + 1
        );

        // A real paid Zombify creates a new battlefield incarnation before
        // the delayed trigger's next end step. The old schedule remains bound
        // to the earlier graveyard incarnation and must resolve to no effect.
        let zombify = put(&mut state, actor, "Zombify", Zone::Hand);
        paid_cast(&mut state, actor, zombify, Some(familiar));
        let reanimated_generation = state.objects.get(familiar).zone_change_count;
        assert_eq!(state.objects.get(familiar).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(familiar).counters.revival, 0);
        assert_eq!(reanimated_generation, first_generation + 2);
        for _ in 0..600 {
            if state.nine_lives_delayed_v1.is_none()
                && state.stack.is_empty()
                && state.engine.pending_effect.is_none()
                && state.engine.pending_triggers.is_empty()
                && state.objects.get(familiar).zone_change_count == reanimated_generation
            {
                break;
            }
            let decision = engine::advance_until_decision(&mut state);
            pass(&mut state, decision);
        }
        assert_eq!(state.objects.get(familiar).zone, Zone::Battlefield);
        assert_eq!(
            state.objects.get(familiar).zone_change_count,
            reanimated_generation
        );
        assert_eq!(state.objects.get(familiar).counters.revival, 0);
        assert!(state.nine_lives_delayed_v1.is_none());
    }
}

#[test]
fn ability_suppression_prevents_death_trigger_despite_departure_counters_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let familiar = put(&mut state, actor, "Nine-Lives Familiar", Zone::Hand);
        paid_cast(&mut state, actor, familiar, None);
        assert_eq!(state.objects.get(familiar).counters.revival, 8);
        let battlefield_generation = state.objects.get(familiar).zone_change_count;
        let suppression = put(&mut state, actor, "Witness Protection", Zone::Hand);
        paid_cast(&mut state, actor, suppression, Some(familiar));
        assert!(!crate::continuous_characteristics_v1::printed_abilities_active(&state, familiar));
        let abrade = put(&mut state, actor, "Abrade", Zone::Hand);
        paid_cast(&mut state, actor, abrade, Some(familiar));
        assert_eq!(state.objects.get(familiar).zone, Zone::Graveyard);
        assert!(state
            .counter_lki_for(familiar, battlefield_generation)
            .is_some_and(|c| c.revival == 8));
        assert!(state.nine_lives_delayed_v1.is_none());
    }
}
