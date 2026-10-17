//! Independent actual native streams from identical pre-advance inputs.
//! Battlefield/hand setup is an initial native fixture, never production reset admission.
use super::*;
use crate::{
    card_def, ids::ObjectId, policy_observation_v6::tests::put, rl::ActionSemanticV1 as A,
    state::Zone,
};
pub(crate) fn ready(p: PlayerId, hidden: &str) -> (GameState, ObjectId, ObjectId) {
    let mut s = crate::effect::genesis_wave_v1::tests::ready(
        p,
        &["Forest", "Giant Growth", "Llanowar Elves", "Pacifism"],
        12,
        hidden,
    );
    let wave = put(&mut s, p, "Genesis Wave", Zone::Hand);
    let host = put(&mut s, p, "Llanowar Elves", Zone::Battlefield);
    (s, wave, host)
}
pub(crate) struct Pair {
    pub(crate) own: GenesisWaveSessionV69,
    pub(crate) native: FastActorSessionV1,
}
impl Pair {
    pub(crate) fn new(s: GameState) -> Self {
        let native = FastActorSessionV1::from_genesis_wave_v69_fixture_state(s.clone());
        let own = GenesisWaveSessionV69::from_session(
            FastActorSessionV1::from_genesis_wave_v69_fixture_state(s),
        )
        .unwrap();
        let pair = Self { own, native };
        pair.equal();
        pair
    }
    pub(crate) fn equal(&self) {
        assert_eq!(
            self.own.fixture_state_v69(),
            self.native.genesis_wave_fixture_state()
        );
        assert_eq!(
            self.own.fixture_counts_v69(),
            (
                self.native.policy_step_count(),
                self.native.physical_decision_count()
            )
        );
        assert_eq!(self.own.response(), self.native.current_response());
    }
    pub(crate) fn act(&mut self, choose: impl Fn(&A) -> bool) {
        let d = self.own.decision_v69().unwrap();
        let i = d
            .actions()
            .iter()
            .position(choose)
            .unwrap_or_else(|| panic!("actual Wave action absent: {:?}", d.actions()));
        assert_eq!(
            d.binding.original,
            self.native.current.as_ref().unwrap().candidates
        );
        let raw = d.fixture_original_index_v69(i as u32).unwrap();
        let e = d.expected();
        assert_eq!(
            self.own.step_v69(&d, i as u32).unwrap(),
            self.native.step(e.episode_id, e.step, raw).unwrap()
        );
        self.equal();
    }
    pub(crate) fn quiet(&mut self) {
        let d = self.own.decision_v69().unwrap();
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
    pub(crate) fn cast(&mut self, wave: ObjectId, p: PlayerId, x: u16) {
        let taps = crate::effect::genesis_wave_v1::tests::tapped(self.own.fixture_state_v69(), p);
        self.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==wave.0));
        self.act(|a|matches!(a,A::ChooseEffectOption{source,option_index,..}if source.arena_id==wave.0&&*option_index==x) || matches!(a,A::ChooseFullRangeCastXValueV59{source,x_value,..}if source.arena_id==wave.0&&*x_value==x));
        let s = self.own.fixture_state_v69();
        assert!(s.engine.pending_cast.is_none());
        assert_eq!(s.players[p.index()].mana_pool, [0; 6]);
        assert_eq!(
            crate::effect::genesis_wave_v1::tests::tapped(s, p) - taps,
            usize::from(x) + 3
        );
        let row = s.stack.iter().find(|r| r.source == wave).unwrap();
        assert_eq!((row.controller, row.v4.x_value), (p, x));
        assert!(row.v4.source_contract.is_some());
        assert!(s
            .engine
            .event_history
            .iter()
            .any(|e| matches!(e,crate::event::CommittedEvent::SpellCast{spell,..}if *spell==wave)));
    }
    pub(crate) fn to_subset(&mut self) {
        for _ in 0..32 {
            if self
                .own
                .decision_v69()
                .unwrap()
                .observation()
                .genesis_wave
                .waves
                .iter()
                .any(|w| w.stage == crate::genesis_wave_public_v69::StageV69::Subset)
            {
                return;
            }
            self.quiet();
        }
        panic!("actual subset prompt absent")
    }
    pub(crate) fn restore(&mut self) {
        let d = self.own.decision_v69().unwrap();
        let save = self.own.snapshot_v69();
        let raw = self.native.snapshot_v1();
        let world = self.own.fixture_state_v69().clone();
        let counts = self.own.fixture_counts_v69();
        let decoded: GameState =
            serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        assert_eq!(decoded, world);
        assert_eq!(decoded.state_hash(), world.state_hash());
        crate::genesis_wave_observation_v69::validate_state(&decoded).unwrap();
        let value = d.candidates_v69();
        assert_eq!(
            serde_json::from_slice::<crate::genesis_wave_candidates_v69::GenesisWaveCandidatesV69>(
                &serde_json::to_vec(value).unwrap()
            )
            .unwrap(),
            *value
        );
        self.quiet();
        self.own.restore_v69(&save).unwrap();
        self.native.genesis_wave_restore_v69(&raw).unwrap();
        self.equal();
        assert_eq!(self.own.fixture_state_v69(), &world);
        assert_eq!(self.own.fixture_counts_v69(), counts);
        assert!(self.own.validate_decision_v69(&d).is_err());
        assert_eq!(self.own.decision_v69().unwrap().candidates_v69(), value);
    }
}
#[test]
fn genuine_paid_wave_owner_original_indices_subset_aura_remainder_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, wave, host) = ready(p, "Island");
        let mut pair = Pair::new(s);
        pair.cast(wave, p, 4);
        pair.restore();
        pair.to_subset();
        pair.restore();
        let d = pair.own.decision_v69().unwrap();
        let w = &d.observation().genesis_wave.waves[0];
        assert_eq!(w.x_value, 4);
        assert_eq!(w.revealed.len(), 4);
        assert_eq!(w.revealed.iter().filter(|r| r.eligible).count(), 3);
        for name in ["Forest", "Pacifism"] {
            let id = card_def::card_id_by_name(name).unwrap();
            pair.act(|a|matches!(a,A::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.card_db_id==id&&object.zone==Zone::Library));
        }
        pair.act(|a| matches!(a, A::FinishEffectSelection { .. }));
        assert!(matches!(
            pair.own
                .decision_v69()
                .unwrap()
                .observation()
                .genesis_wave
                .waves[0]
                .stage,
            crate::genesis_wave_public_v69::StageV69::AuraHost { .. }
        ));
        pair.restore();
        pair.act(|a|matches!(a,A::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==host.0));
        for _ in 0..32 {
            if pair
                .own
                .decision_v69()
                .unwrap()
                .observation()
                .genesis_wave
                .waves[0]
                .stage
                == crate::genesis_wave_public_v69::StageV69::Finished
            {
                break;
            }
            pair.quiet();
        }
        let d = pair.own.decision_v69().unwrap();
        let w = &d.observation().genesis_wave.waves[0];
        assert_eq!(w.stage, crate::genesis_wave_public_v69::StageV69::Finished);
        assert_eq!(w.selected_indices, vec![0, 3]);
        assert_eq!(w.attachments.len(), 1);
        assert_eq!(w.attachments[0].original_host.arena_id, host.0);
        assert_eq!(w.spell_departure, Some(Zone::Graveyard));
        assert_eq!(
            pair.own.fixture_state_v69().objects.get(wave).zone,
            Zone::Graveyard
        );
        pair.restore();
    }
}
#[test]
fn genuine_paid_wave_owner_foreign_forged_and_callback_failure_are_readonly_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, wave, _) = ready(p, "Island");
        let mut pair = Pair::new(s);
        pair.cast(wave, p, 4);
        pair.to_subset();
        let d = pair.own.decision_v69().unwrap();
        let before = pair.own.fixture_state_v69().clone();
        let counts = pair.own.fixture_counts_v69();
        let mut foreign = Pair::new(before.clone());
        assert!(foreign.own.step_v69(&d, 0).is_err());
        assert!(pair.own.restore_v69(&foreign.own.snapshot_v69()).is_err());
        let mut forged = d.clone();
        forged.binding.projection.observation.genesis_wave.waves[0].x_value = 3;
        assert!(pair.own.step_v69(&forged, 0).is_err());
        assert!(pair.own.step_v69(&d, u32::MAX).is_err());
        let result = pair.own.step_with_current_check(
            &d,
            0,
            |_, _| Err("closed callback rejection"),
            |_| "binding",
            |_| "native",
        );
        assert_eq!(result, Err("closed callback rejection"));
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: Result<FastActorResponseV1, RlContractError> = pair.own.step_with_current_check(
                &d,
                0,
                |_, _| panic!("closed callback unwind"),
                |e| e,
                |e| e,
            );
        }));
        assert!(panic.is_err());
        assert_eq!(pair.own.fixture_state_v69(), &before);
        assert_eq!(pair.own.fixture_counts_v69(), counts);
        pair.equal();
        assert!(crate::rl_session::EndTurnSessionV68::from_session(pair.native.clone()).is_err());
    }
}
#[test]
fn genuine_paid_wave_owner_zero_subset_and_entry_color_keep_native_batch_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        for x in [0, 4] {
            let (s, wave, _) = ready(p, "Island");
            let mut pair = Pair::new(s);
            pair.cast(wave, p, x);
            pair.to_subset();
            pair.act(|a| matches!(a, A::FinishEffectSelection { .. }));
            for _ in 0..32 {
                if pair
                    .own
                    .decision_v69()
                    .unwrap()
                    .observation()
                    .genesis_wave
                    .waves[0]
                    .stage
                    == crate::genesis_wave_public_v69::StageV69::Finished
                {
                    break;
                }
                pair.quiet();
            }
            let d = pair.own.decision_v69().unwrap();
            assert_eq!(
                d.observation().genesis_wave.waves[0].stage,
                crate::genesis_wave_public_v69::StageV69::Finished
            );
            assert!(d.observation().genesis_wave.waves[0]
                .selected_indices
                .is_empty());
            pair.restore();
        }
        let mut s = crate::effect::genesis_wave_v1::tests::ready(
            p,
            &["Heraldic Banner", "Llanowar Elves"],
            8,
            "Island",
        );
        let wave = put(&mut s, p, "Genesis Wave", Zone::Hand);
        let mut pair = Pair::new(s);
        pair.cast(wave, p, 3);
        pair.to_subset();
        for name in ["Heraldic Banner", "Llanowar Elves"] {
            let id = card_def::card_id_by_name(name).unwrap();
            pair.act(|a|matches!(a,A::ChooseEffectTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.card_db_id==id));
        }
        // Selecting the complete issued maximum can finalize automatically.
        if pair
            .own
            .decision_v69()
            .unwrap()
            .actions()
            .iter()
            .any(|a| matches!(a, A::FinishEffectSelection { .. }))
        {
            pair.act(|a| matches!(a, A::FinishEffectSelection { .. }));
        }
        let d = pair.own.decision_v69().unwrap();
        assert_eq!(
            d.observation().genesis_wave.waves[0].stage,
            crate::genesis_wave_public_v69::StageV69::EntryChoices
        );
        assert_eq!(
            d.observation().genesis_wave.waves[0].selected_indices,
            vec![0, 1]
        );
        pair.restore();
        pair.act(|a| {
            matches!(
                a,
                A::ChooseEntryColorV36 {
                    color: crate::mana::ManaColor::W,
                    ..
                }
            )
        });
        let s = pair.own.fixture_state_v69();
        assert!(s.players[p.index()]
            .battlefield
            .iter()
            .any(|id| s.objects.get(*id).card_def
                == card_def::card_id_by_name("Heraldic Banner").unwrap()));
        pair.equal();
    }
}

#[test]
fn genuine_paid_wave_owner_counter_response_preserves_paid_x_and_never_reveals_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, wave, _) = ready(p, "Island");
        let counter = put(&mut s, p.opponent(), "Counterspell", Zone::Hand);
        let mut pair = Pair::new(s);
        pair.cast(wave, p, 4);
        pair.act(|a| matches!(a, A::Pass { .. }));
        pair.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==counter.0));
        pair.act(|a|matches!(a,A::ChooseTarget{target:crate::rl::TargetRefV1::Object{object},..}if object.arena_id==wave.0));
        let s = pair.own.fixture_state_v69();
        assert!(s.engine.pending_cast.is_none());
        assert_eq!(s.players[p.opponent().index()].mana_pool, [0; 6]);
        assert_eq!(
            crate::effect::genesis_wave_v1::tests::tapped(s, p.opponent()),
            2
        );
        assert!(s
            .stack
            .iter()
            .any(|r| r.source == wave && r.v4.x_value == 4 && r.controller == p));
        pair.restore();
        for _ in 0..20 {
            if pair.own.fixture_state_v69().objects.get(wave).zone == Zone::Graveyard {
                break;
            }
            pair.quiet();
        }
        assert_eq!(
            pair.own.fixture_state_v69().objects.get(wave).zone,
            Zone::Graveyard
        );
        assert!(pair
            .own
            .decision_v69()
            .unwrap()
            .observation()
            .genesis_wave
            .waves
            .is_empty());
        assert!(!pair
            .own
            .fixture_state_v69()
            .engine
            .event_history
            .has_any(&[crate::event_history_v1::EventKindV1::GenesisWaveProgressV1]));
        pair.equal();
    }
}
