//! Genuine paid native histories; initial lands and hidden hands are fixture setup.
use super::*;
use crate::{
    graveyard_return_public_v61::{tests as native, ProgramV61},
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    rl::{ActionSemanticV1 as A, TargetRefV1},
    state::{GameState, Zone},
};
fn own(state: GameState, actor: PlayerId) -> HumanGraveyardReturnSessionV61 {
    HumanGraveyardReturnSessionV61::from_session(
        FastActorSessionV1::from_graveyard_return_v61_fixture_state(state),
        actor.into(),
    )
    .unwrap()
}
fn expected(s: &HumanGraveyardReturnSessionV61) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(d) = s.current_response() else {
        panic!("genuine V61 Human scene became terminal")
    };
    d
}
fn gameplay(s: &HumanGraveyardReturnSessionV61) -> (Vec<u8>, (u64, u64)) {
    (
        serde_json::to_vec(s.inner.fixture_state_v61()).unwrap(),
        s.inner.fixture_counts_v61(),
    )
}
fn act(
    s: &mut HumanGraveyardReturnSessionV61,
    p: &mut HumanGraveyardReturnProjectorV61,
    choose: impl Fn(&A) -> bool,
) {
    let d = s.inner.decision_v61().unwrap();
    let e = d.expected();
    let ordinal =
        d.actions()
            .iter()
            .position(choose)
            .unwrap_or_else(|| panic!("missing native action {:?}", d.actions())) as u32;
    let original = d.fixture_original_index_v61(ordinal).unwrap();
    let mut oracle = s.inner.fixture_native_v61();
    if e.acting_player == s.human_seat {
        let prompt = p.project_current(s, e).unwrap();
        assert_eq!(prompt.actions.len(), prompt.action_meanings.len());
        assert_eq!(prompt.state.graveyard_return.meanings.len(), 2);
        let index = p
            .pending
            .as_ref()
            .unwrap()
            .indexes
            .iter()
            .position(|i| *i == ordinal)
            .unwrap() as u32;
        assert_eq!(prompt.actions[index as usize].action_index, index);
        p.submit(
            s,
            HumanActionRequestV1 {
                prompt_seq: prompt.prompt_seq,
                action_index: index,
            },
        )
        .unwrap();
    } else {
        s.step_opponent_with(e, |_, actions| {
            assert_eq!(actions, d.actions());
            Ok(ordinal)
        })
        .unwrap();
    }
    assert_eq!(
        s.current_response(),
        oracle.step(e.episode_id, e.step, original).unwrap()
    );
    assert_eq!(
        s.inner.fixture_state_v61(),
        oracle.graveyard_return_fixture_state()
    );
    assert_eq!(
        s.inner.fixture_counts_v61(),
        (oracle.policy_step_count(), oracle.physical_decision_count())
    );
}
fn quiet(s: &mut HumanGraveyardReturnSessionV61, p: &mut HumanGraveyardReturnProjectorV61) {
    act(s, p, |a| {
        matches!(
            a,
            A::Pass { .. }
                | A::OrderTriggers { .. }
                | A::ChooseTriggerOrderNext { .. }
                | A::ChooseAttackerInclusion { include: false, .. }
                | A::ChooseBlockerInclusion { include: false, .. }
                | A::FinishEffectSelection { .. }
        )
    });
}
fn drain(s: &mut HumanGraveyardReturnSessionV61, p: &mut HumanGraveyardReturnProjectorV61) {
    for _ in 0..100 {
        let world = s.inner.fixture_state_v61();
        if world.stack.is_empty()
            && world.engine.pending_cast.is_none()
            && world.engine.pending_activation.is_none()
            && world.engine.pending_effect.is_none()
            && world.engine.pending_triggers.is_empty()
        {
            return;
        }
        quiet(s, p);
    }
    panic!("real V61 stack did not settle");
}
fn taps(s: &HumanGraveyardReturnSessionV61, actor: PlayerId) -> usize {
    let world = s.inner.fixture_state_v61();
    world.players[actor.index()]
        .battlefield
        .iter()
        .filter(|&&id| {
            let o = world.objects.get(id);
            o.tapped && crate::card_def::CARD_DEFS[usize::from(o.card_def)].is_land
        })
        .count()
}
fn cast(
    s: &mut HumanGraveyardReturnSessionV61,
    p: &mut HumanGraveyardReturnProjectorV61,
    actor: PlayerId,
    id: ObjectId,
    mana: usize,
) {
    let before = taps(s, actor);
    act(
        s,
        p,
        |a| matches!(a,A::CastSpell{source,..}if source.arena_id==id.0),
    );
    let world = s.inner.fixture_state_v61();
    assert!(world.engine.pending_cast.is_none());
    assert_eq!(world.objects.get(id).zone, Zone::Stack);
    assert_eq!(world.stack.last().unwrap().source, id);
    assert_eq!(taps(s, actor) - before, mana);
    assert_eq!(world.players[actor.index()].mana_pool, [0; 6]);
}
#[test]
fn human_v61_paid_both_casts_full_rules_counters_and_native_parity_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut world = native::ready(actor);
        let alesha = put(&mut world, actor, "Alesha, Who Laughs at Fate", Zone::Hand);
        let cat = put(&mut world, actor, "Nine-Lives Familiar", Zone::Hand);
        let mut s = own(world, actor);
        let mut p = HumanGraveyardReturnProjectorV61::new(actor.into());
        let prompt = p.project_current(&s, expected(&s)).unwrap();
        assert_eq!(prompt.state.next_five.meanings.len(), 5);
        for m in &prompt.state.graveyard_return.meanings {
            assert!(m.ordered_rules.len() >= 3);
        }
        cast(&mut s, &mut p, actor, alesha, 3);
        drain(&mut s, &mut p);
        cast(&mut s, &mut p, actor, cat, 3);
        drain(&mut s, &mut p);
        // Advance through genuine priority windows to the Human seat.
        for _ in 0..12 {
            if expected(&s).acting_player == actor.into() {
                break;
            }
            quiet(&mut s, &mut p);
        }
        let prompt = p.project_current(&s, expected(&s)).unwrap();
        assert!(
            prompt.state.graveyard_return.battlefield_counters[actor.index()]
                .iter()
                .any(|r| r.card.name == "Nine-Lives Familiar" && r.counters.revival == 8)
        );
        assert_eq!(
            s.inner.fixture_state_v61().objects.get(alesha).zone,
            Zone::Battlefield
        );
        let json = serde_json::to_value(&prompt).unwrap();
        let all = serde_json::to_string(&json).unwrap();
        for forbidden in [
            "arena_id",
            "zone_change_count",
            "card_db_id",
            "projection_sha256",
            "original_index",
            "producer_sha256",
        ] {
            assert!(!all.contains(forbidden), "{forbidden}");
        }
    }
}
#[test]
fn human_v61_paid_raid_target_may_accept_decline_restore_and_prompt_forgery_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for accept in [false, true] {
            let (world, _, target) = native::raid_ready(actor);
            let mut s = own(world, actor);
            let mut p = HumanGraveyardReturnProjectorV61::new(actor.into());
            let target_prompt = p.project_current(&s, expected(&s)).unwrap();
            assert!(target_prompt.action_meanings.iter().any(|m| m
                .selected_graveyard_target
                .as_ref()
                .is_some_and(|r| r.original.zone == Zone::Graveyard && r.legal_now)));
            act(
                &mut s,
                &mut p,
                |a| matches!(a,A::ChooseTarget{target:TargetRefV1::Object{object},..}if object.arena_id==target.0),
            );
            for _ in 0..40 {
                if s.inner
                    .decision_v61()
                    .unwrap()
                    .actions()
                    .iter()
                    .any(|a| matches!(a, A::ChooseEffectBoolean { .. }))
                {
                    break;
                }
                quiet(&mut s, &mut p);
            }
            let prompt = p.project_current(&s, expected(&s)).unwrap();
            assert!(prompt
                .action_meanings
                .iter()
                .all(|r| r.programs.iter().any(|r| matches!(
                    r.program,
                    ProgramV61::AleshaRaid {
                        may_return_at_resolution: true,
                        optional_choice: public::AleshaChoiceV61::AwaitingCostlessReturnChoice,
                        ..
                    }
                ))));
            assert!(prompt
                .actions
                .iter()
                .any(|a| a.label.starts_with("Accept returning")));
            assert!(prompt
                .actions
                .iter()
                .any(|a| a.label.starts_with("Decline returning")));
            let saved = s.snapshot_v61();
            let stale = HumanActionRequestV1 {
                prompt_seq: prompt.prompt_seq,
                action_index: 0,
            };
            let before_taps = taps(&s, actor);
            // The private cached Human DTO cannot become an alternate authority.
            p.pending.as_mut().unwrap().visible.action_meanings[0]
                .programs
                .clear();
            let before = gameplay(&s);
            assert_eq!(p.submit(&mut s, stale.clone()), Err(Error::StaleDecision));
            assert_eq!(gameplay(&s), before);
            p.project_current(&s, expected(&s)).unwrap();
            let mapped_prompt = p.pending.as_ref().unwrap().visible.prompt_seq;
            p.pending.as_mut().unwrap().indexes.swap(0, 1);
            assert_eq!(
                p.submit(
                    &mut s,
                    HumanActionRequestV1 {
                        prompt_seq: mapped_prompt,
                        action_index: 0
                    }
                ),
                Err(Error::StaleDecision)
            );
            assert_eq!(gameplay(&s), before);
            p.project_current(&s, expected(&s)).unwrap();
            act(
                &mut s,
                &mut p,
                |a| matches!(a,A::ChooseEffectBoolean{value,..}if *value==accept),
            );
            drain(&mut s, &mut p);
            assert_eq!(
                s.inner.fixture_state_v61().objects.get(target).zone,
                if accept {
                    Zone::Battlefield
                } else {
                    Zone::Graveyard
                }
            );
            assert_eq!(taps(&s, actor), before_taps);
            let result = gameplay(&s);
            s.restore_v61(&saved).unwrap();
            let before = gameplay(&s);
            assert_eq!(p.submit(&mut s, stale), Err(Error::StaleDecision));
            assert_eq!(gameplay(&s), before);
            act(
                &mut s,
                &mut p,
                |a| matches!(a,A::ChooseEffectBoolean{value,..}if *value==accept),
            );
            drain(&mut s, &mut p);
            assert_eq!(gameplay(&s), result);
        }
    }
}
#[test]
fn human_v61_paid_familiar_death_schedule_and_delayed_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut world = native::ready(actor);
        let cat = native::paid(&mut world, actor, "Nine-Lives Familiar", None);
        let breath = put(&mut world, actor, "Breath Weapon", Zone::Hand);
        let mut s = own(world, actor);
        let mut p = HumanGraveyardReturnProjectorV61::new(actor.into());
        cast(&mut s, &mut p, actor, breath, 3);
        drain(&mut s, &mut p);
        for _ in 0..12 {
            if expected(&s).acting_player == actor.into() {
                break;
            }
            quiet(&mut s, &mut p);
        }
        let prompt = p.project_current(&s, expected(&s)).unwrap();
        let [schedule] = prompt
            .state
            .graveyard_return
            .nine_lives_schedules
            .as_slice()
        else {
            panic!("missing genuine schedule")
        };
        assert_eq!(schedule.departure_revival, 8);
        assert_eq!(schedule.return_base_before_replacements, 7);
        assert!(schedule.next_future_any_player_end_step);
        assert!(schedule.exact_graveyard_card_now.is_some());
        let saved = s.snapshot_v61();
        let mut result = None;
        for replay in [false, true] {
            if replay {
                s.restore_v61(&saved).unwrap();
            }
            for _ in 0..300 {
                if s.inner.fixture_state_v61().objects.get(cat).zone == Zone::Battlefield {
                    break;
                }
                quiet(&mut s, &mut p);
            }
            assert_eq!(
                s.inner.fixture_state_v61().objects.get(cat).zone,
                Zone::Battlefield
            );
            assert_eq!(
                s.inner
                    .fixture_state_v61()
                    .objects
                    .get(cat)
                    .counters
                    .revival,
                7
            );
            if let Some(result) = &result {
                assert_eq!(&gameplay(&s), result)
            } else {
                result = Some(gameplay(&s));
            }
        }
    }
}
#[test]
fn human_v61_paid_independent_hidden_world_privacy_and_owner_restore_refusal_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let make = |secret: &str| {
            let mut world = native::ready(actor);
            put(&mut world, actor.opponent(), secret, Zone::Hand);
            native::paid(&mut world, actor, "Nine-Lives Familiar", None);
            own(world, actor)
        };
        let mut left = make("Abrade");
        let mut right = make("Vampire Nighthawk");
        let mut p = HumanGraveyardReturnProjectorV61::new(actor.into());
        let mut q = HumanGraveyardReturnProjectorV61::new(actor.into());
        let a = p.project_current(&left, expected(&left)).unwrap();
        let b = q.project_current(&right, expected(&right)).unwrap();
        assert_eq!(a, b);
        let before = gameplay(&right);
        assert_eq!(
            right.restore_v61(&left.snapshot_v61()),
            Err(Error::StaleDecision)
        );
        assert_eq!(gameplay(&right), before);
        let before = gameplay(&left);
        assert_eq!(
            q.submit(
                &mut left,
                HumanActionRequestV1 {
                    prompt_seq: b.prompt_seq,
                    action_index: 0
                }
            ),
            Err(Error::StaleDecision)
        );
        assert_eq!(gameplay(&left), before);
        let request = HumanActionRequestV1 {
            prompt_seq: a.prompt_seq,
            action_index: u32::MAX,
        };
        assert_eq!(p.submit(&mut left, request), Err(Error::InvalidAction));
        assert_eq!(gameplay(&left), before);
        let response = expected(&left);
        assert!(left
            .step_opponent_with(response, |_, _| panic!(
                "Human must not reach opponent callback"
            ))
            .is_err());
        assert_eq!(gameplay(&left), before);
        act(&mut left, &mut p, |a| matches!(a, A::Pass { .. }));
        let receipt = p.accepted.as_ref().unwrap().0.clone();
        let before = gameplay(&left);
        assert_eq!(
            p.submit(
                &mut left,
                HumanActionRequestV1 {
                    prompt_seq: receipt.prompt_seq,
                    action_index: receipt.action_index
                }
            ),
            Ok(receipt.clone())
        );
        assert_eq!(
            p.submit(
                &mut left,
                HumanActionRequestV1 {
                    prompt_seq: receipt.prompt_seq,
                    action_index: u32::MAX
                }
            ),
            Err(Error::ConflictingRetry)
        );
        assert_eq!(gameplay(&left), before);
    }
}
#[test]
fn human_v61_paid_source_scope_keeps_predecessor_and_partial_deck_refusals_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut world = native::ready(actor);
        native::paid(&mut world, actor, "Nine-Lives Familiar", None);
        assert!(
            super::super::next_five_v59::HumanNextFiveSessionV59::from_session(
                FastActorSessionV1::from_graveyard_return_v61_fixture_state(world.clone()),
                actor.into()
            )
            .is_err()
        );
        for name in ["Alesha, Who Laughs at Fate", "Nine-Lives Familiar"] {
            let id = crate::card_def::card_id_by_name(name).unwrap();
            let deck = vec![id; 40];
            assert!(HumanGraveyardReturnSessionV61::reset(
                7,
                11,
                (100, 100),
                ["human-v61-p0".into(), "human-v61-p1".into()],
                [deck.clone(), deck],
                actor,
                actor.into()
            )
            .is_err());
        }
    }
}

#[test]
fn human_v61_paid_raid_current_and_original_departure_power_are_distinct_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut world, alesha, target) = native::raid_ready(actor);
        let current = own(world.clone(), actor);
        let mut p = HumanGraveyardReturnProjectorV61::new(actor.into());
        let prompt = p.project_current(&current, expected(&current)).unwrap();
        assert!(prompt.state.graveyard_return.triggers.iter().any(|r|matches!(&r.program,ProgramV61::AleshaRaid{power,..}if power.amount==3&&power.source==public::PowerSourceV61::CurrentOriginalIncarnation)));
        crate::engine::step(
            &mut world,
            crate::engine::Action::ChooseTarget(crate::state::Target::Object(target)),
        )
        .unwrap();
        let removal = put(&mut world, actor.opponent(), "Abrade", Zone::Hand);
        native::announce(&mut world, actor.opponent(), removal, Some(alesha));
        for _ in 0..12 {
            let decision = crate::engine::advance_until_decision(&mut world);
            if world.objects.get(alesha).zone == Zone::Graveyard {
                break;
            }
            native::quiet(&mut world, decision);
        }
        assert_eq!(world.objects.get(alesha).zone, Zone::Graveyard);
        let departed = own(world, actor);
        let mut q = HumanGraveyardReturnProjectorV61::new(actor.into());
        let prompt = q.project_current(&departed, expected(&departed)).unwrap();
        assert!(prompt.state.graveyard_return.triggers.iter().any(|r|matches!(&r.program,ProgramV61::AleshaRaid{power,..}if power.amount==3&&power.source==public::PowerSourceV61::ExactDepartureLki)&&r.source.current_original_source.is_none()&&r.targets.len()==1&&r.targets[0].legal_now));
        assert_eq!(prompt.state.graveyard_return.alesha_departures.len(), 1);
        assert_eq!(prompt.state.graveyard_return.alesha_departures[0].power, 3);
    }
}
