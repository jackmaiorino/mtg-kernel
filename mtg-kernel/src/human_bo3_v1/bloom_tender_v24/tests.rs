use super::*;
use crate::{
    ids::PlayerId,
    policy_observation_v6::tests::{put, ready_state},
    state::{GameState, Zone},
};
fn expected(session: &HumanBloomTenderSessionV24) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(e) = session.current_response() else {
        panic!("actual playable decision")
    };
    e
}
fn owned(state: GameState) -> HumanBloomTenderSessionV24 {
    HumanBloomTenderSessionV24::from_session(FastActorSessionV1::from_human_v8_fixture_state(state))
        .unwrap()
}
fn basic(actor: PlayerId, hidden: &str) -> GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    state
}
fn request(prompt: &HumanDecisionBloomTenderV24, prefix: &str) -> HumanActionRequestV1 {
    let row = prompt
        .actions
        .iter()
        .find(|a| a.label.starts_with(prefix))
        .unwrap();
    HumanActionRequestV1 {
        prompt_seq: prompt.prompt_seq,
        action_index: row.action_index,
    }
}
fn safe(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(rows) => {
            for (k, v) in rows {
                assert!(!matches!(
                    k.as_str(),
                    "arena_id"
                        | "card_db_id"
                        | "zone_change_count"
                        | "binding"
                        | "producer_sha256"
                        | "event_history"
                        | "generation"
                ));
                safe(v);
            }
        }
        serde_json::Value::Array(rows) => {
            for v in rows {
                safe(v);
            }
        }
        _ => {}
    }
}
#[test]
fn registered_prompt_parity_and_hidden_substitution_do_not_expose_authority() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut prompts = Vec::new();
        for hidden in ["Counterspell", "Duress"] {
            let session = owned(basic(actor, hidden));
            let e = expected(&session);
            let mut p = HumanBloomTenderProjectorV24::new(actor.into());
            let prompt = p.project_current(&session, e).unwrap();
            safe(&serde_json::to_value(&prompt).unwrap());
            assert!(prompt.state.extensions.mana_bundles.is_empty());
            let old = HumanDecisionProjectorV8::new(actor.into())
                .project_current(&session.inner, e)
                .unwrap();
            assert_eq!(prompt.actions, old.actions);
            assert_eq!(prompt.state.public, old.state.public);
            assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
            prompts.push(serde_json::to_vec(&prompt).unwrap());
        }
        assert_eq!(prompts[0], prompts[1]);
    }
}
#[test]
fn owning_restore_foreign_snapshot_and_prior_receipt_refuse_without_mutation() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut session = owned(basic(actor, "Counterspell"));
        let foreign = owned(basic(actor, "Counterspell"));
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            session.restore_v24(&foreign.snapshot_v24()),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let snapshot = session.snapshot_v24();
        let mut p = HumanBloomTenderProjectorV24::new(actor.into());
        let prompt = p.project_current(&session, expected(&session)).unwrap();
        let r = request(&prompt, "Play Forest");
        let receipt = p.submit(&mut session, r.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(p.submit(&mut session, r.clone()).unwrap(), receipt);
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v24(&snapshot).unwrap();
        let restored = session.inner.privileged_core_environment_hash();
        assert_eq!(p.submit(&mut session, r), Err(Error::StaleDecision));
        assert_eq!(session.inner.privileged_core_environment_hash(), restored);
        let fresh = p.project_current(&session, expected(&session)).unwrap();
        assert!(fresh.prompt_seq > prompt.prompt_seq);
        assert_eq!(
            p.submit(
                &mut session,
                HumanActionRequestV1 {
                    prompt_seq: fresh.prompt_seq,
                    action_index: u32::MAX
                }
            ),
            Err(Error::InvalidAction)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), restored);
    }
}
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
fn future_actual_cast_bundle_public_human_submit_restore_and_forgery_both_seats() {
    use crate::mana::ManaColor;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = crate::bloom_tender_mana_v1::tests::actual_source(actor);
        for name in [
            "Healer's Hawk",
            "High Fae Trickster",
            "Vengeful Bloodwitch",
            "Guttersnipe",
        ] {
            put(&mut state, actor, name, Zone::Battlefield);
        }
        let mut session = owned(state);
        let e = expected(&session);
        assert_eq!(
            HumanDecisionProjectorV8::new(actor.into()).project_current(&session.inner, e),
            Err(Error::UnsupportedPrompt)
        );
        let public = session.public_observation(e).unwrap();
        assert_eq!(public.bundles.len(), 1);
        assert_eq!(public.bundles[0].source.arena_id, source.0);
        assert_eq!(
            public.bundles[0].colors,
            vec![
                ManaColor::W,
                ManaColor::U,
                ManaColor::B,
                ManaColor::R,
                ManaColor::G
            ]
        );
        let mut p = HumanBloomTenderProjectorV24::new(actor.into());
        let prompt = p.project_current(&session, e).unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        let r = request(&prompt, "Tap Bloom Tender");
        assert!(prompt.actions[r.action_index as usize]
            .label
            .contains("{W}{U}{B}{R}{G}"));
        let before = session.inner.privileged_core_environment_hash();
        p.pending.as_mut().unwrap().public.bundles[0].colors.clear();
        assert_eq!(p.submit(&mut session, r.clone()), Err(Error::StaleDecision));
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let prompt = p.project_current(&session, e).unwrap();
        let r = request(&prompt, "Tap Bloom Tender");
        let snapshot = session.snapshot_v24();
        let receipt = p.submit(&mut session, r.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            session
                .public_observation(expected(&session))
                .unwrap()
                .predecessor
                .projection
                .surface
                .mana_pools[actor.index()],
            [1, 1, 1, 1, 1, 0]
        );
        assert_eq!(p.submit(&mut session, r.clone()).unwrap(), receipt);
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v24(&snapshot).unwrap();
        assert_eq!(p.submit(&mut session, r), Err(Error::StaleDecision));
        let prompt = p.project_current(&session, expected(&session)).unwrap();
        let r = request(&prompt, "Tap Bloom Tender");
        p.submit(&mut session, r).unwrap();
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
    }
}
