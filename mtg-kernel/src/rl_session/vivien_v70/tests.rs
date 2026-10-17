//! Independent actual native streams from identical pre-advance inputs.
//! Battlefield/hand setup is an initial native fixture, never production reset admission.
use super::*;
use crate::{
    card_def, ids::ObjectId, policy_observation_v6::tests::put, rl::ActionSemanticV1 as A,
    state::Zone,
};
pub(crate) fn ready(p: PlayerId, hidden: &str) -> (GameState, ObjectId, ObjectId) {
    let own = [
        "Llanowar Elves",
        "Think Twice",
        "Forest",
        "Healer's Hawk",
        "Mountain",
        "Island",
    ]
    .map(|n| card_def::card_id_by_name(n).unwrap());
    let other = vec![card_def::card_id_by_name(hidden).unwrap(); 16];
    let (a, b) = if p == PlayerId::P0 {
        (own.as_slice(), other.as_slice())
    } else {
        (other.as_slice(), own.as_slice())
    };
    let mut s =
        GameState::new_from_libraries_with_starting_player_v1(a, b, crate::rl::card_name, 11070, p);
    s.step = crate::state::Step::Main1;
    for _ in 0..5 {
        put(&mut s, p, "Forest", Zone::Battlefield);
    }
    let v = put(&mut s, p, "Vivien Reid", Zone::Hand);
    let h = put(&mut s, p.opponent(), "Healer's Hawk", Zone::Battlefield);
    (s, v, h)
}
pub(crate) struct Pair {
    pub(crate) own: VivienSessionV70,
    pub(crate) native: FastActorSessionV1,
}
impl Pair {
    pub(crate) fn new(s: GameState) -> Self {
        let native = FastActorSessionV1::from_vivien_v70_fixture_state(s.clone());
        let own =
            VivienSessionV70::from_session(FastActorSessionV1::from_vivien_v70_fixture_state(s))
                .unwrap();
        let pair = Self { own, native };
        pair.equal();
        pair
    }
    pub(crate) fn equal(&self) {
        assert_eq!(
            self.own.fixture_state_v70(),
            self.native.vivien_fixture_state()
        );
        assert_eq!(
            self.own.fixture_counts_v70(),
            (
                self.native.policy_step_count(),
                self.native.physical_decision_count()
            )
        );
        assert_eq!(self.own.response(), self.native.current_response());
    }
    pub(crate) fn act(&mut self, choose: impl Fn(&A) -> bool) {
        let d = self.own.decision_v70().unwrap();
        let i = d
            .actions()
            .iter()
            .position(choose)
            .unwrap_or_else(|| panic!("actual Wave action absent: {:?}", d.actions()));
        assert_eq!(
            d.binding.original,
            self.native.current.as_ref().unwrap().candidates
        );
        let raw = d.fixture_original_index_v70(i as u32).unwrap();
        let e = d.expected();
        assert_eq!(
            self.own.step_v70(&d, i as u32).unwrap(),
            self.native.step(e.episode_id, e.step, raw).unwrap()
        );
        self.equal();
    }
    pub(crate) fn quiet(&mut self) {
        let d = self.own.decision_v70().unwrap();
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
        self.act(|r| r == &a);
    }
    pub(crate) fn cast(&mut self, v: ObjectId, p: PlayerId) {
        self.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==v.0));
        let s = self.own.fixture_state_v70();
        assert!(s.engine.pending_cast.is_none());
        assert_eq!(crate::engine::time_stop_v1::tests::tapped(s, p), 5);
        assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
        assert!(s
            .stack
            .iter()
            .any(|r| r.source == v && r.v4.source_contract.is_some()));
        for _ in 0..12 {
            if self.own.fixture_state_v70().objects.get(v).zone == Zone::Battlefield {
                return;
            }
            self.quiet();
        }
        panic!("paid Vivien did not enter")
    }
    pub(crate) fn advance_to_look(&mut self) {
        for _ in 0..16 {
            if self
                .own
                .decision_v70()
                .unwrap()
                .observation()
                .vivien
                .activations
                .iter()
                .any(|a| a.stage == crate::vivien_public_v70::StageV70::Looking)
            {
                return;
            }
            self.quiet();
        }
        panic!("native look not issued")
    }

    pub(crate) fn restore(&mut self) {
        let d = self.own.decision_v70().unwrap();
        let save = self.own.snapshot_v70();
        let raw = self.native.snapshot_v1();
        let world = self.own.fixture_state_v70().clone();
        let counts = self.own.fixture_counts_v70();
        let decoded: GameState =
            serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        assert_eq!(decoded, world);
        assert_eq!(decoded.state_hash(), world.state_hash());
        crate::vivien_observation_v70::validate_state(&decoded).unwrap();
        let value = d.candidates_v70();
        assert_eq!(
            serde_json::from_slice::<crate::vivien_candidates_v70::VivienCandidatesV70>(
                &serde_json::to_vec(value).unwrap()
            )
            .unwrap(),
            *value
        );
        self.quiet();
        self.own.restore_v70(&save).unwrap();
        self.native.vivien_restore_v70(&raw).unwrap();
        self.equal();
        assert_eq!(self.own.fixture_state_v70(), &world);
        assert_eq!(self.own.fixture_counts_v70(), counts);
        assert!(self.own.validate_decision_v70(&d).is_err());
        assert_eq!(self.own.decision_v70().unwrap().candidates_v70(), value);
    }
}
#[test]
fn genuine_paid_vivien_owner_cast_loyalty_optional_look_and_restore_native_parity_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for choose in [false, true] {
            let (s, v, _) = ready(p, "Island");
            let mut pair = Pair::new(s);
            pair.cast(v, p);
            pair.restore();
            let quote = pair.own.decision_v70().unwrap();
            let cost = quote
                .candidates_v70()
                .candidates
                .iter()
                .find_map(|r| {
                    r.vivien_activation
                        .as_ref()
                        .filter(|c| c.ability_index == 0)
                })
                .unwrap();
            assert_eq!((cost.before, cost.after), (5, 6));
            pair.act(|a|matches!(a,A::ActivateAbility{source,ability_index:0,..}if source.arena_id==v.0));
            assert_eq!(
                crate::planeswalker_v1::loyalty(pair.own.fixture_state_v70(), v),
                Some(6)
            );
            pair.restore();
            pair.advance_to_look();
            pair.restore();
            let before = pair.own.fixture_state_v70().players[p.index()].hand.len();
            if choose {
                pair.act(|a|matches!(a,A::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.card_db_id==card_def::card_id_by_name("Llanowar Elves").unwrap()));
            } else {
                pair.act(|a| matches!(a, A::FinishEffectSelection { .. }));
            }
            for _ in 0..12 {
                if pair
                    .own
                    .decision_v70()
                    .unwrap()
                    .observation()
                    .vivien
                    .activations[0]
                    .stage
                    == crate::vivien_public_v70::StageV70::LookFinished
                {
                    break;
                }
                pair.quiet();
            }
            assert_eq!(
                pair.own.fixture_state_v70().players[p.index()].hand.len(),
                before + usize::from(choose)
            );
            assert_eq!(
                pair.own
                    .decision_v70()
                    .unwrap()
                    .observation()
                    .vivien
                    .activations[0]
                    .stage,
                crate::vivien_public_v70::StageV70::LookFinished
            );
            pair.equal();
        }
    }
}
#[test]
fn genuine_paid_vivien_destroy_target_and_foreign_stale_packet_refuse_readonly_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, v, host) = ready(p, "Island");
        let mut pair = Pair::new(s);
        pair.cast(v, p);
        pair.act(
            |a| matches!(a,A::ActivateAbility{source,ability_index:1,..}if source.arena_id==v.0),
        );
        pair.act(|a|matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==host.0));
        pair.restore();
        let d = pair.own.decision_v70().unwrap();
        let facts = &d.observation().vivien;
        assert_eq!(
            (
                facts.activations[0].loyalty_before,
                facts.activations[0].loyalty_after
            ),
            (5, 2)
        );
        assert!(facts.activations[0].original_targets[0].legal_now);
        let before = pair.own.fixture_state_v70().clone();
        let mut foreign = Pair::new(before.clone());
        assert!(foreign.own.step_v70(&d, 0).is_err());
        let mut bad = d.clone();
        bad.binding.projection.observation.vivien.activations[0].loyalty_after = 3;
        assert!(pair.own.step_v70(&bad, 0).is_err());
        assert_eq!(pair.own.fixture_state_v70(), &before);
        for _ in 0..12 {
            if pair.own.fixture_state_v70().objects.get(host).zone == Zone::Graveyard {
                break;
            }
            pair.quiet();
        }
        assert_eq!(
            pair.own.fixture_state_v70().objects.get(host).zone,
            Zone::Graveyard
        );
        assert_eq!(
            pair.own
                .decision_v70()
                .unwrap()
                .observation()
                .vivien
                .activations[0]
                .stage,
            crate::vivien_public_v70::StageV70::NoLongerPublished
        );
    }
}
#[test]
fn genuine_paid_vivien_independent_hidden_worlds_keep_complete_public_packet_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let make = |hidden| {
            let (s, v, _) = ready(p, hidden);
            let mut pair = Pair::new(s);
            pair.cast(v, p);
            pair.act(|a|matches!(a,A::ActivateAbility{source,ability_index:0,..}if source.arena_id==v.0));
            pair.advance_to_look();
            pair
        };
        let a = make("Island");
        let b = make("Mountain");
        assert_ne!(a.own.fixture_state_v70(), b.own.fixture_state_v70());
        assert_eq!(
            a.own.decision_v70().unwrap().candidates_v70(),
            b.own.decision_v70().unwrap().candidates_v70()
        );
        assert!(
            crate::genesis_wave_observation_v69::validate_state(a.own.fixture_state_v70()).is_err()
        );
    }
}

#[test]
fn genuine_paid_vivien_ultimate_time_stop_response_has_disposed_outcome_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = crate::effect::vivien_reid_v1::tests::ready(
            p,
            &["Forest"; 8],
            &["Vivien Reid", "Time Stop"],
            "Island",
        );
        let v = crate::effect::vivien_reid_v1::tests::walker(&mut s, p);
        // Native counter-placement primitive funds the printed ultimate; activation itself is paid.
        crate::planeswalker_v1::add_loyalty_counters_checked_v1(&mut s, v, 3).unwrap();
        let row = crate::effect::vivien_reid_v1::tests::activate(&mut s, p, v, 2, None);
        assert_eq!(crate::planeswalker_v1::loyalty(&s, v), None);
        let stop = *s.players[p.index()]
            .hand
            .iter()
            .find(|id| {
                s.objects.get(**id).card_def == card_def::card_id_by_name("Time Stop").unwrap()
            })
            .unwrap();
        let taps = crate::engine::time_stop_v1::tests::tapped(&s, p);
        let mut pair = Pair::new(s);
        pair.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==stop.0));
        assert_eq!(
            crate::engine::time_stop_v1::tests::tapped(pair.own.fixture_state_v70(), p) - taps,
            6
        );
        assert_eq!(
            pair.own.fixture_state_v70().players[p.index()].mana_pool,
            [0; 6]
        );
        for _ in 0..20 {
            if crate::engine::time_stop_v1::disposed(pair.own.fixture_state_v70(), &row).unwrap() {
                break;
            }
            pair.quiet();
        }
        assert!(crate::engine::time_stop_v1::disposed(pair.own.fixture_state_v70(), &row).unwrap());
        let d = pair.own.decision_v70().unwrap();
        assert_eq!(
            d.observation().vivien.activations[0].stage,
            crate::vivien_public_v70::StageV70::Disposed
        );
        assert!(d.observation().vivien.emblems.is_empty());
        assert!(!d.observation().end_turn.disposals.is_empty());
        pair.restore();
        pair.equal();
    }
}
