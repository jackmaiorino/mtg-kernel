use super::*;
use crate::{
    card_def,
    engine::time_stop_v1::tests::ready,
    ids::{ObjectId, PlayerId},
    policy_observation_v6::tests::put,
    rl::{ActionSemanticV1 as A, TargetRefV1},
    state::Zone,
};
struct Pair {
    own: EndTurnSessionV68,
    native: FastActorSessionV1,
}
impl Pair {
    fn new(s: GameState) -> Self {
        // Initialize independent native streams from the same pre-advance input.
        let native = FastActorSessionV1::from_end_turn_v68_fixture_state(s.clone());
        let own =
            EndTurnSessionV68::from_session(FastActorSessionV1::from_end_turn_v68_fixture_state(s))
                .unwrap();
        let pair = Self { own, native };
        pair.equal();
        pair
    }
    fn equal(&self) {
        assert_eq!(
            self.own.fixture_state_v68(),
            self.native.end_turn_fixture_state()
        );
        assert_eq!(
            self.own.fixture_counts_v68(),
            (
                self.native.policy_step_count(),
                self.native.physical_decision_count()
            )
        );
        assert_eq!(self.own.response(), self.native.current_response());
    }
    fn act(&mut self, predicate: impl Fn(&A) -> bool) {
        let d = self.own.decision_v68().unwrap();
        let index = d
            .actions()
            .iter()
            .position(predicate)
            .unwrap_or_else(|| panic!("genuine Call action absent: {:?}", d.actions()));
        let raw = d.fixture_original_index_v68(index as u32).unwrap();
        let e = d.expected();
        assert_eq!(
            d.binding.original,
            self.native.current.as_ref().unwrap().candidates
        );
        assert_eq!(
            self.own.step_v68(&d, index as u32).unwrap(),
            self.native.step(e.episode_id, e.step, raw).unwrap()
        );
        self.equal();
    }
    fn quiet(&mut self) {
        let d = self.own.decision_v68().unwrap();
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
    fn taps(&self, p: PlayerId) -> usize {
        let s = self.own.fixture_state_v68();
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
                .decision_v68()
                .unwrap()
                .actions()
                .iter()
                .any(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==id.0))
            {
                break;
            }
            self.quiet();
        }
        let p = self.own.fixture_state_v68().objects.get(id).owner;
        let before = self.taps(p);
        self.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==id.0));
        if let Some(t) = target {
            self.act(|a|matches!(a,A::ChooseTarget{target:TargetRefV1::Object{object},..}if object.arena_id==t.0));
        }
        let s = self.own.fixture_state_v68();
        assert!(s.engine.pending_cast.is_none());
        assert_eq!(s.objects.get(id).zone, Zone::Stack);
        assert_eq!(s.stack.last().unwrap().source, id);
        assert_eq!(self.taps(p) - before, cost);
        assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
    }
    fn restore(&mut self) {
        let d = self.own.decision_v68().unwrap();
        let snapshot = self.own.snapshot_v68();
        let native = self.native.snapshot_v1();
        let before = self.own.fixture_state_v68().clone();
        let counters = self.own.fixture_counts_v68();
        // Serialized native rows retain their exact source/death/incarnation
        // facts; executable decisions remain private opaque owner capabilities.
        let encoded = serde_json::to_vec(&before).unwrap();
        let decoded: GameState = serde_json::from_slice(&encoded).unwrap();
        crate::end_turn_observation_v68::validate_state(&decoded).unwrap();
        for viewer in [PlayerId::P0, PlayerId::P1] {
            assert_eq!(
                crate::time_stop_public_v68::capture(&decoded, viewer).unwrap(),
                crate::time_stop_public_v68::capture(&before, viewer).unwrap()
            );
        }
        let public = d.candidates_v68();
        assert_eq!(
            serde_json::from_slice::<crate::end_turn_candidates_v68::EndTurnCandidatesV68>(
                &serde_json::to_vec(public).unwrap()
            )
            .unwrap(),
            *public
        );
        self.quiet();
        self.own.restore_v68(&snapshot).unwrap();
        self.native.end_turn_restore_v68(&native).unwrap();
        self.equal();
        assert_eq!(self.own.fixture_state_v68(), &before);
        assert_eq!(self.own.fixture_counts_v68(), counters);
        assert!(self.own.validate_decision_v68(&d).is_err());
        assert_eq!(
            self.own.decision_v68().unwrap().candidates_v68(),
            d.candidates_v68()
        );
    }
}
#[test]
fn genuine_paid_time_stop_owner_complete_disposal_native_parity_and_stale_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let lower = put(&mut s, p, "Think Twice", Zone::Hand);
        let stop = put(&mut s, p, "Time Stop", Zone::Hand);
        let mut pair = Pair::new(s);
        pair.cast(lower, None, 2);
        pair.restore();
        pair.cast(stop, None, 6);
        pair.restore();
        for _ in 0..80 {
            if !pair
                .own
                .decision_v68()
                .unwrap()
                .observation()
                .end_turn
                .disposals
                .is_empty()
            {
                break;
            }
            pair.quiet();
        }
        let d = pair.own.decision_v68().unwrap();
        assert_eq!(d.observation().end_turn.disposals.len(), 1);
        assert_eq!(
            pair.own.fixture_state_v68().objects.get(lower).zone,
            Zone::Exile
        );
        assert_eq!(
            pair.own.fixture_state_v68().objects.get(stop).zone,
            Zone::Exile
        );
        assert!(d
            .candidates_v68()
            .candidates
            .iter()
            .all(|r| r.disposed_stack_outcomes == d.observation().end_turn.disposals));
        let before = pair.own.fixture_state_v68().clone();
        let counts = pair.own.fixture_counts_v68();
        let mut forged = d.clone();
        forged.binding.projection.observation.end_turn.disposals[0].ending_spell_position = 0;
        assert!(pair.own.validate_decision_v68(&forged).is_err());
        assert!(pair.own.step_v68(&forged, 0).is_err());
        assert_eq!(pair.own.fixture_state_v68(), &before);
        assert_eq!(pair.own.fixture_counts_v68(), counts);
        let foreign = EndTurnSessionV68::from_session(
            FastActorSessionV1::from_end_turn_v68_fixture_state(before.clone()),
        )
        .unwrap();
        assert!(pair
            .own
            .validate_decision_v68(&foreign.decision_v68().unwrap())
            .is_err());
        pair.restore();
    }
}

#[test]
fn genuine_paid_time_stop_cleanup_discard_complete_context_native_parity_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p);
        let stop = put(&mut s, p, "Time Stop", Zone::Hand);
        for _ in 0..12 {
            put(&mut s, p, "Forest", Zone::Hand);
        }
        let mut pair = Pair::new(s);
        pair.cast(stop, None, 6);
        for _ in 0..80 {
            if pair
                .own
                .fixture_state_v68()
                .engine
                .pending_discard
                .is_some()
            {
                break;
            }
            pair.quiet();
        }
        let d = pair.own.decision_v68().unwrap();
        let cleanup = d.observation().end_turn.cleanup.as_ref().unwrap();
        assert_eq!(
            cleanup.phase,
            crate::time_stop_public_v68::PhaseV68::Discard
        );
        assert_eq!(cleanup.discard_count, Some(5));
        let pending = d
            .observation()
            .common
            .common
            .projection
            .surface
            .engine_context
            .pending_discard
            .as_ref()
            .unwrap();
        assert_eq!(
            pending.resume_stage,
            crate::rl::DiscardResumeSemanticV2::TimeStopCleanupV68
        );
        assert_eq!(pending.count, 5);
        assert!(pending.resume_source.is_none());
        assert!(d.actions().iter().all(|a| matches!(a, A::Discard { .. })));
        pair.restore();
        for _ in 0..80 {
            if pair
                .own
                .fixture_state_v68()
                .engine
                .pending_discard
                .is_none()
            {
                break;
            }
            pair.quiet();
        }
        assert_eq!(
            pair.own.fixture_state_v68().players[p.index()].hand.len(),
            7
        );
        pair.equal();
    }
}

#[test]
fn genuine_paid_disposed_drake_erased_overlay_is_not_a_fresh_owner_binding_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let started = std::time::Instant::now();
        let stage = |name: &str| {
            eprintln!(
                "V68 disposed Drake seat={p:?} stage={name} elapsed_ms={}",
                started.elapsed().as_millis()
            )
        };
        stage("setup");
        let mut s = ready(p);
        let h = crate::engine::time_stop_v1::tests::cast(&mut s, p, "Drake Hatcher", &[], 2);
        crate::engine::time_stop_v1::tests::finish(&mut s);
        stage("drake_paid");
        // Genuine native combat-commit primitive, not declared combat.
        crate::engine::commit_combat_damage_events(
            &mut s,
            vec![crate::event::ProposedEvent::damage(
                h,
                crate::state::Target::Player(p.opponent()),
                1,
            )],
        );
        crate::engine::time_stop_v1::tests::priority(&mut s, p);
        assert!(s
            .stack
            .iter()
            .any(|r| r.source == h && r.kind == crate::state::StackItemKind::TriggeredAbility));
        crate::engine::time_stop_v1::tests::cast(&mut s, p, "Time Stop", &[], 6);
        crate::engine::time_stop_v1::tests::end_turn(&mut s);
        stage("time_stop_completed");
        let pair = Pair::new(s);
        stage("owning_session");
        let d = pair.own.decision_v68().unwrap();
        stage("complete_decision");
        assert_eq!(d.observation().end_turn.compatibility_terminals.len(), 1);
        let before = pair.own.fixture_state_v68().clone();
        let counts = pair.own.fixture_counts_v68();
        let mut forged = d.clone();
        let observation = &mut forged.binding.projection.observation;
        observation.end_turn.compatibility_terminals.clear();
        observation.projection_sha256 =
            crate::end_turn_observation_v68::commitment(observation).unwrap();
        stage("omitted_overlay_recommitted");
        // Supplemental value checks cannot authenticate an omitted inventory.
        // The actual owner recaptures the complete native publication roster.
        crate::time_stop_public_v68::validate(&observation.end_turn).unwrap();
        crate::end_turn_observation_v68::validate_artifact(observation).unwrap();
        stage("supplemental_artifact_validation");
        assert!(pair.own.validate_decision_v68(&forged).is_err());
        stage("forgery_refused");
        assert_eq!(pair.own.fixture_state_v68(), &before);
        assert_eq!(pair.own.fixture_counts_v68(), counts);
        pair.own.validate_decision_v68(&d).unwrap();
        stage("genuine_validated");
    }
}
