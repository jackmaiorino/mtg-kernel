//! Paid original printing and independent native-stream parity. Rust unrun.
use super::*;
use crate::{
    effect::liliana_dreadhorde_v1::tests::owner_v73_tests::owning_ready, ids::ObjectId,
    liliana_public_v73 as facts, rl::ActionSemanticV1 as A,
};
struct Pair {
    own: LilianaSessionV73,
    native: FastActorSessionV1,
}
impl Pair {
    fn new(s: GameState) -> Self {
        let native = FastActorSessionV1::from_liliana_v73_fixture_state(s.clone());
        let own =
            LilianaSessionV73::from_session(FastActorSessionV1::from_liliana_v73_fixture_state(s))
                .unwrap();
        let p = Self { own, native };
        p.equal();
        p.own.decision_v73().unwrap();
        p
    }
    fn equal(&self) {
        assert_eq!(
            self.own.fixture_state_v73(),
            self.native.liliana_fixture_state()
        );
        assert_eq!(self.own.response(), self.native.current_response());
        assert_eq!(
            self.own.fixture_counts_v73(),
            (
                self.native.policy_step_count(),
                self.native.physical_decision_count()
            )
        );
    }
    fn act(&mut self, choose: impl Fn(&A) -> bool) {
        let d = self.own.decision_v73().unwrap();
        let i = d
            .actions()
            .iter()
            .position(choose)
            .unwrap_or_else(|| panic!("genuine V73 action missing: {:?}", d.actions()));
        let original = d.fixture_original_index_v73(i as u32).unwrap();
        let h = d.expected();
        assert_eq!(
            self.own.step_v73(&d, i as u32).unwrap(),
            self.native.step(h.episode_id, h.step, original).unwrap()
        );
        self.equal();
    }
    fn activate(&mut self, id: ObjectId, index: u8) {
        let d = self.own.decision_v73().unwrap();
        let row=d.candidates_v73().candidates.iter().find(|r|matches!(&r.action,A::ActivateAbility{source,ability_index,..}if source.arena_id==id.0&&*ability_index==index)).unwrap();
        let quote = row.liliana_activation.as_ref().unwrap();
        assert_eq!(quote.ability_index, index);
        assert_eq!(
            quote.after,
            match index {
                0 => quote.before + 1,
                1 => quote.before - 4,
                2 => quote.before - 9,
                _ => panic!(),
            }
        );
        self.act(|a|matches!(a,A::ActivateAbility{source,ability_index,..}if source.arena_id==id.0&&*ability_index==index));
        let d = self.own.decision_v73().unwrap();
        let a = d.observation().liliana.activations.last().unwrap();
        assert_eq!(
            (a.loyalty_before, a.loyalty_after),
            (quote.before, quote.after)
        );
    }
    fn quiet(&mut self) {
        let d = self.own.decision_v73().unwrap();
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
                        | A::ChooseAttackerInclusion { include: false, .. }
                        | A::ChooseBlockerInclusion { include: false, .. }
                )
            });
        }
    }
    fn complete_root(&mut self) {
        for _ in 0..512 {
            let d = self.own.decision_v73().unwrap();
            if d.observation()
                .liliana
                .activations
                .last()
                .is_some_and(|a| a.stage == facts::ActivationStageV73::NativeCompleted)
            {
                return;
            }
            self.quiet();
        }
        panic!("native Liliana root did not complete");
    }
}
#[test]
fn paid_plus_one_original_loyalty_exact_zombies_wide_fields_and_restore_match_native_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for season in [false, true] {
            let (mut s, id) = owning_ready(p, season, "Island");
            // Explicit full-width counter boundary, separate from paid printing/root.
            s.objects.get_mut(id).counters.stash = 70_000;
            s.objects.get_mut(id).counters.fellowship = 80_000;
            let mut pair = Pair::new(s);
            pair.activate(id, 0);
            let d = pair.own.decision_v73().unwrap();
            let snap = pair.own.snapshot_v73();
            let native = pair.native.snapshot_v1();
            pair.complete_root();
            let expected = pair.own.fixture_state_v73().clone();
            let completed = pair.own.decision_v73().unwrap();
            let f = &completed.observation().liliana;
            assert_eq!(
                f.activations[0].created_zombies.len(),
                if season { 2 } else { 1 }
            );
            assert_eq!(
                f.battlefield[p.index()]
                    .iter()
                    .find(|c| c.card.object == id.0)
                    .unwrap()
                    .counters
                    .stash,
                70_000
            );
            pair.own.restore_v73(&snap).unwrap();
            pair.native.liliana_restore_v73(&native).unwrap();
            pair.equal();
            assert!(pair.own.step_v73(&d, 0).is_err());
            pair.complete_root();
            assert_eq!(pair.own.fixture_state_v73(), &expected);
        }
    }
}
#[test]
fn paid_minus_four_all_choose_before_batch_and_native_observer_inventory_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, id) = owning_ready(p, false, "Island");
        let mut pair = Pair::new(s);
        pair.activate(id, 1);
        let mut actors = Vec::new();
        let mut saw_partial = false;
        for _ in 0..512 {
            let d = pair.own.decision_v73().unwrap();
            let f = &d.observation().liliana;
            if f.activations.last().unwrap().stage == facts::ActivationStageV73::NativeCompleted {
                break;
            }
            for c in d
                .candidates_v73()
                .candidates
                .first()
                .unwrap()
                .liliana_choices
                .iter()
            {
                if c.kind == facts::SelectionKindV73::SacrificeCreatures {
                    if actors.last() != Some(&c.actor) {
                        actors.push(c.actor);
                    }
                    saw_partial |= !c.selected_prefix.is_empty();
                    let r = f.resolutions.last().unwrap();
                    assert!(r.completed_sacrifices.is_empty());
                    for o in r.rosters.iter().flatten() {
                        assert_eq!(
                            pair.own
                                .fixture_state_v73()
                                .objects
                                .get(ObjectId(o.original.object))
                                .zone,
                            crate::state::Zone::Battlefield
                        );
                    }
                }
            }
            pair.quiet();
        }
        let d = pair.own.decision_v73().unwrap();
        let f = &d.observation().liliana;
        assert_eq!(actors, vec![p.into(), p.opponent().into()]);
        assert!(saw_partial);
        let r = f.resolutions.last().unwrap();
        assert!(r.finished);
        assert_eq!(r.completed_sacrifices.len(), 4);
        assert_eq!(f.deaths.len(), 2);
        assert!(f.deaths.iter().all(|d| d.collected));
        // Genuine current held/published draw inventory has original ability contracts.
        assert_eq!(f.current_draws.len(), 2);
    }
}
#[test]
fn paid_ultimate_multitype_same_keeper_union_protected_permanents_and_original_mapping_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, id) = owning_ready(p, true, "Island");
        let mut pair = Pair::new(s);
        pair.activate(id, 2);
        let mut kept_hart = 0;
        for _ in 0..512 {
            let d = pair.own.decision_v73().unwrap();
            let f = &d.observation().liliana;
            if f.activations.last().unwrap().stage == facts::ActivationStageV73::NativeCompleted {
                break;
            }
            let current = d
                .candidates_v73()
                .candidates
                .first()
                .unwrap()
                .liliana_choices
                .first();
            if let Some(c) = current {
                if matches!(
                    c.kind,
                    facts::SelectionKindV73::KeepPermanent(
                        crate::card_def::CardType::Creature | crate::card_def::CardType::Artifact
                    )
                ) && c.selected_prefix.is_empty()
                {
                    let hart = c
                        .candidates
                        .iter()
                        .find(|b| {
                            pair.own
                                .fixture_state_v73()
                                .objects
                                .get(ObjectId(b.object))
                                .card_def
                                == crate::card_def::card_id_by_name("Burnished Hart").unwrap()
                        })
                        .unwrap()
                        .object;
                    pair.act(|a|matches!(a,A::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==hart));
                    kept_hart += 1;
                    continue;
                }
            }
            pair.quiet();
        }
        let d = pair.own.decision_v73().unwrap();
        let r = d.observation().liliana.resolutions.last().unwrap();
        assert!(r.finished && r.ultimate);
        assert_eq!(kept_hart, 2);
        let hart = r
            .keepers
            .iter()
            .find(|k| k.permanent_type == crate::card_def::CardType::Creature)
            .unwrap()
            .permanent
            .object;
        assert_eq!(
            r.keepers
                .iter()
                .filter(|k| k.permanent.object == hart)
                .count(),
            2
        );
        assert!(!r.completed_sacrifices.iter().any(|b| b.object == hart));
    }
}
#[test]
fn paid_sealed_owner_foreign_stale_forged_complete_packet_and_callback_failures_are_atomic_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, _) = owning_ready(p, false, "Island");
        let mut pair = Pair::new(s.clone());
        let mut foreign = Pair::new(s);
        let d = pair.own.decision_v73().unwrap();
        let before = pair.own.fixture_state_v73().clone();
        let counts = pair.own.fixture_counts_v73();
        assert!(foreign.own.step_v73(&d, 0).is_err());
        assert!(pair.own.restore_v73(&foreign.own.snapshot_v73()).is_err());
        let mut erased = d.clone();
        std::sync::Arc::make_mut(&mut erased.binding)
            .original
            .clear();
        assert!(pair.own.step_v73(&erased, 0).is_err());
        let mut changed = d.clone();
        std::sync::Arc::make_mut(&mut changed.binding)
            .projection
            .observation
            .liliana
            .authorized_printings
            .clear();
        assert!(pair.own.step_v73(&changed, 0).is_err());
        let mut reordered = d.clone();
        std::sync::Arc::make_mut(&mut reordered.binding)
            .public_to_original
            .reverse();
        assert_ne!(
            reordered.binding.public_to_original,
            d.binding.public_to_original
        );
        assert!(pair.own.step_v73(&reordered, 0).is_err());
        assert!(pair.own.step_v73(&d, u32::MAX).is_err());
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
        assert_eq!(pair.own.fixture_state_v73(), &before);
        assert_eq!(pair.own.fixture_counts_v73(), counts);
        pair.own.validate_decision_v73(&d).unwrap();
        assert!(TinybonesSessionV72::from_session(pair.own.fixture_native_clone_v73()).is_err());
    }
}
#[test]
fn independently_paid_private_worlds_equal_public_projection_without_sharing_authority_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let (a, _) = owning_ready(p, false, "Island");
        let (b, _) = owning_ready(p, false, "Imprisoned in the Moon");
        let mut a = Pair::new(a);
        let b = Pair::new(b);
        let da = a.own.decision_v73().unwrap();
        let db = b.own.decision_v73().unwrap();
        assert_eq!(da.candidates_v73(), db.candidates_v73());
        let before = a.own.fixture_state_v73().clone();
        assert!(a.own.step_v73(&db, 0).is_err());
        assert_eq!(a.own.fixture_state_v73(), &before);
        let raw = serde_json::to_value(da.candidates_v73()).unwrap();
        let mut unknown = raw.clone();
        unknown["observation"]["liliana"]["world"] = serde_json::json!({});
        assert!(serde_json::from_value::<LilianaCandidatesV73>(unknown).is_err());
        let mut absent = raw;
        absent["candidates"][0]
            .as_object_mut()
            .unwrap()
            .remove("liliana_activation");
        assert!(serde_json::from_value::<LilianaCandidatesV73>(absent).is_err());
    }
}

#[test]
fn paid_source_departure_reentry_and_concealment_preserve_original_public_binding_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for concealed in [false, true] {
            let (s,id,original)=crate::effect::liliana_dreadhorde_v1::tests::owner_v73_tests::published_source_boundary(p,concealed);
            let mut pair = Pair::new(s);
            let d = pair.own.decision_v73().unwrap();
            let a = &d.observation().liliana.activations[0];
            assert_eq!(a.source.original_source.generation, original);
            assert_eq!(
                a.source.original_source.identity,
                crate::optional_trigger_public_v1::IdentityV1::Known { card_id: 491 }
            );
            assert_eq!(a.source.captured_controller, p.into());
            if concealed {
                assert!(matches!(
                    a.source.current_original_source.as_ref().unwrap().identity,
                    crate::optional_trigger_public_v1::IdentityV1::FaceDown { .. }
                ));
            } else {
                assert!(a.source.current_original_source.is_none());
                assert_eq!(
                    pair.own.fixture_state_v73().objects.get(id).controller,
                    p.opponent()
                );
            }
            pair.complete_root();
            let d = pair.own.decision_v73().unwrap();
            let a = &d.observation().liliana.activations[0];
            assert_eq!(a.created_zombies.len(), 1);
            assert_eq!(a.created_zombies[0].controller, p.into());
        }
    }
}
#[test]
fn paid_minus_four_at_exact_four_loses_own_observer_before_simultaneous_sacrifices_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, id) = owning_ready(p, false, "Island");
        // Explicit preactivation loyalty boundary, followed by genuine paid -4.
        crate::planeswalker_v1::change_loyalty(&mut s, id, -2);
        let mut pair = Pair::new(s);
        pair.activate(id, 1);
        assert_eq!(
            pair.own.fixture_state_v73().objects.get(id).zone,
            crate::state::Zone::Graveyard
        );
        pair.complete_root();
        let d = pair.own.decision_v73().unwrap();
        let f = &d.observation().liliana;
        assert!(f.deaths.is_empty() && f.current_draws.is_empty());
        assert_eq!(f.resolutions[0].completed_sacrifices.len(), 4);
    }
}
