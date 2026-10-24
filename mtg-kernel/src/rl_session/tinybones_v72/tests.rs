//! Actual paid native inputs, independent native stream and sealed V72 binding.
use super::*;
use crate::{
    engine::tinybones_v72::tests::complete_observation::owning_ready, ids::ObjectId,
    rl::ActionSemanticV1 as A, state::Zone,
};

pub(crate) struct Pair {
    pub(crate) own: TinybonesSessionV72,
    pub(crate) native: FastActorSessionV1,
}
impl Pair {
    pub(crate) fn new(state: GameState) -> Self {
        let native = FastActorSessionV1::from_tinybones_v72_fixture_state(state.clone());
        let own = TinybonesSessionV72::from_session(
            FastActorSessionV1::from_tinybones_v72_fixture_state(state),
        )
        .unwrap();
        let pair = Self { own, native };
        pair.equal();
        pair
    }
    pub(crate) fn equal(&self) {
        assert_eq!(
            self.own.fixture_state_v72(),
            self.native.tinybones_fixture_state()
        );
        assert_eq!(self.own.response(), self.native.current_response());
        assert_eq!(
            self.own.fixture_counts_v72(),
            (
                self.native.policy_step_count(),
                self.native.physical_decision_count()
            )
        );
    }
    pub(crate) fn act(&mut self, choose: impl Fn(&A) -> bool) {
        let decision = self.own.decision_v72().unwrap();
        let index = decision
            .actions()
            .iter()
            .position(choose)
            .unwrap_or_else(|| panic!("missing genuine V72 action: {:?}", decision.actions()));
        let original = decision.fixture_original_index_v72(index as u32).unwrap();
        let expected = decision.expected();
        assert_eq!(
            self.own.step_v72(&decision, index as u32).unwrap(),
            self.native
                .step(expected.episode_id, expected.step, original)
                .unwrap()
        );
        self.equal();
    }
    pub(crate) fn quiet(&mut self) {
        self.act(|a| {
            matches!(
                a,
                A::Pass { .. }
                    | A::OrderTriggers { .. }
                    | A::ChooseTriggerOrderNext { .. }
                    | A::ChooseAttackerInclusion { include: false, .. }
                    | A::ChooseBlockerInclusion { include: false, .. }
            )
        });
    }
    pub(crate) fn stash(&mut self, tiny: ObjectId, card: ObjectId) {
        let before = self.own.fixture_state_v72().players
            [self.own.fixture_state_v72().active_player.index()]
        .battlefield
        .iter()
        .filter(|id| self.own.fixture_state_v72().objects.get(**id).tapped)
        .count();
        self.act(|a|matches!(a,A::ActivateAbility{source,ability_index:0,..} if source.arena_id==tiny.0));
        for _ in 0..32 {
            let d = self.own.decision_v72().unwrap();
            if d.actions().iter().any(|a| matches!(a, A::Discard { .. })) {
                break;
            }
            self.quiet();
        }
        let state = self.own.fixture_state_v72();
        let after = state.players[state.active_player.index()]
            .battlefield
            .iter()
            .filter(|id| state.objects.get(**id).tapped)
            .count();
        // Activation is paid only after the genuine native advancement reaches discard.
        assert_eq!(after - before, 5); // Four mana lands and the printed tap source.
        self.act(|a| matches!(a,A::Discard{cards,..} if cards.iter().any(|r|r.arena_id==card.0)));
        for _ in 0..48 {
            if self.own.fixture_state_v72().objects.get(card).zone == Zone::Exile {
                return;
            }
            self.quiet();
        }
        panic!("actual Tinybones trigger failed to finish");
    }
}

#[test]
fn genuine_paid_discard_stash_original_indices_and_opaque_restore_match_native_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (state, tiny, card) = owning_ready(p, "Forest");
        let mut pair = Pair::new(state);
        pair.stash(tiny, card);
        let d = pair.own.decision_v72().unwrap();
        assert_eq!(d.observation().tinybones.stash[0].card.object, card.0);
        assert_eq!(
            d.observation().tinybones.stash[0].card.owner,
            p.opponent().into()
        );
        assert!(d.observation().tinybones.stash[0].any_type_mana[p.index()]);
        let snap = pair.own.snapshot_v72();
        let native_snap = pair.native.snapshot_v1();
        let before = pair.own.fixture_state_v72().clone();
        pair.quiet();
        pair.own.restore_v72(&snap).unwrap();
        pair.native.tinybones_restore_v72(&native_snap).unwrap();
        pair.equal();
        assert_eq!(pair.own.fixture_state_v72(), &before);
        assert!(pair.own.step_v72(&d, 0).is_err());
    }
}

#[test]
fn genuine_paid_complete_binding_refuses_foreign_erased_reordered_and_range_inputs_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (state, _, _) = owning_ready(p, "Forest");
        let mut pair = Pair::new(state.clone());
        let mut foreign = Pair::new(state);
        let d = pair.own.decision_v72().unwrap();
        let before = pair.own.fixture_state_v72().clone();
        let counts = pair.own.fixture_counts_v72();
        assert!(foreign.own.step_v72(&d, 0).is_err());
        assert!(pair.own.restore_v72(&foreign.own.snapshot_v72()).is_err());
        let mut erased = d.clone();
        erased.binding.original.clear();
        assert!(pair.own.step_v72(&erased, 0).is_err());
        let mut reordered = d.clone();
        reordered.binding.public_to_original.reverse();
        assert!(d.binding.public_to_original.len() > 1);
        assert_ne!(
            reordered.binding.public_to_original,
            d.binding.public_to_original
        );
        assert!(pair.own.step_v72(&reordered, 0).is_err());
        let mut changed = d.clone();
        changed
            .binding
            .projection
            .observation
            .tinybones
            .authorized_printings
            .clear();
        assert!(pair.own.step_v72(&changed, 0).is_err());
        assert!(pair.own.step_v72(&d, u32::MAX).is_err());
        assert_eq!(pair.own.fixture_state_v72(), &before);
        assert_eq!(pair.own.fixture_counts_v72(), counts);
    }
}

#[test]
fn independent_prepaid_hidden_worlds_keep_v72_public_bindings_equal_and_authority_foreign_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (a, _, _) = owning_ready(p, "Forest");
        let (b, _, _) = owning_ready(p, "Imprisoned in the Moon");
        let mut a = Pair::new(a);
        let b = Pair::new(b);
        let da = a.own.decision_v72().unwrap();
        let db = b.own.decision_v72().unwrap();
        assert_eq!(da.candidates_v72(), db.candidates_v72());
        let before = a.own.fixture_state_v72().clone();
        assert!(a.own.step_v72(&db, 0).is_err());
        assert_eq!(a.own.fixture_state_v72(), &before);
        assert!(MuldrothaSessionV71::from_session(a.own.fixture_native_clone_v72()).is_err());
    }
}

#[test]
fn genuine_paid_v72_exclusive_callback_error_and_unwind_leave_native_binding_unchanged_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let (state, _, _) = owning_ready(p, "Forest");
        let mut pair = Pair::new(state);
        let d = pair.own.decision_v72().unwrap();
        let before = pair.own.fixture_state_v72().clone();
        let counts = pair.own.fixture_counts_v72();
        let result: Result<_, String> = pair.own.step_with_current_check(
            &d,
            0,
            |_, _| Err("stop before native mutation".into()),
            |e| e.to_string(),
            |e| e.to_string(),
        );
        assert!(result.is_err());
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: Result<_, String> = pair.own.step_with_current_check(
                &d,
                0,
                |_, _| panic!("stop before native mutation"),
                |e| e.to_string(),
                |e| e.to_string(),
            );
        }));
        assert!(panic.is_err());
        assert_eq!(pair.own.fixture_state_v72(), &before);
        assert_eq!(pair.own.fixture_counts_v72(), counts);
        pair.own.validate_decision_v72(&d).unwrap();
    }
}
