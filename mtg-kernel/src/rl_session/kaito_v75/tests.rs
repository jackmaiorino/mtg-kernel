//! Paid original printing and independent native-stream parity. Rust unrun.
use super::*;
use crate::{
    effect::kaito_cunning_v75::tests::owner_v75_tests::owning_ready, ids::ObjectId,
    kaito_public_v75 as facts, rl::ActionSemanticV1 as A,
};
struct Pair {
    own: KaitoSessionV75,
    native: FastActorSessionV1,
}
impl Pair {
    fn new(s: GameState) -> Self {
        let native = FastActorSessionV1::from_kaito_v75_fixture_state(s.clone());
        let own =
            KaitoSessionV75::from_session(FastActorSessionV1::from_kaito_v75_fixture_state(s))
                .unwrap();
        let p = Self { own, native };
        p.equal();
        p.own.decision_v75().unwrap();
        p
    }
    fn equal(&self) {
        assert_eq!(
            self.own.fixture_state_v75(),
            self.native.kaito_fixture_state()
        );
        assert_eq!(self.own.response(), self.native.current_response());
        assert_eq!(
            self.own.fixture_counts_v75(),
            (
                self.native.policy_step_count(),
                self.native.physical_decision_count()
            )
        );
    }
    fn act(&mut self, choose: impl Fn(&A) -> bool) {
        let d = self.own.decision_v75().unwrap();
        let i = d
            .actions()
            .iter()
            .position(choose)
            .unwrap_or_else(|| panic!("genuine V75 action missing: {:?}", d.actions()));
        let original = d.fixture_original_index_v75(i as u32).unwrap();
        let h = d.expected();
        assert_eq!(
            self.own.step_v75(&d, i as u32).unwrap(),
            self.native.step(h.episode_id, h.step, original).unwrap()
        );
        self.equal();
    }
    fn activate(&mut self, id: ObjectId, index: u8) {
        let d = self.own.decision_v75().unwrap();
        let row=d.candidates_v75().candidates.iter().find(|r|matches!(&r.action,A::ActivateAbility{source,ability_index,..}if source.arena_id==id.0&&*ability_index==index)).unwrap();
        let quote = row.kaito_activation.as_ref().unwrap();
        assert_eq!(quote.ability_index, index);
        assert_eq!(
            quote.after,
            match index {
                0 => quote.before + 1,
                1 => quote.before - 2,
                2 => quote.before - 9,
                _ => panic!(),
            }
        );
        self.act(|a|matches!(a,A::ActivateAbility{source,ability_index,..}if source.arena_id==id.0&&*ability_index==index));
    }
    fn target(&mut self, object: ObjectId) {
        self.act(|a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object:c},..}|A::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object:c},..} if c.arena_id==object.0));
    }
    fn quiet(&mut self) {
        let d = self.own.decision_v75().unwrap();
        if d.actions()
            .iter()
            .any(|a| matches!(a, A::FinishEffectSelection { .. }))
        {
            self.act(|a| matches!(a, A::FinishEffectSelection { .. }));
        } else {
            self.act(|a| {
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
            });
        }
    }
    fn settle(&mut self) {
        for _ in 0..512 {
            let s = self.own.fixture_state_v75();
            if s.stack.is_empty()
                && s.engine.pending_effect.is_none()
                && s.engine.pending_discard.is_none()
                && s.engine.pending_triggers.is_empty()
            {
                return;
            }
            self.quiet();
        }
        panic!("Kaito mandatory work did not settle");
    }
    fn complete_root(&mut self) {
        for _ in 0..512 {
            let d = self.own.decision_v75().unwrap();
            if d.observation()
                .kaito
                .activations
                .last()
                .is_some_and(|a| a.stage == facts::StageV75::Resolved)
            {
                return;
            }
            self.quiet();
        }
        panic!("native Kaito root did not complete");
    }
}

#[test]
fn paid_kaito_optional_target_actual_draw_private_discard_and_restore_match_native_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for selected in [false, true] {
            let (s, k, c) = owning_ready(p, false, "Island");
            // The complete V75 envelope may carry these inherited prefixes;
            // their standalone V72 entrypoints still cannot represent Kaito.
            assert!(crate::drake_public_v53::capture_tinybones_compat_v72(&s).is_err());
            assert!(crate::drake_public_v53::capture_chandra_compat_v74(&s).is_err());
            assert!(crate::spinner_public_v54::capture_tinybones_compat_v72(&s).is_err());
            for viewer in [PlayerId::P0, PlayerId::P1] {
                assert!(
                    crate::quandary_public_v55::capture_tinybones_compat_v72(&s, viewer).is_err()
                );
                assert!(
                    crate::next_five_public_v59::capture_tinybones_compat_v72(&s, viewer).is_err()
                );
                assert!(
                    crate::graveyard_return_public_v61::capture_tinybones_compat_v72(&s, viewer)
                        .is_err()
                );
                assert!(
                    crate::valkyries_call_public_v62::capture_tinybones_compat_v72(&s, viewer)
                        .is_err()
                );
            }
            let mut pair = Pair::new(s);
            pair.activate(k, 0);
            if selected {
                pair.target(c);
            } else {
                pair.act(|a| matches!(a, A::FinishEffectSelection { .. }));
            }
            for _ in 0..64 {
                if pair
                    .own
                    .decision_v75()
                    .unwrap()
                    .observation()
                    .kaito
                    .activations[0]
                    .stage
                    == facts::StageV75::WaitingDiscard
                {
                    break;
                }
                pair.quiet();
            }
            let d = pair.own.decision_v75().unwrap();
            let a = &d.observation().kaito.activations[0];
            assert_eq!((a.loyalty_before, a.loyalty_after), (3, 4));
            assert_eq!(a.targets.len(), usize::from(selected));
            assert_eq!(a.evasion.as_ref().unwrap().active_on_original, selected);
            assert_eq!(a.evasion.as_ref().unwrap().target.is_some(), selected);
            assert_eq!(a.draw, facts::DrawV75::Drawn);
            assert!(a.discard.as_ref().unwrap().private_choices.is_some());
            let other =
                crate::kaito_public_v75::capture(pair.own.fixture_state_v75(), p.opponent())
                    .unwrap();
            assert!(other.activations[0]
                .discard
                .as_ref()
                .unwrap()
                .private_choices
                .is_none());
            assert!(other.activations[0]
                .discard
                .as_ref()
                .unwrap()
                .private_chosen
                .is_none());
            let saved = pair.own.snapshot_v75();
            let raw = pair.native.snapshot_v1();
            pair.complete_root();
            let expected = pair.own.fixture_state_v75().clone();
            pair.own.restore_v75(&saved).unwrap();
            pair.native.kaito_restore_v75(&raw).unwrap();
            pair.equal();
            assert!(pair.own.step_v75(&d, 0).is_err());
            pair.complete_root();
            assert_eq!(pair.own.fixture_state_v75(), &expected);
            let turn = pair.own.fixture_state_v75().turn_key_v1();
            for _ in 0..512 {
                if pair.own.fixture_state_v75().turn_key_v1() != turn {
                    break;
                }
                pair.quiet();
            }
            assert_ne!(pair.own.fixture_state_v75().turn_key_v1(), turn);
            assert!(
                !pair
                    .own
                    .decision_v75()
                    .unwrap()
                    .observation()
                    .kaito
                    .activations[0]
                    .evasion
                    .as_ref()
                    .unwrap()
                    .active_on_original
            );
        }
    }
}
#[test]
fn paid_kaito_minus_two_season_exact_ninja_batches_and_full_current_forms_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for season in [false, true] {
            let (mut s, k, _) = owning_ready(p, season, "Island");
            s.objects.get_mut(k).counters.stash = 70_000;
            s.objects.get_mut(k).counters.fellowship = 80_000;
            let mut pair = Pair::new(s);
            pair.activate(k, 1);
            pair.complete_root();
            let d = pair.own.decision_v75().unwrap();
            let f = &d.observation().kaito;
            assert_eq!(f.ninja_batches[0].created.len(), if season { 2 } else { 1 });
            assert_eq!(f.activations[0].loyalty_after, if season { 4 } else { 1 });
            assert!(f.ninja_batches[0].created.iter().all(|n| matches!(
                n.original.identity,
                crate::optional_trigger_public_v1::IdentityV1::Known { card_id: 495 }
            )));
            assert_eq!(
                f.battlefield[p.index()]
                    .iter()
                    .find(|c| c.card.object == k.0)
                    .unwrap()
                    .counters
                    .stash,
                70_000
            );
        }
    }
}
#[test]
fn paid_kaito_ultimate_persistent_emblem_any_cast_countered_spell_and_virtual_copy_roles_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let s = crate::effect::kaito_cunning_v75::tests::owner_v75_tests::emblem_cast_boundary(p);
        let mut pair = Pair::new(s);
        let d = pair.own.decision_v75().unwrap();
        assert_eq!(d.observation().kaito.emblems.len(), 1);
        assert_eq!(d.observation().kaito.casts.len(), 2);
        assert_eq!(d.observation().kaito.emblems[0].controller, p.into());
        let snap = pair.own.snapshot_v75();
        let raw = pair.native.snapshot_v1();
        pair.settle();
        let expected = pair.own.fixture_state_v75().clone();
        let done = pair.own.decision_v75().unwrap();
        assert_eq!(done.observation().kaito.ninja_batches.len(), 2);
        assert!(done
            .observation()
            .kaito
            .triggers
            .iter()
            .all(|t| t.stage == facts::StageV75::Resolved));
        pair.own.restore_v75(&snap).unwrap();
        pair.native.kaito_restore_v75(&raw).unwrap();
        pair.settle();
        assert_eq!(pair.own.fixture_state_v75(), &expected);
    }
}
#[test]
fn paid_kaito_declared_and_native_departing_combat_preserve_original_trigger_roles_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for departing in [false, true] {
            let s = crate::effect::kaito_cunning_v75::tests::owner_v75_tests::combat_boundary(
                p, departing,
            );
            let mut pair = Pair::new(s);
            let d = pair.own.decision_v75().unwrap();
            let f = &d.observation().kaito;
            assert_eq!(f.combats.len(), 1);
            assert_eq!(f.combats[0].final_damage, 2);
            assert_eq!(f.combats[0].observer.captured_controller, p.into());
            assert_eq!(f.triggers[0].origin, facts::OriginV75::Combat(0));
            if departing {
                assert!(f.combats[0].observer.current_original_source.is_none());
            }
            pair.settle();
            assert_eq!(
                pair.own
                    .decision_v75()
                    .unwrap()
                    .observation()
                    .kaito
                    .triggers[0]
                    .stage,
                facts::StageV75::Resolved
            );
        }
    }
}
#[test]
fn paid_kaito_departure_reentry_concealment_and_current_source_visibility_match_native_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        for concealed in [false, true] {
            let (s, k, generation) =
                crate::effect::kaito_cunning_v75::tests::owner_v75_tests::published_source_boundary(
                    p, concealed,
                );
            let mut pair = Pair::new(s);
            let d = pair.own.decision_v75().unwrap();
            let a = &d.observation().kaito.activations[0];
            assert_eq!(
                (
                    a.source.original_source.object,
                    a.source.original_source.generation,
                    a.source.captured_controller
                ),
                (k.0, generation, p.into())
            );
            pair.complete_root();
            assert_eq!(
                pair.own
                    .decision_v75()
                    .unwrap()
                    .observation()
                    .kaito
                    .activations[0]
                    .stage,
                facts::StageV75::Resolved
            );
        }
    }
}
#[test]
fn paid_sealed_owner_foreign_stale_forged_complete_packet_and_callback_failures_are_atomic_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, _, _) = owning_ready(p, false, "Island");
        let mut pair = Pair::new(s.clone());
        let mut foreign = Pair::new(s);
        let d = pair.own.decision_v75().unwrap();
        let before = pair.own.fixture_state_v75().clone();
        let counts = pair.own.fixture_counts_v75();
        assert!(foreign.own.step_v75(&d, 0).is_err());
        assert!(pair.own.restore_v75(&foreign.own.snapshot_v75()).is_err());
        let mut erased = d.clone();
        erased.binding.original.clear();
        assert!(pair.own.step_v75(&erased, 0).is_err());
        let mut changed = d.clone();
        changed
            .binding
            .projection
            .observation
            .kaito
            .authorized_printings
            .clear();
        assert!(pair.own.step_v75(&changed, 0).is_err());
        let mut reordered = d.clone();
        reordered.binding.public_to_original.reverse();
        assert_ne!(
            reordered.binding.public_to_original,
            d.binding.public_to_original
        );
        assert!(pair.own.step_v75(&reordered, 0).is_err());
        assert!(pair.own.step_v75(&d, u32::MAX).is_err());
        let result: Result<_, String> = pair.own.step_with_current_check(
            &d,
            0,
            |_, _| Err("refuse before mutation".into()),
            |e| e.to_string(),
            |e| e.to_string(),
        );
        assert!(result.is_err());
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: Result<_, String> = pair.own.step_with_current_check(
                &d,
                0,
                |_, _| panic!("unwind before mutation"),
                |e| e.to_string(),
                |e| e.to_string(),
            );
        }));
        assert!(panic.is_err());
        assert_eq!(pair.own.fixture_state_v75(), &before);
        assert_eq!(pair.own.fixture_counts_v75(), counts);
        pair.own.validate_decision_v75(&d).unwrap();
        assert!(ChandraSessionV74::from_session(pair.own.fixture_native_clone_v75()).is_err());
    }
}
#[test]
fn independently_paid_private_worlds_equal_public_projection_without_sharing_authority_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let (a, _, _) = owning_ready(p, false, "Island");
        let (b, _, _) = owning_ready(p, false, "Imprisoned in the Moon");
        let mut a = Pair::new(a);
        let b = Pair::new(b);
        let da = a.own.decision_v75().unwrap();
        let db = b.own.decision_v75().unwrap();
        assert_eq!(da.candidates_v75(), db.candidates_v75());
        let before = a.own.fixture_state_v75().clone();
        assert!(a.own.step_v75(&db, 0).is_err());
        assert_eq!(a.own.fixture_state_v75(), &before);
        let raw = serde_json::to_value(da.candidates_v75()).unwrap();
        let mut unknown = raw.clone();
        unknown["observation"]["kaito"]["world"] = serde_json::json!({});
        assert!(serde_json::from_value::<KaitoCandidatesV75>(unknown).is_err());
        let mut absent = raw;
        absent["candidates"][0]
            .as_object_mut()
            .unwrap()
            .remove("kaito_activation");
        assert!(serde_json::from_value::<KaitoCandidatesV75>(absent).is_err());
    }
}
