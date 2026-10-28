use super::*;
use crate::{
    ids::PlayerId,
    rl::ActionSemanticV1 as A,
    rl_session::muldrotha_v71::tests::{ready, Pair},
};
#[test]
fn paid_human_original_permission_handles_restore_and_native_parity_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (world, _, card) = ready(p, "Island");
        let mut pair = Pair::new(world);
        pair.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==card.0));
        let world = pair.own.fixture_state_v71().clone();
        let mut native = FastActorSessionV1::from_muldrotha_v71_fixture_state(world.clone());
        let mut own = HumanMuldrothaSessionV71::from_session(
            FastActorSessionV1::from_muldrotha_v71_fixture_state(world),
            p.into(),
        )
        .unwrap();
        let mut human = HumanMuldrothaProjectorV71::new(p.into());
        let expected = own.inner.decision_v71().unwrap().expected();
        let prompt = human.project_current(&own, expected).unwrap();
        assert_eq!(
            prompt.state.muldrotha.plays[0].stage,
            crate::muldrotha_public_v71::StageV71::PermissionChoice
        );
        assert_eq!(prompt.state.muldrotha.budgets[0].unused.len(), 5);
        let text = serde_json::to_string(&prompt).unwrap();
        for private in [
            "arena_id",
            "history_index",
            "native_stack_item_id",
            "private_world",
        ] {
            assert!(!text.contains(private));
        }
        let save = own.snapshot_v71();
        let raw = native.snapshot_v1();
        let request = HumanActionRequestV1 {
            prompt_seq: prompt.prompt_seq,
            action_index: 0,
        };
        let d = own.inner.decision_v71().unwrap();
        let index = d.fixture_original_index_v71(0).unwrap();
        human.submit(&mut own, request.clone()).unwrap();
        native
            .step(expected.episode_id, expected.step, index)
            .unwrap();
        assert_eq!(
            own.inner.fixture_state_v71(),
            native.muldrotha_fixture_state()
        );
        own.restore_v71(&save).unwrap();
        native.restore_v1(&raw);
        let before = own.inner.fixture_state_v71().clone();
        assert!(human.submit(&mut own, request).is_err());
        assert_eq!(own.inner.fixture_state_v71(), &before);
        let expected = own.inner.decision_v71().unwrap().expected();
        let fresh = human.project_current(&own, expected).unwrap();
        assert!(fresh.prompt_seq > prompt.prompt_seq);
        assert_eq!(
            fresh.state.muldrotha.plays[0].permissions,
            prompt.state.muldrotha.plays[0].permissions
        );
    }
}

#[test]
fn paid_human_sorted_artifact_first_keeps_native_creature_zero_artifact_one_both_seats() {
    use crate::{
        card_def::CardType, engine::muldrotha_v1::tests as native,
        policy_observation_v6::tests::put, state::Zone,
    };
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut world = native::ready(p, "Island");
        native::paid(&mut world, p);
        let card = put(&mut world, p, "Burnished Hart", Zone::Graveyard);
        let mut pair = Pair::new(world);
        pair.act(|a| matches!(a,A::CastSpell{source,..}if source.arena_id==card.0));
        let mut own = HumanMuldrothaSessionV71::from_session(
            FastActorSessionV1::from_muldrotha_v71_fixture_state(
                pair.own.fixture_state_v71().clone(),
            ),
            p.into(),
        )
        .unwrap();
        let mut human = HumanMuldrothaProjectorV71::new(p.into());
        let expected = own.inner.decision_v71().unwrap().expected();
        let prompt = human.project_current(&own, expected).unwrap();
        assert_eq!(
            prompt
                .action_meanings
                .iter()
                .map(|a| a.muldrotha_permission.as_ref().unwrap().menu_index)
                .collect::<Vec<_>>(),
            [1, 0]
        );
        assert!(prompt.actions[0]
            .label
            .starts_with("Use the Artifact permission"));
        assert!(prompt.actions[1]
            .label
            .starts_with("Use the Creature permission"));
        let before = native::tapped(own.inner.fixture_state_v71(), p);
        human
            .submit(
                &mut own,
                HumanActionRequestV1 {
                    prompt_seq: prompt.prompt_seq,
                    action_index: 0,
                },
            )
            .unwrap();
        assert_eq!(native::tapped(own.inner.fixture_state_v71(), p) - before, 3);
        assert_eq!(
            own.inner
                .decision_v71()
                .unwrap()
                .observation()
                .muldrotha
                .budgets[0]
                .spent,
            [CardType::Artifact]
        );
    }
}
