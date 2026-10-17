use super::*;
use crate::{
    engine::time_stop_v1::tests::ready,
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    rl::{ActionSemanticV1 as A, TargetRefV1},
    state::{GameState, Zone},
};
struct Pair {
    model: EndTurnSession,
    native: FastActorSessionV1,
}
impl Pair {
    fn new(s: GameState) -> Self {
        let native = FastActorSessionV1::from_end_turn_v68_fixture_state(s.clone());
        let model =
            EndTurnSession::from_session(FastActorSessionV1::from_end_turn_v68_fixture_state(s))
                .unwrap();
        Self { model, native }
    }
    fn decision(&self) -> EndTurnDecision {
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
        let raw = d.backend.fixture_original_index_v68(i as u32).unwrap();
        let e = d.backend.expected();
        let logits = NativeModel::deterministic_fixture()
            .unwrap()
            .forward(d.packet())
            .unwrap();
        assert_eq!(logits.len(), d.backend.actions().len());
        assert!(logits.iter().all(|x| x.is_finite()));
        assert_eq!(d.packet().source(), Source::EndTurnV68);
        assert_eq!(
            self.model.apply(&d, i as u32).unwrap(),
            self.native.step(e.episode_id, e.step, raw).unwrap()
        );
        assert_eq!(
            self.model.fixture_state(),
            self.native.end_turn_fixture_state()
        );
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
fn genuine_paid_time_stop_numeric_full_packet_native_mapping_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let lower = put(&mut s, p, "Think Twice", Zone::Hand);
        let stop = put(&mut s, p, "Time Stop", Zone::Hand);
        let mut pair = Pair::new(s);
        pair.cast(lower, None);
        pair.cast(stop, None);
        for _ in 0..80 {
            if !pair
                .decision()
                .backend
                .observation()
                .end_turn
                .disposals
                .is_empty()
            {
                break;
            }
            pair.quiet();
        }
        let d = pair.decision();
        assert_eq!(d.backend.observation().end_turn.disposals.len(), 1);
        let snapshot = pair.model.snapshot();
        let native_snapshot = pair.native.snapshot_v1();
        let before = pair.model.fixture_state().clone();
        let packet = d.packet().clone();
        pair.quiet();
        pair.model.restore(&snapshot).unwrap();
        pair.native.end_turn_restore_v68(&native_snapshot).unwrap();
        assert_eq!(
            pair.model.fixture_state(),
            pair.native.end_turn_fixture_state()
        );
        assert_eq!(
            pair.model.fixture_counts(),
            (
                pair.native.policy_step_count(),
                pair.native.physical_decision_count()
            )
        );
        assert_eq!(pair.model.fixture_state(), &before);
        assert!(pair.model.apply(&d, 0).is_err());
        assert_eq!(pair.decision().packet(), &packet);
        let independent = |hidden| {
            let mut state = crate::engine::time_stop_v1::tests::ready_hidden(p, hidden);
            crate::engine::time_stop_v1::tests::cast(&mut state, p, "Think Twice", &[], 2);
            for _ in 0..12 {
                put(&mut state, p, "Forest", Zone::Hand);
            }
            crate::engine::time_stop_v1::tests::cast(&mut state, p, "Time Stop", &[], 6);
            for _ in 0..80 {
                let d = crate::engine::time_stop_v1::tests::next(&mut state);
                if state.engine.pending_discard.is_some() {
                    break;
                }
                crate::engine::time_stop_v1::tests::pass(&mut state, d);
            }
            assert!(state.engine.pending_discard.is_some());
            EndTurnSession::from_session(FastActorSessionV1::from_end_turn_v68_fixture_state(state))
                .unwrap()
        };
        let a = independent("Forest");
        let b = independent("Counterspell");
        assert_ne!(
            a.fixture_state().state_hash(),
            b.fixture_state().state_hash()
        );
        let expected = |m: &EndTurnSession| {
            let FastActorResponseV1::Decision(e) = m.current_response() else {
                panic!("independent paid cleanup decision absent")
            };
            e
        };
        assert_eq!(
            a.encode(expected(&a)).unwrap().packet(),
            b.encode(expected(&b)).unwrap().packet()
        );
    }
}
