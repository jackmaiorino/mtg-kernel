//! Genuine paid complete Call owner cases. No Full or runtime admission claim.
use super::*;
use crate::{
    call_observation_v62::tests::{hand, ready},
    card_def,
    ids::{ObjectId, PlayerId},
    rl::{ActionSemanticV1 as A, TargetRefV1},
    state::Zone,
};
struct Pair {
    own: CallSessionV62,
    native: FastActorSessionV1,
}
impl Pair {
    fn new(s: GameState) -> Self {
        // Initialize independent native streams from the same pre-advance input.
        let native = FastActorSessionV1::from_call_v62_fixture_state(s.clone());
        let own = CallSessionV62::from_session(FastActorSessionV1::from_call_v62_fixture_state(s))
            .unwrap();
        let pair = Self { own, native };
        pair.equal();
        pair
    }
    fn equal(&self) {
        assert_eq!(
            self.own.fixture_state_v62(),
            self.native.call_fixture_state()
        );
        assert_eq!(
            self.own.fixture_counts_v62(),
            (
                self.native.policy_step_count(),
                self.native.physical_decision_count()
            )
        );
        assert_eq!(self.own.response(), self.native.current_response());
    }
    fn act(&mut self, predicate: impl Fn(&A) -> bool) {
        let d = self.own.decision_v62().unwrap();
        let index = d
            .actions()
            .iter()
            .position(predicate)
            .unwrap_or_else(|| panic!("genuine Call action absent: {:?}", d.actions()));
        let raw = d.fixture_original_index_v62(index as u32).unwrap();
        let e = d.expected();
        assert_eq!(
            d.binding.original,
            self.native.current.as_ref().unwrap().candidates
        );
        assert_eq!(
            self.own.step_v62(&d, index as u32).unwrap(),
            self.native.step(e.episode_id, e.step, raw).unwrap()
        );
        self.equal();
    }
    fn quiet(&mut self) {
        let d = self.own.decision_v62().unwrap();
        let a = d
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
            .unwrap_or(&d.actions()[0])
            .clone();
        self.act(|x| x == &a);
    }
    fn drain(&mut self) {
        for _ in 0..100 {
            let s = self.own.fixture_state_v62();
            if s.stack.is_empty()
                && s.engine.pending_cast.is_none()
                && s.engine.pending_effect.is_none()
                && s.engine.pending_triggers.is_empty()
            {
                return;
            }
            self.quiet();
        }
        panic!("Call stack unfinished");
    }
    fn taps(&self, p: PlayerId) -> usize {
        let s = self.own.fixture_state_v62();
        s.players[p.index()]
            .battlefield
            .iter()
            .filter(|&&id| {
                s.objects.get(id).tapped
                    && card_def::CARD_DEFS[usize::from(s.objects.get(id).card_def)].is_land
            })
            .count()
    }
    fn cast(&mut self, id: ObjectId, target: Option<ObjectId>, cost: usize) {
        for _ in 0..32 {
            if self
                .own
                .decision_v62()
                .unwrap()
                .actions()
                .iter()
                .any(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==id.0))
            {
                break;
            }
            self.quiet();
        }
        let p = self.own.fixture_state_v62().objects.get(id).owner;
        let before = self.taps(p);
        self.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==id.0));
        if let Some(t) = target {
            self.act(|a|matches!(a,A::ChooseTarget{target:TargetRefV1::Object{object},..}if object.arena_id==t.0));
        }
        let s = self.own.fixture_state_v62();
        assert!(s.engine.pending_cast.is_none());
        assert_eq!(s.objects.get(id).zone, Zone::Stack);
        assert_eq!(s.stack.last().unwrap().source, id);
        assert_eq!(self.taps(p) - before, cost);
        assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
    }
    fn restore(&mut self) {
        let d = self.own.decision_v62().unwrap();
        let snapshot = self.own.snapshot_v62();
        let native = self.native.snapshot_v1();
        let before = self.own.fixture_state_v62().clone();
        let counters = self.own.fixture_counts_v62();
        // Serialized native rows retain their exact source/death/incarnation
        // facts; executable decisions remain private opaque owner capabilities.
        let encoded = serde_json::to_vec(&before).unwrap();
        let decoded: GameState = serde_json::from_slice(&encoded).unwrap();
        crate::call_observation_v62::validate_state(&decoded).unwrap();
        for viewer in [PlayerId::P0, PlayerId::P1] {
            assert_eq!(
                crate::valkyries_call_public_v62::capture(&decoded, viewer).unwrap(),
                crate::valkyries_call_public_v62::capture(&before, viewer).unwrap()
            );
        }
        let public = d.candidates_v62();
        assert_eq!(
            serde_json::from_slice::<crate::call_candidates_v62::CallCandidatesV62>(
                &serde_json::to_vec(public).unwrap()
            )
            .unwrap(),
            *public
        );
        self.quiet();
        self.own.restore_v62(&snapshot).unwrap();
        self.native.call_restore_v62(&native).unwrap();
        self.equal();
        assert_eq!(self.own.fixture_state_v62(), &before);
        assert_eq!(self.own.fixture_counts_v62(), counters);
        assert!(self.own.validate_decision_v62(&d).is_err());
        assert_eq!(
            self.own.decision_v62().unwrap().candidates_v62(),
            d.candidates_v62()
        );
    }
}
#[test]
fn genuine_paid_call_cast_death_opposing_source_removal_and_return_keep_full_native_pair_restore_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let call = hand(&mut s, p, "Valkyrie's Call");
        let elf = hand(&mut s, p, "Llanowar Elves");
        let bolt = hand(&mut s, p.opponent(), "Lightning Bolt");
        let wings = hand(&mut s, p.opponent(), "Broken Wings");
        let mut pair = Pair::new(s);
        pair.cast(call, None, 5);
        pair.restore();
        pair.drain();
        pair.cast(elf, None, 1);
        pair.drain();
        pair.cast(bolt, Some(elf), 1);
        for _ in 0..60 {
            let d = pair.own.decision_v62().unwrap();
            if d.observation()
                .call
                .returns
                .iter()
                .any(|r| r.stage == crate::valkyries_call_public_v62::StageV62::Published)
            {
                break;
            }
            pair.quiet();
        }
        let d = pair.own.decision_v62().unwrap();
        assert_eq!(d.observation().call.deaths.len(), 1);
        assert_eq!(d.observation().call.returns.len(), 1);
        assert!(d
            .candidates_v62()
            .candidates
            .iter()
            .all(|a| a.call_returns == d.observation().call.returns));
        pair.restore();
        pair.cast(wings, Some(call), 3);
        for _ in 0..50 {
            if pair.own.fixture_state_v62().objects.get(call).zone == Zone::Graveyard {
                break;
            }
            pair.quiet();
        }
        assert_eq!(
            pair.own.fixture_state_v62().objects.get(call).zone,
            Zone::Graveyard
        );
        let d = pair.own.decision_v62().unwrap();
        assert!(d
            .observation()
            .call
            .returns
            .iter()
            .all(|r| r.source.current_original_source.is_none()));
        pair.restore();
        pair.drain();
        let state = pair.own.fixture_state_v62();
        assert_eq!(state.objects.get(elf).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(elf).counters.plus1_plus1, 1);
        // Call adds a subtype to this incarnation. The standard-creature
        // replacement helper does not include additive subtype grants.
        assert!(crate::engine::has_effective_subtype(
            state,
            elf,
            card_def::Subtype::Angel
        ));
        assert_eq!(
            state
                .objects
                .get(elf)
                .v4
                .added_subtype_v1
                .map(|(subtype, _)| subtype),
            Some(card_def::Subtype::Angel)
        );
        assert!(crate::engine::has_effective_keyword(
            state,
            elf,
            card_def::Keywords::FLYING
        ));
    }
}
#[test]
fn paid_call_full_private_binding_unknown_fields_foreign_restore_and_callback_error_refuse_readonly_both_seats(
) {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, _, _) = crate::call_observation_v62::tests::paid_scene(p, "Forest");
        let mut pair = Pair::new(s.clone());
        let foreign = Pair::new(s);
        let d = pair.own.decision_v62().unwrap();
        let before = pair.own.fixture_state_v62().clone();
        let counts = pair.own.fixture_counts_v62();
        assert!(pair
            .own
            .validate_decision_v62(&foreign.own.decision_v62().unwrap())
            .is_err());
        assert!(pair.own.restore_v62(&foreign.own.snapshot_v62()).is_err());
        for mode in 0..4 {
            let mut bad = d.clone();
            match mode {
                0 => bad
                    .binding
                    .projection
                    .observation
                    .call
                    .meaning
                    .ordered_rules
                    .reverse(),
                1 => bad.binding.public_to_original.clear(),
                2 => bad.binding.original.clear(),
                _ => bad.actions.clear(),
            };
            assert!(pair.own.step_v62(&bad, 0).is_err());
            assert_eq!(pair.own.fixture_state_v62(), &before);
            assert_eq!(pair.own.fixture_counts_v62(), counts);
        }
        let err = pair.own.step_with_current_check(
            &d,
            0,
            |_, _| Err::<(), _>("closed callback"),
            |_| "binding",
            |_| "native",
        );
        assert_eq!(err, Err("closed callback"));
        assert_eq!(pair.own.fixture_state_v62(), &before);
        assert_eq!(pair.own.fixture_counts_v62(), counts);
        let mut raw = serde_json::to_value(d.candidates_v62()).unwrap();
        raw["private_world"] = true.into();
        assert!(
            serde_json::from_value::<crate::call_candidates_v62::CallCandidatesV62>(raw).is_err()
        );
        pair.restore();
    }
}
