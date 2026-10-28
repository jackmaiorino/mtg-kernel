//! Paid native fixture streams; no public cancellation or Partial reset admission.
use super::*;
use crate::{
    effect::liliana_dreadhorde_v1::tests::owner_v73_tests::owning_ready, ids::PlayerId,
    rl::ActionSemanticV1 as A, state::Zone,
};
fn quiet(a: &A) -> bool {
    matches!(
        a,
        A::Pass { .. }
            | A::OrderTriggers { .. }
            | A::ChooseTriggerOrderNext { .. }
            | A::ChooseAttackerInclusion { include: false, .. }
            | A::ChooseBlockerInclusion { include: false, .. }
            | A::ChooseEffectTarget { .. }
    )
}
fn act(
    own: &mut HumanLilianaSessionV73,
    h: &mut HumanLilianaProjectorV73,
    native: &mut FastActorSessionV1,
    choose: impl Fn(&A) -> bool,
) {
    let d = own.inner.decision_v73().unwrap();
    let e = d.expected();
    let i = d.actions().iter().position(choose).unwrap();
    let original = d.fixture_original_index_v73(i as u32).unwrap();
    if e.acting_player == own.human_seat {
        let visible = h.project_current(own, e).unwrap();
        let display = h
            .pending
            .as_ref()
            .unwrap()
            .indexes
            .iter()
            .position(|x| *x == i as u32)
            .unwrap();
        let request = HumanActionRequestV1 {
            prompt_seq: visible.prompt_seq,
            action_index: display as u32,
        };
        let receipt = h.submit(own, request.clone()).unwrap();
        let after = own.inner.fixture_state_v73().clone();
        assert_eq!(h.submit(own, request).unwrap(), receipt);
        assert_eq!(own.inner.fixture_state_v73(), &after);
    } else {
        own.step_opponent_with(e, |_, _| Ok(i as u32)).unwrap();
    }
    native.step(e.episode_id, e.step, original).unwrap();
    assert_eq!(
        own.inner.fixture_state_v73(),
        native.liliana_fixture_state()
    );
    assert_eq!(
        own.inner.fixture_counts_v73(),
        (native.policy_step_count(), native.physical_decision_count())
    );
}

fn complete(
    own: &mut HumanLilianaSessionV73,
    h: &mut HumanLilianaProjectorV73,
    native: &mut FastActorSessionV1,
) {
    for _ in 0..512 {
        let d = own.inner.decision_v73().unwrap();
        if d.observation().liliana.activations.last().is_some_and(|a| {
            a.stage == crate::liliana_public_v73::ActivationStageV73::NativeCompleted
        }) {
            return;
        }
        if let Some(choice) = d
            .candidates_v73()
            .candidates
            .first()
            .and_then(|r| r.liliana_choices.first())
        {
            if matches!(
                choice.kind,
                crate::liliana_public_v73::SelectionKindV73::KeepPermanent(
                    crate::card_def::CardType::Creature | crate::card_def::CardType::Artifact
                )
            ) && choice.selected_prefix.is_empty()
            {
                let hart = choice
                    .candidates
                    .iter()
                    .find(|r| {
                        own.inner
                            .fixture_state_v73()
                            .objects
                            .get(crate::ids::ObjectId(r.object))
                            .card_def
                            == crate::card_def::card_id_by_name("Burnished Hart").unwrap()
                    })
                    .unwrap()
                    .object;
                act(
                    own,
                    h,
                    native,
                    |a| matches!(a,A::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==hart),
                );
                continue;
            }
        }
        let finish = d
            .actions()
            .iter()
            .any(|a| matches!(a, A::FinishEffectSelection { .. }));
        act(own, h, native, |a| {
            if finish {
                matches!(a, A::FinishEffectSelection { .. })
            } else {
                quiet(a)
            }
        });
    }
    panic!("Human native root did not complete")
}
#[test]
fn genuine_paid_human_three_roots_original_loyalty_apnap_choices_native_parity_restore_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        for index in [0, 1, 2] {
            let (s, id) = owning_ready(p, index == 2, "Island");
            let mut native = FastActorSessionV1::from_liliana_v73_fixture_state(s.clone());
            let mut own = HumanLilianaSessionV73::from_session(
                FastActorSessionV1::from_liliana_v73_fixture_state(s),
                p.into(),
            )
            .unwrap();
            let mut h = HumanLilianaProjectorV73::new(p.into());
            let d = h
                .project_current(&own, own.inner.decision_v73().unwrap().expected())
                .unwrap();
            assert!(d
                .actions
                .iter()
                .any(|a| a.label.contains("loyalty") && a.label.contains("Liliana")));
            let saved = own.snapshot_v73();
            let raw = native.snapshot_v1();
            own.restore_v73(&saved).unwrap();
            native.liliana_restore_v73(&raw).unwrap();
            let before = own.inner.fixture_state_v73().clone();
            assert!(h
                .submit(
                    &mut own,
                    HumanActionRequestV1 {
                        prompt_seq: d.prompt_seq,
                        action_index: 0
                    }
                )
                .is_err());
            assert_eq!(own.inner.fixture_state_v73(), &before);
            act(
                &mut own,
                &mut h,
                &mut native,
                |a| matches!(a,A::ActivateAbility{source,ability_index,..}if source.arena_id==id.0&&*ability_index==index),
            );
            complete(&mut own, &mut h, &mut native);
            let d = own.inner.decision_v73().unwrap();
            let (visible, _, _) = project_with_handles(&d, d.expected().acting_player, 1).unwrap();
            let f = &visible.state.liliana;
            assert_eq!(f.activations[0].ability_index, index);
            assert_eq!(f.battlefield, visible.state.tinybones.battlefield);
            if index == 0 {
                assert_eq!(f.activations[0].created_zombies.len(), 1);
            } else {
                assert!(f.resolutions[0].finished);
                assert_eq!(f.resolutions[0].ultimate, index == 2);
                if index == 2 {
                    let keepers = &f.resolutions[0].keepers;
                    let creature = &keepers
                        .iter()
                        .find(|k| k.permanent_type == crate::card_def::CardType::Creature)
                        .unwrap()
                        .permanent;
                    assert_eq!(
                        keepers.iter().filter(|k| &k.permanent == creature).count(),
                        2
                    );
                    assert!(!f.resolutions[0].completed_sacrifices.contains(creature));
                }
            }
            let text = serde_json::to_string(&visible).unwrap();
            for forbidden in [
                "arena_id",
                "zone_change_count",
                "history_index",
                "private_world",
                "original_library",
            ] {
                assert!(!text.contains(forbidden));
            }
        }
    }
}
#[test]
fn genuine_paid_human_foreign_changed_packet_callback_and_later_owner_refusals_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, _) = owning_ready(p, false, "Island");
        let mut a = HumanLilianaSessionV73::from_session(
            FastActorSessionV1::from_liliana_v73_fixture_state(s.clone()),
            p.into(),
        )
        .unwrap();
        let b = HumanLilianaSessionV73::from_session(
            FastActorSessionV1::from_liliana_v73_fixture_state(s),
            p.into(),
        )
        .unwrap();
        let mut h = HumanLilianaProjectorV73::new(p.into());
        let e = a.inner.decision_v73().unwrap().expected();
        let d = h.project_current(&a, e).unwrap();
        let before = a.inner.fixture_state_v73().clone();
        assert!(a.restore_v73(&b.snapshot_v73()).is_err());
        assert!(a.step_opponent_with(e, |_, _| Ok(0)).is_err());
        h.pending
            .as_mut()
            .unwrap()
            .visible
            .state
            .liliana
            .battlefield[p.index()][0]
            .counters
            .stash = 1;
        assert!(h
            .submit(
                &mut a,
                HumanActionRequestV1 {
                    prompt_seq: d.prompt_seq,
                    action_index: 0
                }
            )
            .is_err());
        assert_eq!(a.inner.fixture_state_v73(), &before);
        assert!(crate::rl_session::TinybonesSessionV72::from_session(
            a.inner.fixture_native_clone_v73()
        )
        .is_err());
    }
}
#[test]
fn independently_paid_human_hidden_world_equality_and_exact_eighteen_counter_prefix_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut projected = vec![];
        for hidden in ["Island", "Imprisoned in the Moon"] {
            let (s, _) = owning_ready(p, false, hidden);
            let own = HumanLilianaSessionV73::from_session(
                FastActorSessionV1::from_liliana_v73_fixture_state(s),
                p.into(),
            )
            .unwrap();
            projected.push(
                HumanLilianaProjectorV73::new(p.into())
                    .project_current(&own, own.inner.decision_v73().unwrap().expected())
                    .unwrap(),
            );
        }
        assert_eq!(projected[0], projected[1]);
        let hart = projected[0]
            .actions
            .iter()
            .filter(|row| row.label.starts_with("Activate Burnished Hart"))
            .collect::<Vec<_>>();
        assert_eq!(hart.len(), 1);
        assert!(hart[0].label.contains("pay {3}; sacrifice Burnished Hart"));
        assert!(hart[0].label.contains("up to two basic land cards"));
        assert!(hart[0].label.contains("battlefield tapped, then shuffle"));
        let (s, id) = owning_ready(p, false, "Island");
        let mut wide = vec![];
        for count in [i32::MAX - 1, i32::MAX] {
            let mut world = s.clone();
            let c = &mut world.objects.get_mut(id).counters;
            c.stash = count;
            c.fellowship = i32::MAX;
            c.charge = i32::MAX;
            let own = HumanLilianaSessionV73::from_session(
                FastActorSessionV1::from_liliana_v73_fixture_state(world),
                p.into(),
            )
            .unwrap();
            let d = HumanLilianaProjectorV73::new(p.into())
                .project_current(&own, own.inner.decision_v73().unwrap().expected())
                .unwrap();
            let current = d.state.liliana.battlefield[p.index()]
                .iter()
                .find(|c| c.card.name == "Liliana, Dreadhorde General")
                .unwrap();
            assert_eq!(current.counters.stash, count);
            assert_eq!(d.state.liliana.battlefield, d.state.tinybones.battlefield);
            wide.push(d);
        }
        assert_ne!(wide[0], wide[1]);
    }
}
#[test]
fn genuine_paid_human_sourcegone_and_concealed_original_stay_distinct_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for concealed in [false, true] {
            let (s,_,_)=crate::effect::liliana_dreadhorde_v1::tests::owner_v73_tests::published_source_boundary(p,concealed);
            let mut native = FastActorSessionV1::from_liliana_v73_fixture_state(s.clone());
            let mut own = HumanLilianaSessionV73::from_session(
                FastActorSessionV1::from_liliana_v73_fixture_state(s),
                p.into(),
            )
            .unwrap();
            let mut h = HumanLilianaProjectorV73::new(p.into());
            let d = own.inner.decision_v73().unwrap();
            let (visible, _, _) = project_with_handles(&d, d.expected().acting_player, 1).unwrap();
            let source = &visible.state.liliana.activations[0].source;
            assert_eq!(source.original_source.name, "Liliana, Dreadhorde General");
            assert_eq!(source.captured_controller, p.into());
            if concealed {
                assert_eq!(
                    source.current_original_source.as_ref().unwrap().name,
                    "Face-down card"
                );
            } else {
                assert!(source.current_original_source.is_none());
            }
            complete(&mut own, &mut h, &mut native);
            let d = own.inner.decision_v73().unwrap();
            let (visible, _, _) = project_with_handles(&d, d.expected().acting_player, 1).unwrap();
            assert_eq!(
                visible.state.liliana.activations[0].created_zombies[0].controller,
                p.into()
            );
        }
    }
}
#[test]
fn genuine_paid_human_prebatch_token_animated_self_sourcegone_and_private_hand_roles_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let s =
            crate::effect::liliana_dreadhorde_v1::tests::owner_v73_tests::observer_boundary_ready(
                p,
            );
        let own = HumanLilianaSessionV73::from_session(
            FastActorSessionV1::from_liliana_v73_fixture_state(s),
            p.opponent().into(),
        )
        .unwrap();
        let d = own.inner.decision_v73().unwrap();
        let (visible, _, _) = project_with_handles(&d, d.expected().acting_player, 1).unwrap();
        let f = &visible.state.liliana;
        assert_eq!(f.deaths.len(), 3);
        assert_eq!(f.current_draws.len(), 3);
        assert!(f
            .deaths
            .iter()
            .all(|d| d.collected && d.observer.current_original_source.is_none()));
        assert!(!f
            .authorized_printings
            .iter()
            .any(|r| r.zone == Zone::Hand && r.owner == p.into()));
        let v = serde_json::to_value(f).unwrap();
        assert_eq!(v["deaths"][0]["creature"].as_object().unwrap().len(), 5);
        assert!(v["deaths"][0]["creature"].get("name").is_none());
        assert!(f
            .deaths
            .iter()
            .any(|d| d.creature == d.observer.original_source_relation));
        let source = f.deaths[0].observer.original_source.clone();
        assert!(f
            .current_draws
            .iter()
            .all(|r| r.source.original_source == source));
    }
}
