//! Genuine paid native source streams. Rust fixtures remain uncompiled/unrun.
use super::*;
use crate::{
    effect::kaito_cunning_v75::tests::owner_v75_tests::{owning_ready, published_source_boundary},
    ids::{ObjectId, PlayerId},
    kaito_public_v75::StageV75,
    rl::ActionSemanticV1 as A,
    state::{GameState, Zone},
};
struct Pair {
    own: HumanKaitoSessionV75,
    human: HumanKaitoProjectorV75,
    native: FastActorSessionV1,
}
impl Pair {
    fn new(s: GameState, human: PlayerId) -> Self {
        let native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
        let own = HumanKaitoSessionV75::from_session(
            FastActorSessionV1::from_kaito_v75_fixture_state(s),
            human.into(),
        )
        .unwrap();
        Self {
            own,
            human: HumanKaitoProjectorV75::new(human.into()),
            native,
        }
    }
    fn visible(&self) -> HumanDecisionChandraV75 {
        let d = self.own.inner.decision_v75().unwrap();
        project_with_handles(&d, d.expected().acting_player, 1)
            .unwrap()
            .0
    }
    fn act(&mut self, choose: impl Fn(&A) -> bool) {
        let d = self.own.inner.decision_v75().unwrap();
        let e = d.expected();
        let i = d
            .actions()
            .iter()
            .position(choose)
            .unwrap_or_else(|| panic!("missing paid action {:?}", d.actions()));
        let original = d.fixture_original_index_v75(i as u32).unwrap();
        if e.acting_player == self.own.human_seat {
            let v = self.human.project_current(&self.own, e).unwrap();
            let display = self
                .human
                .pending
                .as_ref()
                .unwrap()
                .indexes
                .iter()
                .position(|x| *x == i as u32)
                .unwrap();
            let request = HumanActionRequestV1 {
                prompt_seq: v.prompt_seq,
                action_index: display as u32,
            };
            let receipt = self.human.submit(&mut self.own, request.clone()).unwrap();
            let after = self.own.inner.fixture_state_v75().clone();
            assert_eq!(self.human.submit(&mut self.own, request).unwrap(), receipt);
            assert_eq!(self.own.inner.fixture_state_v75(), &after);
        } else {
            self.own.step_opponent_with(e, |_, _| Ok(i as u32)).unwrap();
        }
        self.native.step(e.episode_id, e.step, original).unwrap();
        assert_eq!(
            self.own.inner.fixture_state_v75(),
            self.native.kaito_fixture_state()
        );
        assert_eq!(
            self.own.inner.fixture_counts_v75(),
            (
                self.native.policy_step_count(),
                self.native.physical_decision_count()
            )
        );
    }
    fn activate(&mut self, id: ObjectId, index: u8) {
        self.act(|a|matches!(a,A::ActivateAbility{source,ability_index,..}if source.arena_id==id.0&&*ability_index==index));
    }
    fn target(&mut self, id: ObjectId) {
        self.act(|a|matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}|A::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==id.0));
    }
    fn quiet(&mut self) {
        let d = self.own.inner.decision_v75().unwrap();
        let finish = d
            .actions()
            .iter()
            .any(|a| matches!(a, A::FinishEffectSelection { .. }));
        self.act(|a| {
            if finish {
                matches!(a, A::FinishEffectSelection { .. })
            } else {
                matches!(
                    a,
                    A::Pass { .. }
                        | A::OrderTriggers { .. }
                        | A::ChooseTriggerOrderNext { .. }
                        | A::ChooseEffectTarget { .. }
                        | A::ChooseTarget { .. }
                        | A::Discard { .. }
                        | A::ChooseAttackerInclusion { include: false, .. }
                        | A::ChooseBlockerInclusion { include: false, .. }
                )
            }
        });
    }
    fn complete(&mut self) {
        for _ in 0..512 {
            if self
                .visible()
                .state
                .kaito
                .activations
                .last()
                .is_some_and(|a| a.stage == StageV75::Resolved)
            {
                return;
            }
            self.quiet();
        }
        panic!("paid Chandra root did not complete")
    }
    fn restore_replay(&mut self) {
        let old = self
            .human
            .project_current(&self.own, self.own.inner.decision_v75().unwrap().expected())
            .unwrap();
        let saved = self.own.snapshot_v75();
        let raw = self.native.snapshot_v1();
        self.complete();
        let expected = self.own.inner.fixture_state_v75().clone();
        self.own.restore_v75(&saved).unwrap();
        self.native.kaito_restore_v75(&raw).unwrap();
        let before = self.own.inner.fixture_state_v75().clone();
        assert!(self
            .human
            .submit(
                &mut self.own,
                HumanActionRequestV1 {
                    prompt_seq: old.prompt_seq,
                    action_index: 0
                }
            )
            .is_err());
        assert_eq!(self.own.inner.fixture_state_v75(), &before);
        self.complete();
        assert_eq!(self.own.inner.fixture_state_v75(), &expected);
    }
    fn settle(&mut self) {
        for _ in 0..512 {
            let s = self.own.inner.fixture_state_v75();
            if s.stack.is_empty()
                && s.engine.pending_effect.is_none()
                && s.engine.pending_discard.is_none()
                && s.engine.pending_triggers.is_empty()
            {
                return;
            }
            self.quiet();
        }
        panic!("mandatory Kaito work did not settle")
    }
}

#[test]
fn genuine_paid_human_kaito_optional_target_private_draw_discard_native_indices_and_restore_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for selected in [false, true] {
            let (s, k, c) = owning_ready(p, false, "Island");
            let mut pair = Pair::new(s, p);
            pair.activate(k, 0);
            if selected {
                let v = pair.visible();
                assert!(v
                    .action_meanings
                    .iter()
                    .any(|r| r.kaito.selected_target.is_some()));
                pair.target(c);
            } else {
                pair.act(|a| matches!(a, A::FinishEffectSelection { .. }));
            }
            for _ in 0..64 {
                if pair.visible().state.kaito.activations[0].stage == StageV75::WaitingDiscard {
                    break;
                }
                pair.quiet();
            }
            let v = pair.visible();
            let a = &v.state.kaito.activations[0];
            assert_eq!((a.loyalty_before, a.loyalty_after), (3, 4));
            assert_eq!(a.targets.len(), usize::from(selected));
            assert_eq!(a.draw, crate::kaito_public_v75::DrawV75::Drawn);
            assert_eq!(a.evasion.as_ref().unwrap().active_on_original, selected);
            let choices = a
                .discard
                .as_ref()
                .unwrap()
                .private_choices
                .as_ref()
                .unwrap();
            assert!(!choices.is_empty());
            assert!(v.action_meanings.iter().all(|r| r
                .kaito
                .selected_discard
                .as_ref()
                .is_some_and(|rs| rs.len() == 1 && choices.contains(&rs[0]))));
            pair.restore_replay();
            let f = pair.visible().state.kaito;
            let a = &f.activations[0];
            assert!(a.discard.as_ref().unwrap().answered);
            assert_eq!(
                a.discard
                    .as_ref()
                    .unwrap()
                    .private_chosen
                    .as_ref()
                    .unwrap()
                    .len(),
                1
            );
            for _ in 0..4 {
                if pair
                    .own
                    .inner
                    .decision_v75()
                    .unwrap()
                    .expected()
                    .acting_player
                    == p.opponent().into()
                {
                    break;
                }
                pair.act(|a| matches!(a, A::Pass { .. }));
            }
            assert_eq!(
                pair.own
                    .inner
                    .decision_v75()
                    .unwrap()
                    .expected()
                    .acting_player,
                p.opponent().into()
            );
            let v = pair.visible();
            let private = v.state.kaito.activations[0].discard.as_ref().unwrap();
            assert!(private.private_choices.is_none() && private.private_chosen.is_none());
            let turn = v.state.kaito.turn_key;
            for _ in 0..512 {
                if pair.visible().state.kaito.turn_key != turn {
                    break;
                }
                pair.quiet();
            }
            let f = pair.visible().state.kaito;
            assert_ne!(f.turn_key, turn);
            assert!(
                !f.activations[0]
                    .evasion
                    .as_ref()
                    .unwrap()
                    .active_on_original
            );
        }
    }
}
#[test]
fn genuine_paid_human_kaito_ninja_season_ultimate_emblem_loyalty_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for index in [1, 2] {
            for season in [false, true] {
                let (mut s, k, _) = owning_ready(p, season, "Island");
                if index == 2 {
                    crate::effect::kaito_cunning_v75::tests::owner_v75_tests::prepare_ultimate(
                        &mut s, k,
                    );
                }
                // Native storage operands on a genuine paid printing, preserving widths.
                s.objects.get_mut(k).counters.stash = 70_000;
                s.objects.get_mut(k).counters.fellowship = 80_000;
                let before = crate::planeswalker_v1::loyalty(&s, k).unwrap();
                let mut pair = Pair::new(s, p);
                pair.activate(k, index);
                pair.restore_replay();
                let v = pair.visible();
                let f = &v.state.kaito;
                assert_eq!(f.battlefield, v.state.tinybones.battlefield);
                assert_eq!(
                    f.activations[0].loyalty_after,
                    before - if index == 1 { 2 } else { 9 }
                );
                if index == 1 {
                    assert_eq!(f.ninja_batches.len(), 1);
                    assert_eq!(f.ninja_batches[0].created.len(), if season { 2 } else { 1 });
                    assert!(f.ninja_batches[0]
                        .created
                        .iter()
                        .all(|n| n.original.name == "Ninja Token"));
                    let current = f.battlefield[p.index()]
                        .iter()
                        .find(|r| r.card.name == "Kaito, Cunning Infiltrator")
                        .unwrap();
                    assert_eq!(
                        (current.counters.stash, current.counters.fellowship),
                        (70_000, 80_000)
                    );
                } else {
                    assert_eq!(f.emblems.len(), 1);
                    assert_eq!(f.emblems[0].controller, p.into());
                    assert_eq!(
                        f.activations[0].source.current_original_source.is_none(),
                        before == 9
                    );
                }
            }
        }
    }
}
#[test]
fn genuine_paid_human_kaito_prebatch_combat_departure_anonymous_creature_and_mandatory_trigger_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for departing in [false, true] {
            let s = crate::effect::kaito_cunning_v75::tests::owner_v75_tests::combat_boundary(
                p, departing,
            );
            let mut pair = Pair::new(s, p);
            let f = pair.visible().state.kaito;
            assert_eq!(f.combats.len(), 1);
            assert_eq!(f.combats[0].final_damage, 2);
            assert_eq!(f.combats[0].observer.captured_controller, p.into());
            assert_eq!(
                f.triggers[0].origin,
                crate::kaito_public_v75::OriginV75::Combat(0)
            );
            if departing {
                assert!(f.combats[0].observer.current_original_source.is_none());
            }
            let raw = serde_json::to_value(&f).unwrap();
            assert_eq!(
                raw["combats"][0]["original_creature"]
                    .as_object()
                    .unwrap()
                    .len(),
                5
            );
            assert!(raw["combats"][0]["original_creature"].get("name").is_none());
            let saved = pair.own.snapshot_v75();
            let native = pair.native.snapshot_v1();
            pair.settle();
            let expected = pair.own.inner.fixture_state_v75().clone();
            assert_eq!(
                pair.visible().state.kaito.triggers[0].stage,
                StageV75::Resolved
            );
            pair.own.restore_v75(&saved).unwrap();
            pair.native.kaito_restore_v75(&native).unwrap();
            pair.settle();
            assert_eq!(pair.own.inner.fixture_state_v75(), &expected);
        }
    }
}
#[test]
fn genuine_paid_human_kaito_persistent_emblem_both_cast_actors_countered_spell_copy_not_cast_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let s = crate::effect::kaito_cunning_v75::tests::owner_v75_tests::emblem_cast_boundary(p);
        let mut pair = Pair::new(s, p);
        let f = pair.visible().state.kaito;
        assert_eq!(f.emblems.len(), 1);
        assert_eq!(f.emblems[0].controller, p.into());
        assert_eq!(f.casts.len(), 2);
        assert!(f
            .casts
            .iter()
            .any(|c| c.original_spell.controller == p.into()));
        assert!(f
            .casts
            .iter()
            .any(|c| c.original_spell.controller == p.opponent().into()));
        let saved = pair.own.snapshot_v75();
        let native = pair.native.snapshot_v1();
        pair.settle();
        let expected = pair.own.inner.fixture_state_v75().clone();
        let f = pair.visible().state.kaito;
        assert_eq!(f.ninja_batches.len(), 2);
        assert!(f.triggers.iter().all(|t| t.stage == StageV75::Resolved));
        pair.own.restore_v75(&saved).unwrap();
        pair.native.kaito_restore_v75(&native).unwrap();
        pair.settle();
        assert_eq!(pair.own.inner.fixture_state_v75(), &expected);
    }
}
#[test]
fn genuine_paid_human_kaito_original_source_reentry_concealment_and_single_target_fizzle_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for concealed in [false, true] {
            let (s, _, _) = published_source_boundary(p, concealed);
            let mut pair = Pair::new(s, p);
            let v = pair.visible();
            let a = &v.state.kaito.activations[0];
            assert_eq!(a.source.original_source.name, "Kaito, Cunning Infiltrator");
            assert_eq!(a.source.captured_controller, p.into());
            if concealed {
                assert_eq!(
                    a.source.current_original_source.as_ref().unwrap().name,
                    "Face-down card"
                );
            } else {
                assert!(a.source.current_original_source.is_none());
            }
            pair.complete();
            assert_eq!(
                pair.visible().state.kaito.activations[0].stage,
                StageV75::Resolved
            );
        }
        let (s, k, c) = owning_ready(p, false, "Island");
        let mut pair = Pair::new(s, p);
        pair.activate(k, 0);
        pair.target(c);
        let mut s = pair.own.inner.fixture_state_v75().clone();
        // Labelled original-target departure after genuine payment/publication.
        crate::event::propose_and_commit(
            &mut s,
            crate::event::ProposedEvent::zone_change(c, Zone::Hand),
        );
        let mut pair = Pair::new(s, p);
        pair.settle();
        let f = pair.visible().state.kaito;
        let a = &f.activations[0];
        assert_eq!(a.stage, StageV75::Fizzled);
        assert_eq!(a.draw, crate::kaito_public_v75::DrawV75::NotReached);
        assert!(a.discard.is_none());
        assert!(!a.targets[0].legal_now);
    }
}
#[test]
fn independently_paid_human_kaito_private_world_equality_foreign_restore_changed_full_packet_refusal_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut visible = vec![];
        for hidden in ["Island", "Imprisoned in the Moon"] {
            let (s, _, _) = owning_ready(p, false, hidden);
            visible.push(Pair::new(s, p).visible());
        }
        assert_eq!(visible[0], visible[1]);
        let (s, _, _) = owning_ready(p, false, "Island");
        let mut pair = Pair::new(s.clone(), p);
        let foreign = Pair::new(s, p);
        let e = pair.own.inner.decision_v75().unwrap().expected();
        let v = pair.human.project_current(&pair.own, e).unwrap();
        let before = pair.own.inner.fixture_state_v75().clone();
        assert!(pair.own.restore_v75(&foreign.own.snapshot_v75()).is_err());
        assert!(pair.own.step_opponent_with(e, |_, _| Ok(0)).is_err());
        pair.human
            .pending
            .as_mut()
            .unwrap()
            .visible
            .state
            .kaito
            .battlefield[p.index()][0]
            .counters
            .stash = 1;
        assert!(pair
            .human
            .submit(
                &mut pair.own,
                HumanActionRequestV1 {
                    prompt_seq: v.prompt_seq,
                    action_index: 0
                }
            )
            .is_err());
        assert_eq!(pair.own.inner.fixture_state_v75(), &before);
        assert!(crate::rl_session::ChandraSessionV74::from_session(
            pair.own.inner.fixture_native_clone_v75()
        )
        .is_err());
        let text = serde_json::to_string(&v).unwrap();
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
