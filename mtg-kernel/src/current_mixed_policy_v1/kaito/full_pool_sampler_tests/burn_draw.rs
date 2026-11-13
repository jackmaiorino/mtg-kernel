use super::*;
use crate::{
    current_mixed_policy_v1::Error,
    effect::thousand_year_storm_v1_tests::ready,
    event::{self, CommittedEvent, ProposedEvent},
    rl::ActionSemanticV1 as A,
    scrawling_crawler_public_v1::{IndexedTriggerV1, TriggerV1},
    state::Step,
};

pub(super) fn owner(state: GameState) -> KaitoSession {
    KaitoSession::from_session(FastActorSessionV1::from_kaito_v75_fixture_state_scoped(
        state,
        LimitedAppendixV1::FullPool,
    ))
    .unwrap()
}

pub(super) fn act(owner: &mut KaitoSession, choose: impl Fn(&A) -> bool) {
    let d = decision(owner);
    let backend = d.fixture_backend_v75();
    let index = backend.actions().iter().position(choose).unwrap();
    let original = backend.fixture_original_index_v75(index as u32).unwrap();
    let mut native = owner.inner.fixture_native_clone_v75();
    let expected = backend.expected();
    assert_eq!(
        owner.apply(&d, index as u32).unwrap(),
        native
            .step(expected.episode_id, expected.step, original)
            .unwrap(),
    );
    assert_eq!(owner.fixture_state(), native.kaito_fixture_state());
}

pub(super) fn pass(owner: &mut KaitoSession) {
    act(owner, |action| matches!(action, A::Pass { .. }));
}

pub(super) fn settle(owner: &mut KaitoSession) {
    for _ in 0..8 {
        if owner.fixture_state().stack.is_empty() {
            return;
        }
        pass(owner);
    }
    panic!("native spell or trigger did not settle");
}

pub(super) fn tap_count(state: &GameState, actor: PlayerId) -> usize {
    state.players[actor.index()]
        .battlefield
        .iter()
        .filter(|id| state.objects.get(**id).tapped)
        .count()
}

#[test]
fn full_pool_slagstorm_paid_modes_bind_original_ordinals_and_resolution_both_seats() {
    let id = card("Slagstorm");
    assert!(matches!(
        super::super::super::roots::baseline(id),
        Err(Error::UnsupportedPrintedRoot(actual)) if actual == id
    ));
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in [0, 1] {
            let mut state = ready(actor, "Swamp");
            modes(&mut state);
            let spell = put(&mut state, actor, "Slagstorm", Zone::Hand);
            let own_creature = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
            let enemy_creature = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let mut owner = owner(state);
            act(&mut owner, |action| {
                matches!(
                    action, A::CastSpell { source, .. } if source.arena_id == spell.0
                )
            });
            let d = decision(&owner);
            let modes: Vec<_> = d
                .fixture_backend_v75()
                .actions()
                .iter()
                .filter_map(|action| match action {
                    A::ChooseSpellMode {
                        source,
                        mode_index,
                        mode_count: 2,
                        ..
                    } if source.arena_id == spell.0 => Some(*mode_index),
                    _ => None,
                })
                .collect();
            assert_eq!(modes, [0, 1]);
            act(&mut owner, |action| {
                matches!(
                    action, A::ChooseSpellMode { mode_index, .. } if *mode_index == mode
                )
            });
            let published = decision(&owner);
            let stack = &published
                .fixture_backend_v75()
                .observation()
                .common
                .common
                .projection
                .surface
                .stack;
            assert!(
                matches!(stack.as_slice(), [row] if row.source.arena_id == spell.0 && row.mode_chosen == mode && row.targets.is_empty())
            );
            assert_eq!(tap_count(owner.fixture_state(), actor), 3);
            settle(&mut owner);
            let after = owner.fixture_state();
            assert_eq!(after.objects.get(spell).zone, Zone::Graveyard);
            for creature in [own_creature, enemy_creature] {
                assert_eq!(
                    after.objects.get(creature).zone,
                    if mode == 0 {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
            }
            for player in [actor, actor.opponent()] {
                assert_eq!(
                    after.players[player.index()].life,
                    if mode == 0 { 200 } else { 197 }
                );
            }
        }
    }
}

fn paid_crawler(actor: PlayerId, hidden: &str) -> GameState {
    let mut state = ready(actor, hidden);
    modes(&mut state);
    let source = put(&mut state, actor, "Scrawling Crawler", Zone::Hand);
    let mut owner = owner(state);
    act(&mut owner, |action| {
        matches!(
            action, A::CastSpell { source: row, .. } if row.arena_id == source.0
        )
    });
    assert_eq!(tap_count(owner.fixture_state(), actor), 3);
    settle(&mut owner);
    assert_eq!(
        owner.fixture_state().objects.get(source).zone,
        Zone::Battlefield
    );
    owner.fixture_state().clone()
}

#[test]
fn full_pool_crawler_paid_upkeep_and_draw_loss_bind_public_programs_both_seats() {
    let id = card("Scrawling Crawler");
    assert!(matches!(
        super::super::super::roots::baseline(id),
        Err(Error::UnsupportedPrintedRoot(actual)) if actual == id
    ));
    for actor in [PlayerId::P0, PlayerId::P1] {
        let upkeep = |hidden| {
            let mut state = paid_crawler(actor, hidden);
            // Start a real native upkeep boundary from the paid battlefield state.
            state.step = Step::Upkeep;
            state.priority_player = actor;
            event::log_upkeep_began(&mut state, actor);
            owner(state)
        };
        let mut left = upkeep("Swamp");
        let mut right = upkeep("Island");
        let first = decision(&left);
        assert_eq!(first.packet(), decision(&right).packet());
        let public = first.fixture_backend_v75().observation();
        assert_eq!(
            public.scrawling_crawler.as_ref().unwrap().stack,
            [IndexedTriggerV1 {
                index: 0,
                trigger: TriggerV1::ControllerUpkeep
            }],
        );
        let mut omitted = public.clone();
        omitted.scrawling_crawler.as_mut().unwrap().stack.clear();
        omitted.projection_sha256 = crate::kaito_observation_v75::commitment(&omitted).unwrap();
        assert!(crate::kaito_observation_v75::validate_artifact(&omitted).is_err());
        let snapshot = left.snapshot();
        left.restore(&snapshot).unwrap();
        assert_eq!(first.packet(), decision(&left).packet());

        for current in [&mut left, &mut right] {
            pass(current);
            pass(current);
            let loss = decision(current);
            assert_eq!(
                loss.fixture_backend_v75()
                    .observation()
                    .scrawling_crawler
                    .as_ref()
                    .unwrap()
                    .stack,
                [IndexedTriggerV1 {
                    index: 0,
                    trigger: TriggerV1::OpponentDraw
                }],
            );
            assert_eq!(
                current.fixture_state().players[actor.opponent().index()].life,
                200
            );
            let draw_order: Vec<_> = current
                .fixture_state()
                .engine
                .event_history
                .iter()
                .filter_map(|event| match event {
                    CommittedEvent::Draw {
                        player,
                        object: Some(_),
                    } => Some(*player),
                    _ => None,
                })
                .collect();
            assert_eq!(draw_order, [actor, actor.opponent()]);
        }
        assert_eq!(decision(&left).packet(), decision(&right).packet());
        for current in [&mut left, &mut right] {
            settle(current);
            assert_eq!(current.fixture_state().players[actor.index()].life, 200);
            assert_eq!(
                current.fixture_state().players[actor.opponent().index()].life,
                199
            );
        }

        // Actual successful opponent draws produce two independently ordered triggers.
        let mut state = paid_crawler(actor, "Swamp");
        for _ in 0..2 {
            event::propose_and_commit(&mut state, ProposedEvent::draw(actor.opponent()));
        }
        let mut ordered = owner(state);
        let pending = decision(&ordered);
        assert_eq!(
            pending
                .fixture_backend_v75()
                .observation()
                .scrawling_crawler
                .as_ref()
                .unwrap()
                .pending,
            [
                IndexedTriggerV1 {
                    index: 0,
                    trigger: TriggerV1::OpponentDraw
                },
                IndexedTriggerV1 {
                    index: 1,
                    trigger: TriggerV1::OpponentDraw
                },
            ],
        );
        act(
            &mut ordered,
            |action| matches!(action, A::OrderTriggers { order, .. } if order.as_slice() == [1, 0]),
        );
        settle(&mut ordered);
        assert_eq!(
            ordered.fixture_state().players[actor.opponent().index()].life,
            198
        );
    }
}
