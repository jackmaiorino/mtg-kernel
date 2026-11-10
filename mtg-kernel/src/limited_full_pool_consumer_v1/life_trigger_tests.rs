use super::*;
use crate::{ids::ObjectId, state::Step};
use trigger_programs::{FactsV1, ProgramV1};

pub(super) fn facts(d: &KaitoDecision) -> &FactsV1 {
    d.fixture_backend_v75()
        .observation()
        .trigger_programs
        .as_ref()
        .unwrap()
}

pub(super) fn choose(
    owner: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    d: &KaitoDecision,
    pick: impl Fn(&Action) -> bool,
) {
    let backend = d.fixture_backend_v75();
    let ordinal = u32::try_from(backend.actions().iter().position(pick).unwrap_or_else(|| {
        panic!(
            "life trigger fixture action missing: {:?}",
            backend.actions()
        )
    }))
    .unwrap();
    let native_index = backend.fixture_original_index_v75(ordinal).unwrap();
    let expected = backend.expected();
    assert_eq!(
        owner.apply(d, ordinal).unwrap(),
        native
            .step(expected.episode_id, expected.step, native_index)
            .unwrap(),
    );
    assert_eq!(owner.fixture_state(), native.kaito_fixture_state());
}

fn tokens(state: &GameState, actor: PlayerId, name: &str) -> Vec<ObjectId> {
    state.players[actor.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&id| state.objects.get(id).card_def == card(name))
        .collect()
}

fn settle(
    owner: &mut KaitoSession,
    native: &mut FastActorSessionV1,
    actor: PlayerId,
    seraph: ObjectId,
) -> (Vec<ProgramV1>, Option<ObjectId>) {
    let mut seen = Vec::new();
    let mut milled = None;
    for _ in 0..48 {
        let d = decision(owner);
        let visible = facts(&d);
        for row in visible.stack.iter().chain(&visible.pending) {
            if !seen.contains(&row.program) {
                seen.push(row.program);
                assert_model(owner, &d);
                if row.program == ProgramV1::CollectorFood {
                    let backend = d.fixture_backend_v75();
                    let (wire, _) =
                        project_limited_kaito_v16(backend, backend.expected().acting_player, 1)
                            .unwrap();
                    assert_eq!(wire.state.trigger_programs.as_ref(), Some(visible));
                    let mut missing = backend.observation().clone();
                    missing.trigger_programs.as_mut().unwrap().stack.clear();
                    missing.trigger_programs.as_mut().unwrap().pending.clear();
                    missing.projection_sha256 =
                        crate::kaito_observation_v75::commitment(&missing).unwrap();
                    assert!(crate::kaito_observation_v75::validate_artifact(&missing).is_err());
                }
            }
        }
        let state = owner.fixture_state();
        if state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
            && state.engine.pending_effect.is_none()
        {
            return (seen, milled);
        }
        let choice = |action: &Action| {
            matches!(action, Action::ChooseEffectOption { source, option_index: 1, option_count: 2, .. }
                if source.arena_id == seraph.0)
        };
        if d.fixture_backend_v75().actions().iter().any(choice) {
            let top = state.players[actor.index()].library[0];
            let backend = d.fixture_backend_v75();
            assert_eq!(backend.expected().acting_player, actor.into());
            assert!(
                backend.observation().common.common.known_library_cards[actor.index()]
                    .iter()
                    .any(|row| row.position == 0 && row.card.stable.arena_id == top.0)
            );
            let saved = owner.snapshot();
            let native_saved = native.snapshot_v1();
            choose(owner, native, &d, choice);
            assert_eq!(owner.fixture_state().objects.get(top).zone, Zone::Graveyard);
            owner.restore(&saved).unwrap();
            native.kaito_restore_v75(&native_saved).unwrap();
            assert!(owner.apply(&d, 0).is_err());
            let rebound = decision(owner);
            assert_eq!(rebound.packet(), d.packet());
            choose(owner, native, &rebound, choice);
            milled = Some(top);
        } else {
            choose(owner, native, &d, |action| {
                matches!(
                    action,
                    Action::Pass { .. }
                        | Action::OrderTriggers { .. }
                        | Action::ChooseTriggerOrderNext { .. }
                )
            });
        }
    }
    panic!("actual life trigger resolution did not settle");
}

fn assert_exemplar_cap_lifetime(
    state: &GameState,
    actor: PlayerId,
    exemplar: ObjectId,
    food: ObjectId,
    seraph: ObjectId,
) {
    let original = state
        .trigger_uses_v1
        .clone()
        .expect("actual first gain paid its cap");
    let original_generation = state.objects.get(exemplar).zone_change_count;
    assert!(original.iter().any(|use_| {
        use_.source.object == exemplar && use_.source.zone_change_count == original_generation
    }));
    let mut reentered = state.clone();
    crate::event::propose_and_commit(
        &mut reentered,
        crate::event::ProposedEvent::zone_change(exemplar, Zone::Hand),
    );
    assert!(reentered.trigger_uses_v1.is_none());
    for printing in ["Exemplar of Light", "Plains"] {
        let mut hidden = reentered.clone();
        hidden.objects.get_mut(exemplar).card_def = card(printing);
        hidden.trigger_uses_v1 = Some(original.clone());
        assert_eq!(trigger_programs::project_live_uses(&hidden).unwrap(), None);
    }
    crate::event::propose_and_commit(
        &mut reentered,
        crate::event::ProposedEvent::zone_change(exemplar, Zone::Battlefield),
    );
    let new_generation = reentered.objects.get(exemplar).zone_change_count;
    assert!(new_generation > original_generation);
    reentered.trigger_uses_v1 = Some(original);
    assert_eq!(
        trigger_programs::project_live_uses(&reentered).unwrap(),
        None
    );

    let mut owner = KaitoSession::from_session(raw(&reentered)).unwrap();
    let mut native = raw(&reentered);
    let decision = decision(&owner);
    choose(&mut owner, &mut native, &decision, |action| {
        matches!(action, Action::ActivateAbility { source, ability_index: 0, .. }
            if source.arena_id == food.0)
    });
    let (seen, milled) = settle(&mut owner, &mut native, actor, seraph);
    assert!(milled.is_none());
    assert_eq!(
        seen,
        vec![ProgramV1::ExemplarCounter, ProgramV1::ExemplarDraw]
    );
    let decision = super::decision(&owner);
    let uses = decision
        .fixture_backend_v75()
        .observation()
        .common
        .common
        .projection
        .surface
        .engine_context
        .trigger_uses
        .as_ref()
        .unwrap();
    assert!(uses.iter().any(|use_| {
        use_.source.arena_id == exemplar.0
            && use_.source.zone_change_count == new_generation
            && use_.ability_index == 1
            && use_.uses == 1
    }));
}

#[test]
fn paid_life_creatures_publish_exact_programs_and_caps_both_seats() {
    use ProgramV1::*;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let library = [card("Plains"); 40];
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &library,
            &library,
            crate::rl::card_name,
            550_218,
            actor,
        );
        state.step = Step::Main1;
        assert!(state.life_gain_turn_v1.is_none());
        FastActorSessionV1::enable_limited_native_modes_v1(&mut state, LimitedAppendixV1::FullPool);
        for _ in 0..15 {
            put(&mut state, actor, "Plains", Zone::Battlefield);
        }
        put(&mut state, actor.opponent(), "Plains", Zone::Battlefield);
        let exemplar = put(&mut state, actor, "Exemplar of Light", Zone::Hand);
        let seraph = put(&mut state, actor, "Vanguard Seraph", Zone::Hand);
        let collector = put(&mut state, actor, "Cat Collector", Zone::Hand);
        let second_food = put(&mut state, actor, "Food Token", Zone::Battlefield);
        let mut owner = KaitoSession::from_session(raw(&state)).unwrap();
        let mut native = raw(&state);
        let tapped = |state: &GameState| {
            state.players[actor.index()]
                .battlefield
                .iter()
                .filter(|&&id| {
                    let object = state.objects.get(id);
                    object.tapped && object.card_def == card("Plains")
                })
                .count()
        };
        for (source, cost) in [(exemplar, 4), (seraph, 4), (collector, 3)] {
            let d = decision(&owner);
            assert_model(&owner, &d);
            assert_eq!(facts(&d).first_positive_gain_consumed, [false; 2]);
            let before = tapped(owner.fixture_state());
            choose(
                &mut owner,
                &mut native,
                &d,
                |action| matches!(action, Action::CastSpell { source: card, .. } if card.arena_id == source.0),
            );
            assert_eq!(tapped(owner.fixture_state()) - before, cost);
            let (seen, _) = settle(&mut owner, &mut native, actor, seraph);
            assert_eq!(
                owner.fixture_state().objects.get(source).zone,
                Zone::Battlefield
            );
            assert_eq!(
                seen,
                if source == collector {
                    vec![CollectorFood]
                } else {
                    vec![]
                }
            );
        }
        let food = tokens(owner.fixture_state(), actor, "Food Token")
            .into_iter()
            .find(|&id| id != second_food)
            .unwrap();
        let hand = owner.fixture_state().players[actor.index()].hand.len();
        for (index, source) in [food, second_food].into_iter().enumerate() {
            let d = decision(&owner);
            let before = tapped(owner.fixture_state());
            choose(&mut owner, &mut native, &d, |action| {
                matches!(action, Action::ActivateAbility { source: card, ability_index: 0, .. }
                    if card.arena_id == source.0)
            });
            assert_eq!(tapped(owner.fixture_state()) - before, 2);
            assert_ne!(
                owner.fixture_state().objects.get(source).zone,
                Zone::Battlefield
            );
            let (seen, milled) = settle(&mut owner, &mut native, actor, seraph);
            assert!(seen.contains(&ExemplarCounter));
            assert_eq!(seen.contains(&ExemplarDraw), index == 0);
            assert_eq!(seen.contains(&VanguardSurveil), index == 0);
            assert_eq!(seen.contains(&CollectorCat), index == 0);
            assert_eq!(milled.is_some(), index == 0);
            let live = owner.fixture_state();
            assert_eq!(
                live.players[actor.index()].life,
                23 + 3 * i32::try_from(index).unwrap()
            );
            assert_eq!(live.players[actor.index()].hand.len(), hand + 1);
            assert_eq!(
                live.objects.get(exemplar).counters.plus1_plus1,
                i32::try_from(index + 1).unwrap()
            );
            assert_eq!(tokens(live, actor, "Cat Token").len(), 1);
            let d = decision(&owner);
            let mut consumed = [false; 2];
            consumed[actor.index()] = true;
            assert_eq!(facts(&d).first_positive_gain_consumed, consumed);
            let public = &d
                .fixture_backend_v75()
                .observation()
                .common
                .common
                .projection
                .surface;
            assert!(public
                .engine_context
                .trigger_uses
                .as_ref()
                .unwrap()
                .iter()
                .any(|row| {
                    row.source.arena_id == exemplar.0 && row.ability_index == 1 && row.uses == 1
                }));
            if index == 0 {
                let mut forged = d.fixture_backend_v75().observation().clone();
                let source = &mut forged
                    .common
                    .common
                    .projection
                    .surface
                    .engine_context
                    .trigger_uses
                    .as_mut()
                    .unwrap()[0]
                    .source;
                source.zone = Zone::Hand;
                assert_eq!(
                    trigger_programs::validate(
                        facts(&d),
                        &forged.common.common.projection.surface,
                    )
                    .unwrap_err(),
                    "invalid FullPool public trigger cap ledger"
                );
                forged.projection_sha256 =
                    crate::kaito_observation_v75::commitment(&forged).unwrap();
                assert!(crate::kaito_observation_v75::validate_artifact(&forged).is_err());
                assert_exemplar_cap_lifetime(live, actor, exemplar, second_food, seraph);
            }
        }
    }
}

#[test]
fn full_pool_opening_life_history_does_not_depend_on_hidden_printings() {
    let plains = vec![card("Plains"); 40];
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut config = ResetV1 {
            expected_card_db_hash: KERNEL_CARDDB_HASH,
            episode_id: 550,
            env_seed: 550_333,
            starting_player: actor.into(),
            deck_ids: ["first".into(), "second".into()],
            mainboards: [plains.clone(), plains.clone()],
            max_physical_decisions: 1_000,
            max_policy_steps: 1_000,
        };
        let empty = reset_numeric_reveal_facts_v3(&config).unwrap();
        config.mainboards[actor.opponent().index()][0] = card("Cat Collector");
        let other = reset_numeric_reveal_facts_v3(&config).unwrap();
        let first = decision(&empty);
        let second = decision(&other);
        assert_eq!(
            first.fixture_backend_v75().expected().acting_player,
            actor.into()
        );
        assert_eq!(first.packet(), second.packet());
        assert_eq!(facts(&first).first_positive_gain_consumed, [false; 2]);
        assert_eq!(facts(&first), facts(&second));
        assert!(empty.fixture_state().life_gain_turn_v1.is_some());
        assert!(other.fixture_state().life_gain_turn_v1.is_some());
    }
}
