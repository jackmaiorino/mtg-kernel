//! Paid native fixture streams; no public cancellation or Partial reset admission.
use super::*;
use crate::{
    engine::tinybones_v72::tests::complete_observation::owning_ready, ids::PlayerId,
    rl::ActionSemanticV1 as A, state::Zone,
};
fn tapped(s: &crate::state::GameState, p: PlayerId) -> usize {
    s.players[p.index()]
        .battlefield
        .iter()
        .filter(|id| {
            let o = s.objects.get(**id);
            o.tapped && crate::engine::object_has_type(s, **id, crate::card_def::CardType::Land)
        })
        .count()
}
fn quiet(a: &A) -> bool {
    matches!(
        a,
        A::Pass { .. }
            | A::OrderTriggers { .. }
            | A::ChooseTriggerOrderNext { .. }
            | A::ChooseAttackerInclusion { include: false, .. }
            | A::ChooseBlockerInclusion { include: false, .. }
    )
}
fn act(
    own: &mut HumanTinybonesSessionV72,
    h: &mut HumanTinybonesProjectorV72,
    native: &mut FastActorSessionV1,
    choose: impl Fn(&A) -> bool,
) {
    let d = own.inner.decision_v72().unwrap();
    let e = d.expected();
    let i = d.actions().iter().position(choose).unwrap();
    let original = d.fixture_original_index_v72(i as u32).unwrap();
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
        let after = own.inner.fixture_state_v72().clone();
        assert_eq!(h.submit(own, request).unwrap(), receipt);
        assert_eq!(own.inner.fixture_state_v72(), &after);
    } else {
        own.step_opponent_with(e, |_, _| Ok(i as u32)).unwrap();
    }
    native.step(e.episode_id, e.step, original).unwrap();
    assert_eq!(
        own.inner.fixture_state_v72(),
        native.tinybones_fixture_state()
    );
    assert_eq!(
        own.inner.fixture_counts_v72(),
        (native.policy_step_count(), native.physical_decision_count())
    );
}
#[test]
fn genuine_paid_human_activation_discard_stash_restore_retries_and_original_indices_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (world, tiny, card) = owning_ready(p, "Forest");
        let mut native = FastActorSessionV1::from_tinybones_v72_fixture_state(world.clone());
        let mut own = HumanTinybonesSessionV72::from_session(
            FastActorSessionV1::from_tinybones_v72_fixture_state(world),
            p.into(),
        )
        .unwrap();
        let mut h = HumanTinybonesProjectorV72::new(p.into());
        let e = own.inner.decision_v72().unwrap().expected();
        let visible = h.project_current(&own, e).unwrap();
        assert!(visible
            .actions
            .iter()
            .any(|a| a.label.contains("pay {3}{B}, tap")));
        let saved = own.snapshot_v72();
        let raw = native.snapshot_v1();
        let old = HumanActionRequestV1 {
            prompt_seq: visible.prompt_seq,
            action_index: 0,
        };
        own.restore_v72(&saved).unwrap();
        native.tinybones_restore_v72(&raw).unwrap();
        let before = own.inner.fixture_state_v72().clone();
        assert!(h.submit(&mut own, old).is_err());
        assert_eq!(own.inner.fixture_state_v72(), &before);
        let taps = tapped(&before, p);
        act(
            &mut own,
            &mut h,
            &mut native,
            |a| matches!(a,A::ActivateAbility{source,ability_index:0,..}if source.arena_id==tiny.0),
        );
        for _ in 0..80 {
            if own.inner.fixture_state_v72().objects.get(card).zone == Zone::Exile {
                break;
            }
            act(&mut own, &mut h, &mut native, |a| {
                quiet(a)
                    || matches!(a,A::Discard{cards,..}if cards.iter().any(|r|r.arena_id==card.0))
            });
        }
        let s = own.inner.fixture_state_v72();
        assert_eq!(s.objects.get(card).zone, Zone::Exile);
        assert_eq!(s.objects.get(card).counters.stash, 1);
        assert!(s.objects.get(tiny).tapped);
        assert_eq!(tapped(s, p) - taps, 4);
        let d = own.inner.decision_v72().unwrap();
        let (visible, _, _) = project_with_handles(&d, d.expected().acting_player, 1).unwrap();
        assert_eq!(
            visible.state.tinybones.discards[0].original_card.owner,
            p.opponent().into()
        );
        assert_eq!(
            visible.state.tinybones.discards[0]
                .original_card
                .object_role,
            visible.state.tinybones.triggers[0]
                .stashed
                .as_ref()
                .unwrap()
                .object_role
        );
        assert_ne!(
            visible.state.tinybones.discards[0]
                .original_card
                .incarnation_role,
            visible.state.tinybones.triggers[0]
                .stashed
                .as_ref()
                .unwrap()
                .incarnation_role
        );
        let value = serde_json::to_value(&visible).unwrap();
        assert_eq!(
            value["state"]["tinybones"]["discards"][0]["original_card"]
                .as_object()
                .unwrap()
                .len(),
            5
        );
        let before_cast = tapped(own.inner.fixture_state_v72(), p);
        act(
            &mut own,
            &mut h,
            &mut native,
            |a| matches!(a,A::CastSpell{source,..}if source.arena_id==card.0),
        );
        for _ in 0..24 {
            let d = own.inner.decision_v72().unwrap();
            if d.observation()
                .tinybones
                .plays
                .last()
                .is_some_and(|r| r.stage == crate::tinybones_public_v72::PlayStageV72::Paid)
            {
                break;
            }
            act(&mut own, &mut h, &mut native, quiet);
        }
        let paid = own.inner.decision_v72().unwrap();
        let play = paid.observation().tinybones.plays.last().unwrap();
        assert_eq!(play.stage, crate::tinybones_public_v72::PlayStageV72::Paid);
        assert_eq!(play.original_card.owner, p.opponent().into());
        assert_eq!(play.permission_source.captured_controller, p.into());
        assert_eq!(play.paid_form, Some(crate::state::CastMethodV4::Normal));
        assert_eq!(play.paid_x, Some(0));
        assert_eq!(
            own.inner.fixture_state_v72().objects.get(card).zone,
            Zone::Stack
        );
        assert_eq!(
            own.inner.fixture_state_v72().objects.get(card).owner,
            p.opponent()
        );
        assert_eq!(tapped(own.inner.fixture_state_v72(), p) - before_cast, 2);
        let text = value.to_string();
        for private in [
            "arena_id",
            "zone_change_count",
            "original_library",
            "private_world",
            "history_index",
            "native_stack_item_id",
        ] {
            assert!(!text.contains(private));
        }
    }
}
#[test]
fn genuine_paid_human_foreign_and_modified_prompt_refusals_are_readonly_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (world, _, _) = owning_ready(p, "Forest");
        let mut a = HumanTinybonesSessionV72::from_session(
            FastActorSessionV1::from_tinybones_v72_fixture_state(world.clone()),
            p.into(),
        )
        .unwrap();
        let b = HumanTinybonesSessionV72::from_session(
            FastActorSessionV1::from_tinybones_v72_fixture_state(world),
            p.into(),
        )
        .unwrap();
        let mut h = HumanTinybonesProjectorV72::new(p.into());
        let d = h
            .project_current(&a, a.inner.decision_v72().unwrap().expected())
            .unwrap();
        let before = a.inner.fixture_state_v72().clone();
        assert!(a.restore_v72(&b.snapshot_v72()).is_err());
        h.pending
            .as_mut()
            .unwrap()
            .visible
            .state
            .tinybones
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
        assert_eq!(a.inner.fixture_state_v72(), &before);
    }
}
#[test]
fn independent_prepaid_human_hidden_worlds_and_full_width_tinybones_counters_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut rows = vec![];
        for hidden in ["Forest", "Imprisoned in the Moon"] {
            let (world, _, _) = owning_ready(p, hidden);
            let own = HumanTinybonesSessionV72::from_session(
                FastActorSessionV1::from_tinybones_v72_fixture_state(world),
                p.into(),
            )
            .unwrap();
            rows.push(
                HumanTinybonesProjectorV72::new(p.into())
                    .project_current(&own, own.inner.decision_v72().unwrap().expected())
                    .unwrap(),
            );
        }
        assert_eq!(rows[0], rows[1]);
        let (world, tiny, _) = owning_ready(p, "Forest");
        let mut values = vec![];
        for count in [i32::MAX - 1, i32::MAX] {
            let mut s = world.clone();
            s.objects.get_mut(tiny).counters.stash = count;
            let own = HumanTinybonesSessionV72::from_session(
                FastActorSessionV1::from_tinybones_v72_fixture_state(s),
                p.into(),
            )
            .unwrap();
            let d = HumanTinybonesProjectorV72::new(p.into())
                .project_current(&own, own.inner.decision_v72().unwrap().expected())
                .unwrap();
            assert_eq!(
                d.state.tinybones.battlefield[p.index()]
                    .iter()
                    .find(|r| r.card.name == "Tinybones, Bauble Burglar")
                    .unwrap()
                    .counters
                    .stash,
                count
            );
            values.push(d);
        }
        assert_ne!(values[0], values[1]);
    }
}
