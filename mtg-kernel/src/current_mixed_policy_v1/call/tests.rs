//! Actual sealed Call gameplay, independently initialized native reference.
use super::*;
use crate::{
    call_observation_v62::tests::{hand, ready},
    ids::{ObjectId, PlayerId},
    rl::{ActionSemanticV1 as A, TargetRefV1},
    state::{GameState, Zone},
};
struct Pair {
    model: CallSession,
    native: FastActorSessionV1,
}
impl Pair {
    fn new(s: GameState) -> Self {
        let native = FastActorSessionV1::from_call_v62_fixture_state(s.clone());
        let model =
            CallSession::from_session(FastActorSessionV1::from_call_v62_fixture_state(s)).unwrap();
        Self { model, native }
    }
    fn decision(&self) -> CallDecision {
        let FastActorResponseV1::Decision(e) = self.model.current_response() else {
            panic!("Call model terminal")
        };
        self.model.encode(e).unwrap()
    }
    fn act(&mut self, choose: impl Fn(&A) -> bool) {
        let d = self.decision();
        let i = d
            .backend
            .actions()
            .iter()
            .position(choose)
            .unwrap_or_else(|| panic!("Call model action absent: {:?}", d.backend.actions()));
        let raw = d.backend.fixture_original_index_v62(i as u32).unwrap();
        let e = d.backend.expected();
        let logits = NativeModel::deterministic_fixture()
            .unwrap()
            .forward(d.packet())
            .unwrap();
        assert_eq!(logits.len(), d.backend.actions().len());
        assert!(logits.iter().all(|x| x.is_finite()));
        assert_eq!(d.packet().source(), Source::CallV62);
        assert_eq!(
            self.model.apply(&d, i as u32).unwrap(),
            self.native.step(e.episode_id, e.step, raw).unwrap()
        );
        assert_eq!(self.model.fixture_state(), self.native.call_fixture_state());
        assert_eq!(
            self.model.fixture_counts(),
            (
                self.native.policy_step_count(),
                self.native.physical_decision_count()
            )
        );
    }
    fn quiet(&mut self) {
        let d = self.decision();
        let a = d
            .backend
            .actions()
            .iter()
            .find(|a| {
                matches!(
                    a,
                    A::Pass { .. }
                        | A::ChooseAttackerInclusion { include: false, .. }
                        | A::ChooseBlockerInclusion { include: false, .. }
                )
            })
            .unwrap_or(&d.backend.actions()[0])
            .clone();
        self.act(|x| x == &a);
    }
    fn drain(&mut self) {
        for _ in 0..100 {
            let s = self.model.fixture_state();
            if s.stack.is_empty()
                && s.engine.pending_cast.is_none()
                && s.engine.pending_effect.is_none()
                && s.engine.pending_triggers.is_empty()
            {
                return;
            }
            self.quiet();
        }
        panic!("Call model stack unfinished");
    }
    fn cast(&mut self, id: ObjectId, target: Option<ObjectId>) {
        for _ in 0..32 {
            if self
                .decision()
                .backend
                .actions()
                .iter()
                .any(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==id.0))
            {
                break;
            }
            self.quiet();
        }
        self.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==id.0));
        if let Some(t) = target {
            self.act(|a|matches!(a,A::ChooseTarget{target:TargetRefV1::Object{object},..}if object.arena_id==t.0));
        }
        let s = self.model.fixture_state();
        assert!(s.engine.pending_cast.is_none());
        assert_eq!(s.objects.get(id).zone, Zone::Stack);
        assert_eq!(s.players[s.objects.get(id).owner.index()].mana_pool, [0; 6]);
    }
}
#[test]
fn paid_call_numeric_complete_program_death_return_native_parity_restore_and_private_packet_refusal_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let call = hand(&mut s, p, "Valkyrie's Call");
        let elf = hand(&mut s, p, "Llanowar Elves");
        let bolt = hand(&mut s, p.opponent(), "Lightning Bolt");
        let mut pair = Pair::new(s);
        pair.cast(call, None);
        pair.drain();
        pair.cast(elf, None);
        pair.drain();
        pair.cast(bolt, Some(elf));
        for _ in 0..60 {
            if !pair
                .decision()
                .backend
                .observation()
                .call
                .returns
                .is_empty()
            {
                break;
            }
            pair.quiet();
        }
        let d = pair.decision();
        assert_eq!(d.backend.observation().call.deaths.len(), 1);
        assert_eq!(d.backend.observation().call.returns.len(), 1);
        let snapshot = pair.model.snapshot();
        let reference = pair.native.snapshot_v1();
        let before = pair.model.fixture_state().clone();
        let counts = pair.model.fixture_counts();
        let mut bad = d.clone();
        bad.packet.state = crate::current_mixed_policy_v1::NumericTree::encode(
            &serde_json::json!({"forged_packet": true}),
        )
        .unwrap();
        assert!(pair.model.apply(&bad, 0).is_err());
        assert_eq!(pair.model.fixture_state(), &before);
        assert_eq!(pair.model.fixture_counts(), counts);
        pair.quiet();
        pair.model.restore(&snapshot).unwrap();
        pair.native.call_restore_v62(&reference).unwrap();
        assert_eq!(pair.model.fixture_state(), pair.native.call_fixture_state());
        assert_eq!(pair.model.fixture_state(), &before);
        assert!(pair.model.apply(&d, 0).is_err());
        pair.drain();
        assert_eq!(
            pair.model.fixture_state().objects.get(elf).zone,
            Zone::Battlefield
        );
        assert_eq!(
            pair.model
                .fixture_state()
                .objects
                .get(elf)
                .counters
                .plus1_plus1,
            1
        );
    }
}
#[test]
fn independent_prepaid_hidden_worlds_have_equal_call_numeric_packets_without_later_family_admission_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (a, _, _) = crate::call_observation_v62::tests::paid_scene(p, "Forest");
        let (b, _, _) = crate::call_observation_v62::tests::paid_scene(p, "Counterspell");
        let a = Pair::new(a);
        let b = Pair::new(b);
        assert_ne!(
            a.model.fixture_state().state_hash(),
            b.model.fixture_state().state_hash()
        );
        assert_eq!(a.decision().packet(), b.decision().packet());
        let mut owner = a.model;
        let before = owner.fixture_state().clone();
        assert!(owner.restore(&b.model.snapshot()).is_err());
        assert_eq!(owner.fixture_state(), &before);
    }
}

#[test]
fn genuine_paid_call_full_width_fellowship_storage_primitive_keeps_adjacent_integer_packets_distinct_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut a, _, _) = crate::call_observation_v62::tests::paid_scene(p, "Forest");
        let land = a.players[p.index()]
            .battlefield
            .iter()
            .copied()
            .find(|&id| crate::card_def::CARD_DEFS[usize::from(a.objects.get(id).card_def)].is_land)
            .unwrap();
        // Storage operands on a genuinely paid scene, not a claimed counter spell.
        a.objects.get_mut(land).counters.fellowship = i32::MAX;
        let mut b = a.clone();
        b.objects.get_mut(land).counters.fellowship = i32::MAX - 1;
        let (a, b) = (Pair::new(a), Pair::new(b));
        let (a, b) = (a.decision(), b.decision());
        assert_ne!(a.packet(), b.packet());
        assert_eq!(
            a.backend.observation().call.battlefield[p.index()]
                .iter()
                .find(|r| r.card.object == land.0)
                .unwrap()
                .counters
                .fellowship,
            i32::MAX
        );
        assert!(NativeModel::deterministic_fixture()
            .unwrap()
            .forward(a.packet())
            .unwrap()
            .iter()
            .all(|x| x.is_finite()));
    }
}

#[path = "complete_family.rs"]
mod complete_family;
