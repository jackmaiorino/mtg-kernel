use super::{
    burn_draw::{act, owner},
    *,
};
use crate::{
    effect::thousand_year_storm_v1_tests as native,
    engine::{Action, Decision},
    ids::ObjectId,
    rl::ActionSemanticV1,
    state::{StackItemKind, Target},
};

fn attack(state: &mut GameState, kiora: ObjectId) {
    for _ in 0..64 {
        match native::next(state) {
            Decision::DeclareAttackers { eligible, .. } if eligible.contains(&kiora) => {
                native::action(state, Action::DeclareAttackers(vec![kiora]));
                return;
            }
            Decision::DeclareAttackers { .. } => {
                native::action(state, Action::DeclareAttackers(vec![]));
            }
            Decision::DeclareBlockers { .. } => {
                native::action(state, Action::DeclareBlockers(vec![]));
            }
            Decision::CastSpellOrPass { .. } => native::action(state, Action::Pass),
            other => panic!("unexpected decision before paid Kiora attacks: {other:?}"),
        }
    }
    panic!("paid Kiora never became eligible to attack");
}

fn tuck_and_shuffle(state: &mut GameState, actor: PlayerId, kiora: ObjectId) {
    let voyage = native::cast(
        state,
        actor.opponent(),
        "Uncharted Voyage",
        &[Target::Object(kiora)],
        4,
    );
    for _ in 0..12 {
        if state.objects.get(voyage).zone == Zone::Graveyard {
            break;
        }
        match native::next(state) {
            Decision::ChooseEffectOption {
                source,
                player,
                option_count: 2,
            } if source == voyage && player == actor => {
                native::action(state, Action::ChooseEffectOption(0));
            }
            Decision::ChooseEffectTargets {
                player,
                can_finish: true,
                ..
            } if player == actor.opponent() => {
                native::action(state, Action::FinishEffectSelection);
            }
            Decision::CastSpellOrPass { .. } => {
                if state.objects.get(voyage).zone == Zone::Graveyard {
                    break;
                }
                native::action(state, Action::Pass);
            }
            other => panic!("unexpected paid Uncharted Voyage choice: {other:?}"),
        }
    }
    assert_eq!(state.objects.get(voyage).zone, Zone::Graveyard);
    assert_eq!(state.objects.get(kiora).zone, Zone::Library);
    // Commit a native shuffle while the original threshold trigger is waiting.
    state.shuffle_library(actor).unwrap();
    assert!(state
        .known_library_cards(actor, actor)
        .iter()
        .all(|row| row.object != kiora));
    for _ in 0..8 {
        match native::next(state) {
            Decision::ChooseEffectOption {
                source,
                player,
                option_count: 2,
            } if source == kiora && player == actor => return,
            Decision::CastSpellOrPass { .. } => native::action(state, Action::Pass),
            other => panic!("unexpected threshold choice: {other:?}"),
        }
    }
    panic!("original threshold trigger never offered its choice");
}

#[test]
fn full_pool_samples_paid_kiora_threshold_after_source_is_shuffled_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = native::ready(actor, "Island");
        modes(&mut state);
        for _ in 0..5 {
            put(&mut state, actor, "Forest", Zone::Graveyard);
        }
        let kiora = native::cast(&mut state, actor, "Kiora, the Rising Tide", &[], 3);
        native::finish(&mut state);
        assert_eq!(state.players[actor.index()].graveyard.len(), 7);
        attack(&mut state, kiora);
        assert!(matches!(
            native::next(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        let original = state
            .stack
            .iter()
            .find(|row| row.kind == StackItemKind::TriggeredAbility && row.source == kiora)
            .unwrap()
            .v4
            .ability_source_contract
            .unwrap();
        tuck_and_shuffle(&mut state, actor, kiora);
        assert!(state.objects.get(kiora).zone_change_count > original.zone_change_count);
        let opponent = &state.players[actor.opponent().index()];
        let corpus = opponent
            .library
            .iter()
            .chain(&opponent.hand)
            .chain(&opponent.battlefield)
            .chain(&opponent.graveyard)
            .map(|&id| state.objects.get(id).card_def)
            .collect::<Vec<_>>();
        let belief = LimitedBeliefV1::new([corpus]);
        let live = owner(state);
        let root = decision(&live);
        let untouched = live.fixture_state().clone();
        let mut rebound = false;
        for seed in [552_227, 552_228, 552_229, 552_230] {
            let mut sampled = live
                .sampled_full_pool_closed_list_v1(&root, seed, &belief)
                .unwrap();
            let fresh = decision(&sampled);
            assert_eq!(fresh.packet(), root.packet());
            if sampled.fixture_state().objects.get(kiora).card_def == original.card_def {
                continue;
            }
            rebound = true;
            let choices = fresh.fixture_backend_v75().actions();
            assert_eq!(choices.len(), 2);
            for (index, action) in choices.iter().enumerate() {
                assert!(matches!(action,
                    ActionSemanticV1::ChooseEffectOption { source, option_index, option_count: 2, .. }
                    if usize::from(*option_index) == index && source.arena_id == kiora.0
                        && source.card_db_id == original.card_def
                        && source.zone == Zone::Battlefield
                        && source.zone_change_count == original.zone_change_count
                ));
            }
            let saved = sampled.snapshot();
            for index in [0, 1] {
                act(&mut sampled, |action| {
                    matches!(
                        action, ActionSemanticV1::ChooseEffectOption { option_index, .. }
                        if *option_index == index
                    )
                });
                let after = sampled.fixture_state();
                let tokens = after.players[actor.index()]
                    .battlefield
                    .iter()
                    .filter(|&&id| {
                        after.objects.get(id).card_def == card("Scion of the Deep Token")
                    })
                    .count();
                assert_eq!(tokens, usize::from(index));
                sampled.restore(&saved).unwrap();
                assert_eq!(decision(&sampled).packet(), root.packet());
            }
            break;
        }
        assert!(
            rebound,
            "sampler must rewrite the original physical library slot"
        );
        assert_eq!(live.fixture_state(), &untouched);
    }
}
