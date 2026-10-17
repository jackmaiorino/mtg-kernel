//! Genuine paid native source streams. Rust fixtures remain uncompiled/unrun.
use super::*;
use crate::{
    chandra_public_v74::ActivationStageV74,
    effect::chandra_flameshaper_v1::tests::owner_v74_tests::{
        owning_ready, pending_damage_boundary, published_source_boundary,
    },
    ids::{ObjectId, PlayerId},
    rl::ActionSemanticV1 as A,
    state::{GameState, Zone},
};
struct Pair {
    own: HumanChandraSessionV74,
    human: HumanChandraProjectorV74,
    native: FastActorSessionV1,
}
impl Pair {
    fn new(s: GameState, human: PlayerId) -> Self {
        let native = FastActorSessionV1::from_chandra_v74_fixture_state(s.clone());
        let own = HumanChandraSessionV74::from_session(
            FastActorSessionV1::from_chandra_v74_fixture_state(s),
            human.into(),
        )
        .unwrap();
        Self {
            own,
            human: HumanChandraProjectorV74::new(human.into()),
            native,
        }
    }
    fn visible(&self) -> HumanDecisionChandraV74 {
        let d = self.own.inner.decision_v74().unwrap();
        project_with_handles(&d, d.expected().acting_player, 1)
            .unwrap()
            .0
    }
    fn act(&mut self, choose: impl Fn(&A) -> bool) {
        let d = self.own.inner.decision_v74().unwrap();
        let e = d.expected();
        let i = d
            .actions()
            .iter()
            .position(choose)
            .unwrap_or_else(|| panic!("missing paid action {:?}", d.actions()));
        let original = d.fixture_original_index_v74(i as u32).unwrap();
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
            let after = self.own.inner.fixture_state_v74().clone();
            assert_eq!(self.human.submit(&mut self.own, request).unwrap(), receipt);
            assert_eq!(self.own.inner.fixture_state_v74(), &after);
        } else {
            self.own.step_opponent_with(e, |_, _| Ok(i as u32)).unwrap();
        }
        self.native.step(e.episode_id, e.step, original).unwrap();
        assert_eq!(
            self.own.inner.fixture_state_v74(),
            self.native.chandra_fixture_state()
        );
        assert_eq!(
            self.own.inner.fixture_counts_v74(),
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
        let d = self.own.inner.decision_v74().unwrap();
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
                .chandra
                .activations
                .last()
                .is_some_and(|a| a.stage == ActivationStageV74::NativeCompleted)
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
            .project_current(&self.own, self.own.inner.decision_v74().unwrap().expected())
            .unwrap();
        let saved = self.own.snapshot_v74();
        let raw = self.native.snapshot_v1();
        self.complete();
        let expected = self.own.inner.fixture_state_v74().clone();
        self.own.restore_v74(&saved).unwrap();
        self.native.chandra_restore_v74(&raw).unwrap();
        let before = self.own.inner.fixture_state_v74().clone();
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
        assert_eq!(self.own.inner.fixture_state_v74(), &before);
        self.complete();
        assert_eq!(self.own.inner.fixture_state_v74(), &expected);
    }
}
#[test]
fn genuine_paid_human_three_roots_exact_loyalty_season_haste_zero_targets_restore_native_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for index in [0, 1, 2] {
            let (s, id, ours, _) = owning_ready(p, index == 1, "Island");
            let mut pair = Pair::new(s, p);
            let before =
                crate::planeswalker_v1::loyalty(pair.own.inner.fixture_state_v74(), id).unwrap();
            let v = pair.visible();
            assert!(v
                .actions
                .iter()
                .any(|a| a.label.contains("loyalty") && a.label.contains("Chandra")));
            pair.activate(id, index);
            if index == 1 {
                pair.target(ours);
            } else if index == 2 {
                pair.act(|a| matches!(a, A::FinishEffectSelection { .. }));
            }
            pair.restore_replay();
            let v = pair.visible();
            let f = &v.state.chandra;
            assert_eq!(f.activations[0].ability_index, index);
            assert_eq!(f.activations[0].loyalty_before, before);
            assert_eq!(
                f.activations[0].loyalty_after,
                match index {
                    0 => before + 2,
                    1 => before + 1,
                    2 => before - 4,
                    _ => unreachable!(),
                }
            );
            assert_eq!(f.battlefield, v.state.tinybones.battlefield);
            if index == 0 {
                assert_eq!(f.exiles[0].cards.len(), 3);
                assert!(f.exiles[0].permission_present);
            }
            if index == 1 {
                assert_eq!(f.copied_tokens.len(), 2);
                assert!(f
                    .copied_tokens
                    .iter()
                    .all(|t| t.current_haste == Some(true) && t.sacrifice_instances == 1));
            }
            if index == 2 {
                assert!(f.activations[0].targets.is_empty());
            }
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
}
#[test]
fn genuine_paid_human_eight_units_two_and_eight_original_targets_before_payment_and_target_loss_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for eight in [false, true] {
            let (mut s, id, ours, theirs) = owning_ready(p, false, "Island");
            // Labelled native target roster primitive; Chandra itself was paid normally.
            let targets = if eight {
                (0..8)
                    .map(|_| {
                        crate::policy_observation_v6::tests::put(
                            &mut s,
                            p.opponent(),
                            "Axgard Cavalry",
                            Zone::Battlefield,
                        )
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![ours, theirs]
            };
            let mut pair = Pair::new(s, p);
            pair.activate(id, 2);
            for n in 0..8 {
                let v = pair.visible();
                let a = v.state.chandra.pending_allocation.as_ref().unwrap();
                assert_eq!((a.remaining_units, a.loyalty_before), (8 - n, 6));
                let target = if eight {
                    targets[n as usize]
                } else {
                    targets[usize::from(n >= 3)]
                };
                pair.target(target);
            }
            let v = pair.visible();
            let a = &v.state.chandra.activations[0];
            assert_eq!(a.targets.len(), if eight { 8 } else { 2 });
            assert_eq!(
                a.targets
                    .iter()
                    .map(|t| t.allocated_damage.unwrap())
                    .sum::<u8>(),
                8
            );
            assert_eq!(
                a.targets
                    .iter()
                    .map(|t| t.allocated_damage.unwrap())
                    .collect::<Vec<_>>(),
                if eight { vec![1; 8] } else { vec![3, 5] }
            );
            pair.restore_replay();
        }
    }
    // Genuine published division followed by explicitly labelled source and target departure.
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, id, ours, theirs) = owning_ready(p, false, "Island");
        let mut pair = Pair::new(s, p);
        pair.activate(id, 2);
        for n in 0..8 {
            pair.target(if n < 3 { ours } else { theirs });
        }
        let mut s = pair.own.inner.fixture_state_v74().clone();
        crate::event::propose_and_commit(
            &mut s,
            crate::event::ProposedEvent::zone_change(id, Zone::Graveyard),
        );
        crate::event::propose_and_commit(
            &mut s,
            crate::event::ProposedEvent::zone_change(ours, Zone::Hand),
        );
        let mut pair = Pair::new(s, p);
        let v = pair.visible();
        let a = &v.state.chandra.activations[0];
        assert!(a.source.current_original_source.is_none());
        assert!(!a.targets[0].legal_now);
        assert!(a.targets[0].current_original.is_none());
        assert_eq!(a.targets[0].allocated_damage, Some(3));
        pair.complete();
        let v = pair.visible();
        let d = v.state.chandra.activations[0]
            .damage_source
            .as_ref()
            .unwrap();
        assert!(d.historical);
        assert_eq!(d.source.original_source.name, "Chandra, Flameshaper");
    }
}
#[test]
fn genuine_paid_human_exile_original_selection_ordinary_spell_payment_land_quota_and_expiry_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for land in [false, true] {
            let (s, id, _, _) = owning_ready(p, false, "Island");
            let mut pair = Pair::new(s, p);
            pair.activate(id, 0);
            for _ in 0..64 {
                if pair
                    .visible()
                    .state
                    .chandra
                    .exiles
                    .iter()
                    .any(|e| e.choice_current)
                {
                    break;
                }
                pair.quiet();
            }
            let d = pair.own.inner.decision_v74().unwrap();
            let e = &d.observation().chandra.exiles[0];
            let chosen = usize::from(!land);
            let original = ObjectId(e.cards[chosen].original.object);
            let v = pair.visible();
            assert!(v
                .action_meanings
                .iter()
                .any(|r| r.chandra.selected_exile == Some(chosen as u16)));
            pair.target(original);
            pair.complete();
            let f = pair.visible().state.chandra;
            assert_eq!(f.exiles[0].chosen, Some(chosen as u16));
            assert_eq!(f.exiles[0].grant_for_land, Some(land));
            assert!(f.exiles[0].permission_present);
            for _ in 0..64 {
                let d = pair.own.inner.decision_v74().unwrap();
                if d.actions().iter().any(|a|matches!(a,A::PlayLand{source:card,..}|A::CastSpell{source:card,..}if card.arena_id==original.0)){break;}
                pair.quiet();
            }
            let before =
                crate::engine::time_stop_v1::tests::tapped(pair.own.inner.fixture_state_v74(), p);
            pair.act(|a| {
                matches!(a,A::PlayLand{source:card,..}if land&&card.arena_id==original.0)
                    || matches!(a,A::CastSpell{source:card,..}if !land&&card.arena_id==original.0)
            });
            if land {
                assert_eq!(
                    pair.own.inner.fixture_state_v74().players[p.index()].lands_played_this_turn,
                    1
                );
            } else {
                for _ in 0..32 {
                    if pair
                        .own
                        .inner
                        .fixture_state_v74()
                        .engine
                        .pending_cast
                        .is_none()
                    {
                        break;
                    }
                    pair.quiet();
                }
                assert_eq!(
                    crate::engine::time_stop_v1::tests::tapped(
                        pair.own.inner.fixture_state_v74(),
                        p
                    ) - before,
                    1
                );
            }
            let turn = pair.visible().state.chandra.turn_key;
            for _ in 0..512 {
                if pair.visible().state.chandra.turn_key != turn {
                    break;
                }
                pair.quiet();
            }
            let f = pair.visible().state.chandra;
            assert!(f.exiles[0].expired && !f.exiles[0].permission_present);
        }
    }
}
#[test]
fn genuine_paid_human_crystal_options_original_menu_sort_restore_and_native_damage_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for double_first in [false, true] {
            let s = pending_damage_boundary(p);
            let mut pair = Pair::new(s, p.opponent());
            let v = pair.visible();
            let damage = v.state.chandra.pending_damage.as_ref().unwrap();
            assert_eq!(damage.chooser, p.opponent().into());
            assert!(damage.options.iter().any(|o| o.kind
                == crate::chandra_public_v74::PreventionKindV74::Shield
                && o.shield_remaining == Some(3)));
            let selected = damage
                .options
                .iter()
                .find(|o| {
                    o.kind
                        == if double_first {
                            crate::chandra_public_v74::PreventionKindV74::Multiplier
                        } else {
                            crate::chandra_public_v74::PreventionKindV74::Shield
                        }
                })
                .unwrap()
                .menu_index;
            assert_eq!(
                v.action_meanings
                    .iter()
                    .filter_map(|r| r.chandra.selected_damage_option)
                    .collect::<std::collections::BTreeSet<_>>(),
                damage.options.iter().map(|o| o.menu_index).collect()
            );
            let saved = pair.own.snapshot_v74();
            let raw = pair.native.snapshot_v1();
            for replay in [false, true] {
                if replay {
                    pair.own.restore_v74(&saved).unwrap();
                    pair.native.chandra_restore_v74(&raw).unwrap();
                }
                pair.act(|a|matches!(a,A::ChooseEffectOption{option_index,..}if *option_index==selected));
                for _ in 0..16 {
                    if pair.visible().state.chandra.pending_damage.is_none() {
                        break;
                    }
                    pair.act(|a| {
                        matches!(
                            a,
                            A::ChooseEffectOption {
                                option_index: 0,
                                ..
                            }
                        )
                    });
                }
                pair.complete();
                let state = pair.own.inner.fixture_state_v74();
                let amounts = state
                    .engine
                    .event_history
                    .iter()
                    .filter_map(|e| match e {
                        crate::event::CommittedEvent::Damage { amount, source, .. }
                            if state.objects.get(*source).card_def
                                == crate::card_def::card_id_by_name("Chandra, Flameshaper")
                                    .unwrap() =>
                        {
                            Some(*amount)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(amounts, vec![if double_first { 13 } else { 10 }]);
            }
        }
    }
}
#[test]
fn genuine_paid_human_sourcegone_face_down_private_world_equality_and_foreign_owner_refusal_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for concealed in [false, true] {
            let (s, _, _) = published_source_boundary(p, concealed);
            let mut pair = Pair::new(s, p);
            let v = pair.visible();
            let source = &v.state.chandra.activations[0].source;
            assert_eq!(source.original_source.name, "Chandra, Flameshaper");
            assert_eq!(source.captured_controller, p.into());
            if concealed {
                assert_eq!(
                    source.current_original_source.as_ref().unwrap().name,
                    "Face-down card"
                );
            } else {
                assert!(source.current_original_source.is_none());
            }
            pair.complete();
            assert_eq!(pair.visible().state.chandra.exiles[0].cards.len(), 3);
        }
        let mut visible = vec![];
        for hidden in ["Island", "Imprisoned in the Moon"] {
            let (s, _, _, _) = owning_ready(p, false, hidden);
            visible.push(Pair::new(s, p).visible());
        }
        assert_eq!(visible[0], visible[1]);
        let (s, _, _, _) = owning_ready(p, false, "Island");
        let mut pair = Pair::new(s.clone(), p);
        let foreign = Pair::new(s, p);
        let e = pair.own.inner.decision_v74().unwrap().expected();
        let v = pair.human.project_current(&pair.own, e).unwrap();
        let before = pair.own.inner.fixture_state_v74().clone();
        assert!(pair.own.restore_v74(&foreign.own.snapshot_v74()).is_err());
        assert!(pair.own.step_opponent_with(e, |_, _| Ok(0)).is_err());
        pair.human
            .pending
            .as_mut()
            .unwrap()
            .visible
            .state
            .chandra
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
        assert_eq!(pair.own.inner.fixture_state_v74(), &before);
        assert!(crate::rl_session::LilianaSessionV73::from_session(
            pair.own.inner.fixture_native_clone_v74()
        )
        .is_err());
        let mut later = before;
        crate::policy_observation_v6::tests::put(
            &mut later,
            p,
            "Kaito, Cunning Infiltrator",
            Zone::Battlefield,
        );
        assert!(HumanChandraSessionV74::from_session(
            FastActorSessionV1::from_chandra_v74_fixture_state(later),
            p.into()
        )
        .is_err());
    }
}
#[test]
fn genuine_paid_human_copiable_season_tokens_copy_of_copy_and_endstep_anonymous_observers_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, id, ours, _) = owning_ready(p, true, "Island");
        let mut pair = Pair::new(s, p);
        pair.activate(id, 1);
        pair.target(ours);
        pair.complete();
        let first = pair
            .own
            .inner
            .decision_v74()
            .unwrap()
            .observation()
            .chandra
            .copied_tokens[0]
            .source
            .original_source
            .object;
        let mut s = pair.own.inner.fixture_state_v74().clone();
        // Genuine six-mana Self-Reflection, not a synthetic copy receipt.
        crate::engine::time_stop_v1::tests::cast(
            &mut s,
            p,
            "Self-Reflection",
            &[crate::state::Target::Object(ObjectId(first))],
            6,
        );
        crate::engine::time_stop_v1::tests::finish(&mut s);
        let mut pair = Pair::new(s, p);
        let f = pair.visible().state.chandra;
        assert_eq!(f.copied_tokens.len(), 4);
        assert!(f
            .copied_tokens
            .iter()
            .all(|t| t.sacrifice_instances == 1 && t.current_haste == Some(true)));
        for _ in 0..512 {
            let f = pair.visible().state.chandra;
            if f.copied_tokens.iter().all(|t| t.current_haste.is_none()) {
                break;
            }
            pair.quiet();
        }
        let f = pair.visible().state.chandra;
        assert!(f.copied_tokens.iter().all(|t| t.current_haste.is_none()));
        assert_eq!(f.end_steps.iter().map(|e| e.finished_count).sum::<u32>(), 4);
        for e in &f.end_steps {
            assert!(e.collected);
            assert_eq!(e.finished_count, e.published_count);
            for observer in &e.observers {
                let token = &f.copied_tokens[observer.token as usize];
                assert_eq!(
                    observer.source.original_source_relation.object_role,
                    token.source.original_source_relation.object_role
                );
                assert_eq!(
                    observer.source.original_source_relation.incarnation_role,
                    token.source.original_source_relation.incarnation_role
                );
            }
        }
    }
}
