//! Paid original printing and independent native-stream parity. Rust unrun.
use super::*;
use crate::{
    chandra_public_v74 as facts,
    effect::chandra_flameshaper_v1::tests::owner_v74_tests::owning_ready, ids::ObjectId,
    rl::ActionSemanticV1 as A,
};
struct Pair {
    own: ChandraSessionV74,
    native: FastActorSessionV1,
}
impl Pair {
    fn new(s: GameState) -> Self {
        let native = FastActorSessionV1::from_chandra_v74_fixture_state(s.clone());
        let own =
            ChandraSessionV74::from_session(FastActorSessionV1::from_chandra_v74_fixture_state(s))
                .unwrap();
        let p = Self { own, native };
        p.equal();
        p.own.decision_v74().unwrap();
        p
    }
    fn equal(&self) {
        assert_eq!(
            self.own.fixture_state_v74(),
            self.native.chandra_fixture_state()
        );
        assert_eq!(self.own.response(), self.native.current_response());
        assert_eq!(
            self.own.fixture_counts_v74(),
            (
                self.native.policy_step_count(),
                self.native.physical_decision_count()
            )
        );
    }
    fn act(&mut self, choose: impl Fn(&A) -> bool) {
        let d = self.own.decision_v74().unwrap();
        let i = d
            .actions()
            .iter()
            .position(choose)
            .unwrap_or_else(|| panic!("genuine V74 action missing: {:?}", d.actions()));
        let original = d.fixture_original_index_v74(i as u32).unwrap();
        let h = d.expected();
        assert_eq!(
            self.own.step_v74(&d, i as u32).unwrap(),
            self.native.step(h.episode_id, h.step, original).unwrap()
        );
        self.equal();
    }
    fn activate(&mut self, id: ObjectId, index: u8) {
        let d = self.own.decision_v74().unwrap();
        let row=d.candidates_v74().candidates.iter().find(|r|matches!(&r.action,A::ActivateAbility{source,ability_index,..}if source.arena_id==id.0&&*ability_index==index)).unwrap();
        let quote = row.chandra_activation.as_ref().unwrap();
        assert_eq!(quote.ability_index, index);
        assert_eq!(
            quote.after,
            match index {
                0 => quote.before + 2,
                1 => quote.before + 1,
                2 => quote.before - 4,
                _ => panic!(),
            }
        );
        self.act(|a|matches!(a,A::ActivateAbility{source,ability_index,..}if source.arena_id==id.0&&*ability_index==index));
    }
    fn target(&mut self, object: ObjectId) {
        self.act(|a| matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object(c),..}|A::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object(c),..} if c.arena_id==object.0));
    }
    fn quiet(&mut self) {
        let d = self.own.decision_v74().unwrap();
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
                        | A::ChooseAttackerInclusion { include: false, .. }
                        | A::ChooseBlockerInclusion { include: false, .. }
                )
            });
        }
    }
    fn complete_root(&mut self) {
        for _ in 0..512 {
            let d = self.own.decision_v74().unwrap();
            if d.observation()
                .chandra
                .activations
                .last()
                .is_some_and(|a| a.stage == facts::ActivationStageV74::NativeCompleted)
            {
                return;
            }
            self.quiet();
        }
        panic!("native Chandra root did not complete");
    }
}

#[test]
fn paid_chandra_all_roots_complete_packets_original_indices_and_restore_match_native_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for index in [0, 1, 2] {
            let (s, id, ours, _) = owning_ready(p, index == 1, "Island");
            let mut pair = Pair::new(s);
            let before = crate::planeswalker_v1::loyalty(pair.own.fixture_state_v74(), id).unwrap();
            pair.activate(id, index);
            if index == 1 {
                pair.target(ours);
            } else if index == 2 {
                pair.act(|a| matches!(a, A::FinishEffectSelection { .. }));
            }
            let d = pair.own.decision_v74().unwrap();
            let f = &d.observation().chandra;
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
            let saved = pair.own.snapshot_v74();
            let raw = pair.native.snapshot_v1();
            pair.complete_root();
            let expected = pair.own.fixture_state_v74().clone();
            let completed = pair.own.decision_v74().unwrap();
            let f = &completed.observation().chandra;
            assert_eq!(f.activations[0].ability_index, index);
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
            pair.own.restore_v74(&saved).unwrap();
            pair.native.chandra_restore_v74(&raw).unwrap();
            pair.equal();
            assert!(pair.own.step_v74(&d, 0).is_err());
            pair.complete_root();
            assert_eq!(pair.own.fixture_state_v74(), &expected);
        }
    }
}
#[test]
fn paid_chandra_eight_units_original_allocation_and_restore_match_native_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, id, ours, theirs) = owning_ready(p, false, "Island");
        let mut pair = Pair::new(s);
        pair.activate(id, 2);
        for n in 0..8 {
            let d = pair.own.decision_v74().unwrap();
            let a = d.observation().chandra.pending_allocation.as_ref().unwrap();
            assert_eq!(a.remaining_units, 8 - n);
            pair.target(if n < 3 { ours } else { theirs });
        }
        let d = pair.own.decision_v74().unwrap();
        let a = &d.observation().chandra.activations[0];
        assert_eq!(a.targets.len(), 2);
        assert_eq!(a.targets[0].allocated_damage, Some(3));
        assert_eq!(a.targets[1].allocated_damage, Some(5));
        let saved = pair.own.snapshot_v74();
        let raw = pair.native.snapshot_v1();
        pair.complete_root();
        let expected = pair.own.fixture_state_v74().clone();
        pair.own.restore_v74(&saved).unwrap();
        pair.native.chandra_restore_v74(&raw).unwrap();
        pair.complete_root();
        assert_eq!(pair.own.fixture_state_v74(), &expected);
    }
}
#[test]
fn paid_chandra_original_source_departure_reentry_and_concealment_preserve_public_binding_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        for concealed in [false, true] {
            let (s,id,generation)=crate::effect::chandra_flameshaper_v1::tests::owner_v74_tests::published_source_boundary(p,concealed);
            let mut pair = Pair::new(s);
            let d = pair.own.decision_v74().unwrap();
            let original = &d.observation().chandra.activations[0]
                .source
                .original_source;
            assert_eq!(
                (original.object, original.generation, original.controller),
                (id.0, generation, p.into())
            );
            if concealed {
                assert!(matches!(
                    d.observation().chandra.activations[0]
                        .source
                        .current_original_source
                        .as_ref()
                        .unwrap()
                        .identity,
                    crate::optional_trigger_public_v1::IdentityV1::FaceDown { .. }
                ));
            }
            pair.complete_root();
            assert_eq!(
                pair.own
                    .decision_v74()
                    .unwrap()
                    .observation()
                    .chandra
                    .exiles[0]
                    .cards
                    .len(),
                3
            );
        }
    }
}
#[test]
fn paid_sealed_owner_foreign_stale_forged_complete_packet_and_callback_failures_are_atomic_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, _, _, _) = owning_ready(p, false, "Island");
        let mut pair = Pair::new(s.clone());
        let mut foreign = Pair::new(s);
        let d = pair.own.decision_v74().unwrap();
        let before = pair.own.fixture_state_v74().clone();
        let counts = pair.own.fixture_counts_v74();
        assert!(foreign.own.step_v74(&d, 0).is_err());
        assert!(pair.own.restore_v74(&foreign.own.snapshot_v74()).is_err());
        let mut erased = d.clone();
        erased.binding.original.clear();
        assert!(pair.own.step_v74(&erased, 0).is_err());
        let mut changed = d.clone();
        changed
            .binding
            .projection
            .observation
            .chandra
            .authorized_printings
            .clear();
        assert!(pair.own.step_v74(&changed, 0).is_err());
        let mut reordered = d.clone();
        reordered.binding.public_to_original.reverse();
        assert_ne!(
            reordered.binding.public_to_original,
            d.binding.public_to_original
        );
        assert!(pair.own.step_v74(&reordered, 0).is_err());
        assert!(pair.own.step_v74(&d, u32::MAX).is_err());
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
        assert_eq!(pair.own.fixture_state_v74(), &before);
        assert_eq!(pair.own.fixture_counts_v74(), counts);
        pair.own.validate_decision_v74(&d).unwrap();
        assert!(LilianaSessionV73::from_session(pair.own.fixture_native_clone_v74()).is_err());
        // Native visible unsupported-family setup is a refusal test, not a
        // paid Kaito route or permission to inspect an opponent's hidden cards.
        let mut later = before.clone();
        crate::policy_observation_v6::tests::put(
            &mut later,
            p,
            "Kaito, Cunning Infiltrator",
            crate::state::Zone::Battlefield,
        );
        assert!(crate::chandra_observation_v74::validate_state(&later).is_err());
        assert_eq!(pair.own.fixture_state_v74(), &before);
    }
}
#[test]
fn independently_paid_private_worlds_equal_public_projection_without_sharing_authority_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let (a, _, _, _) = owning_ready(p, false, "Island");
        let (b, _, _, _) = owning_ready(p, false, "Imprisoned in the Moon");
        let mut a = Pair::new(a);
        let b = Pair::new(b);
        let da = a.own.decision_v74().unwrap();
        let db = b.own.decision_v74().unwrap();
        assert_eq!(da.candidates_v74(), db.candidates_v74());
        let before = a.own.fixture_state_v74().clone();
        assert!(a.own.step_v74(&db, 0).is_err());
        assert_eq!(a.own.fixture_state_v74(), &before);
        let raw = serde_json::to_value(da.candidates_v74()).unwrap();
        let mut unknown = raw.clone();
        unknown["observation"]["chandra"]["world"] = serde_json::json!({});
        assert!(serde_json::from_value::<ChandraCandidatesV74>(unknown).is_err());
        let mut absent = raw;
        absent["candidates"][0]
            .as_object_mut()
            .unwrap()
            .remove("chandra_activation");
        assert!(serde_json::from_value::<ChandraCandidatesV74>(absent).is_err());
    }
}

#[test]
fn paid_chandra_affected_damage_options_original_menu_indices_and_restore_match_native_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let s =
            crate::effect::chandra_flameshaper_v1::tests::owner_v74_tests::pending_damage_boundary(
                p,
            );
        let mut pair = Pair::new(s);
        let d = pair.own.decision_v74().unwrap();
        let f = d.observation().chandra.pending_damage.as_ref().unwrap();
        assert_eq!(f.chooser, p.opponent().into());
        assert_eq!((f.original_amount, f.remaining_amount), (8, 8));
        assert!(f
            .options
            .iter()
            .any(|o| o.kind == facts::PreventionKindV74::Multiplier));
        assert!(f
            .options
            .iter()
            .any(|o| o.kind == facts::PreventionKindV74::Shield && o.shield_remaining == Some(3)));
        let saved = pair.own.snapshot_v74();
        let raw = pair.native.snapshot_v1();
        while pair
            .own
            .decision_v74()
            .unwrap()
            .observation()
            .chandra
            .pending_damage
            .is_some()
        {
            pair.act(|a| matches!(a, A::ChooseEffectOption { .. }));
        }
        pair.complete_root();
        let expected = pair.own.fixture_state_v74().clone();
        pair.own.restore_v74(&saved).unwrap();
        pair.native.chandra_restore_v74(&raw).unwrap();
        pair.equal();
        while pair
            .own
            .decision_v74()
            .unwrap()
            .observation()
            .chandra
            .pending_damage
            .is_some()
        {
            pair.act(|a| matches!(a, A::ChooseEffectOption { .. }));
        }
        pair.complete_root();
        assert_eq!(pair.own.fixture_state_v74(), &expected);
    }
}
