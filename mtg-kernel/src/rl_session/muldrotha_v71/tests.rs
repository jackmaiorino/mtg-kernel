//! Paid native backend/owner comparisons. Initial graveyard operands are setup.
use super::*;
use crate::{
    engine::muldrotha_v1::tests as native, ids::ObjectId, policy_observation_v6::tests::put,
    rl::ActionSemanticV1 as A, state::Zone,
};
pub(crate) fn ready(p: PlayerId, hidden: &str) -> (GameState, ObjectId, ObjectId) {
    let mut s = native::ready(p, hidden);
    let source = native::paid(&mut s, p);
    let card = put(&mut s, p, "Llanowar Elves", Zone::Graveyard);
    (s, source, card)
}
pub(crate) struct Pair {
    pub(crate) own: MuldrothaSessionV71,
    pub(crate) native: FastActorSessionV1,
}
impl Pair {
    pub(crate) fn new(s: GameState) -> Self {
        let native = FastActorSessionV1::from_muldrotha_v71_fixture_state(s.clone());
        let own = MuldrothaSessionV71::from_session(
            FastActorSessionV1::from_muldrotha_v71_fixture_state(s),
        )
        .unwrap();
        let p = Self { own, native };
        p.equal();
        p
    }
    pub(crate) fn equal(&self) {
        assert_eq!(
            self.own.fixture_state_v71(),
            self.native.muldrotha_fixture_state()
        );
        assert_eq!(self.own.response(), self.native.current_response());
        assert_eq!(
            self.own.fixture_counts_v71(),
            (
                self.native.policy_step_count(),
                self.native.physical_decision_count()
            )
        );
    }
    pub(crate) fn act(&mut self, choose: impl Fn(&A) -> bool) {
        let d = self.own.decision_v71().unwrap();
        let i = d
            .actions()
            .iter()
            .position(choose)
            .unwrap_or_else(|| panic!("absent V71 action: {:?}", d.actions()));
        let original = d.fixture_original_index_v71(i as u32).unwrap();
        let expected = d.expected();
        assert_eq!(
            self.own.step_v71(&d, i as u32).unwrap(),
            self.native
                .step(expected.episode_id, expected.step, original)
                .unwrap()
        );
        self.equal();
    }
    pub(crate) fn permission(&mut self, kind: crate::card_def::CardType) {
        let d = self.own.decision_v71().unwrap();
        let option = d
            .candidates_v71()
            .candidates
            .iter()
            .find_map(|c| {
                c.muldrotha_permission
                    .as_ref()
                    .filter(|a| {
                        a.permission
                            .as_ref()
                            .is_some_and(|p| p.permanent_type == kind)
                    })
                    .map(|a| a.menu_index)
            })
            .unwrap();
        self.act(|a| matches!(a,A::ChooseEffectOption{option_index,..}if *option_index==option));
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
    pub(crate) fn finish(&mut self) {
        for _ in 0..80 {
            if self.own.fixture_state_v71().stack.is_empty()
                && self.own.fixture_state_v71().engine.pending_cast.is_none()
            {
                return;
            }
            self.quiet();
        }
        panic!("V71 stack failed to settle")
    }
}
#[test]
fn paid_permission_original_source_payment_and_pending_snapshot_native_parity_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, source, card) = ready(p, "Island");
        let before = native::tapped(&s, p);
        let mut pair = Pair::new(s);
        pair.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==card.0));
        let d = pair.own.decision_v71().unwrap();
        let permission = d.candidates_v71().candidates[0]
            .muldrotha_permission
            .as_ref()
            .unwrap();
        assert_eq!(
            permission
                .permission
                .as_ref()
                .unwrap()
                .source
                .original_source
                .object,
            source.0
        );
        assert_eq!(
            permission.permission.as_ref().unwrap().permanent_type,
            crate::card_def::CardType::Creature
        );
        let save = pair.own.snapshot_v71();
        let raw = pair.native.snapshot_v1();
        pair.act(|a| {
            matches!(
                a,
                A::ChooseEffectOption {
                    option_index: 0,
                    ..
                }
            )
        });
        assert_eq!(native::tapped(pair.own.fixture_state_v71(), p) - before, 1);
        assert_eq!(
            pair.own.fixture_state_v71().players[p.index()].mana_pool,
            [0; 6]
        );
        let bound = pair.own.decision_v71().unwrap();
        let f = &bound.observation().muldrotha;
        assert_eq!(f.budgets[0].spent, [crate::card_def::CardType::Creature]);
        assert_eq!(
            f.plays[0].stage,
            crate::muldrotha_public_v71::StageV71::CastPublished
        );
        pair.own.restore_v71(&save).unwrap();
        pair.native.restore_v1(&raw);
        pair.equal();
        let before = pair.own.fixture_state_v71().clone();
        assert!(pair.own.step_v71(&d, 0).is_err());
        assert_eq!(pair.own.fixture_state_v71(), &before);
        pair.act(|a| {
            matches!(
                a,
                A::ChooseEffectOption {
                    option_index: 0,
                    ..
                }
            )
        });
        pair.finish();
        assert_eq!(
            pair.own.fixture_state_v71().objects.get(card).zone,
            Zone::Battlefield
        );
    }
}
#[test]
fn paid_full_packet_foreign_choice_reorder_and_erasure_refuse_readonly_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (s, _, card) = ready(p, "Island");
        let mut pair = Pair::new(s.clone());
        let mut foreign = Pair::new(s);
        pair.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==card.0));
        foreign.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==card.0));
        let d = pair.own.decision_v71().unwrap();
        let before = pair.own.fixture_state_v71().clone();
        assert!(foreign.own.step_v71(&d, 0).is_err());
        let mut bad = d.clone();
        bad.binding.projection.candidates[0].muldrotha_permission = None;
        assert!(pair.own.step_v71(&bad, 0).is_err());
        let mut bad = d.clone();
        bad.binding.original.clear();
        assert!(pair.own.step_v71(&bad, 0).is_err());
        let mut bad = d.clone();
        bad.binding.public_to_original[0] = usize::MAX;
        assert!(pair.own.step_v71(&bad, 0).is_err());
        assert!(pair.own.step_v71(&d, u32::MAX).is_err());
        assert_eq!(pair.own.fixture_state_v71(), &before);
    }
}
#[test]
fn paid_complete_projection_keeps_hidden_worlds_equal_and_legacy_v70_refuses_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut all = vec![];
        for hidden in ["Island", "Mountain"] {
            let (s, _, card) = ready(p, hidden);
            assert!(crate::rl_session::VivienSessionV70::from_session(
                FastActorSessionV1::from_muldrotha_v71_fixture_state(s.clone())
            )
            .is_err());
            let mut pair = Pair::new(s);
            pair.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==card.0));
            let d = pair.own.decision_v71().unwrap();
            all.push(d.candidates_v71().clone());
            let text = serde_json::to_string(d.candidates_v71()).unwrap();
            for forbidden in [
                "options_history_index",
                "native_stack_item_id",
                "private_world",
                "remaining_library",
            ] {
                assert!(!text.contains(forbidden));
            }
            assert_eq!(d.observation().muldrotha.viewer, p.into());
            assert_eq!(
                d.observation().vivien.schema,
                crate::vivien_public_v70::SCHEMA
            );
        }
        assert_eq!(all[0], all[1]);
    }
}

#[test]
fn paid_land_permission_and_original_native_index_have_no_stack_or_mana_charge_both_seats() {
    use crate::{card_def::CardType, muldrotha_public_v71::StageV71};
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut world = native::ready(p, "Island");
        native::paid(&mut world, p);
        let land = put(&mut world, p, "Forest", Zone::Graveyard);
        let before = native::tapped(&world, p);
        let mut pair = Pair::new(world);
        pair.act(|a| matches!(a,A::PlayLand{source,..}if source.arena_id==land.0));
        let d = pair.own.decision_v71().unwrap();
        assert_eq!(d.observation().muldrotha.plays[0].permissions.len(), 1);
        assert_eq!(
            d.observation().muldrotha.plays[0].permissions[0].permanent_type,
            CardType::Land
        );
        pair.permission(CardType::Land);
        let d = pair.own.decision_v71().unwrap();
        assert_eq!(
            d.observation().muldrotha.plays[0].stage,
            StageV71::PlayedLand
        );
        assert_eq!(d.observation().muldrotha.budgets[0].spent, [CardType::Land]);
        assert!(pair.own.fixture_state_v71().stack.is_empty());
        assert_eq!(
            pair.own.fixture_state_v71().players[p.index()].lands_played_this_turn,
            1
        );
        assert_eq!(native::tapped(pair.own.fixture_state_v71(), p), before);
        assert!(!d.actions().iter().any(|a| matches!(a, A::PlayLand { .. })));
    }
}
#[test]
fn paid_multitype_menu_spends_only_selected_budget_and_native_payment_both_seats() {
    use crate::card_def::CardType;
    for p in [PlayerId::P0, PlayerId::P1] {
        for kind in [CardType::Creature, CardType::Artifact] {
            let mut world = native::ready(p, "Island");
            native::paid(&mut world, p);
            let first = put(&mut world, p, "Burnished Hart", Zone::Graveyard);
            let second = put(&mut world, p, "Burnished Hart", Zone::Graveyard);
            let before = native::tapped(&world, p);
            let mut pair = Pair::new(world);
            pair.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==first.0));
            let d = pair.own.decision_v71().unwrap();
            assert_eq!(
                d.observation().muldrotha.plays[0]
                    .permissions
                    .iter()
                    .map(|v| v.permanent_type)
                    .collect::<Vec<_>>(),
                [CardType::Creature, CardType::Artifact]
            );
            pair.permission(kind);
            assert_eq!(native::tapped(pair.own.fixture_state_v71(), p) - before, 3);
            pair.finish();
            pair.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==second.0));
            let d = pair.own.decision_v71().unwrap();
            let play = d.observation().muldrotha.plays.last().unwrap();
            assert_eq!(play.permissions.len(), 1);
            assert_ne!(play.permissions[0].permanent_type, kind);
            let other = play.permissions[0].permanent_type;
            pair.permission(other);
            pair.finish();
            assert_eq!(native::tapped(pair.own.fixture_state_v71(), p) - before, 6);
            let d = pair.own.decision_v71().unwrap();
            assert_eq!(
                d.observation().muldrotha.budgets[0].spent,
                [CardType::Creature, CardType::Artifact]
            );
        }
    }
}
