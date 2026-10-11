use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::Zone;

fn basic(actor: PlayerId, hidden: &str) -> crate::state::GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    state
}
#[track_caller]
fn expected(session: &HumanJuggernautSessionV25) -> FastActorDecisionV1 {
    let response = session.current_response();
    let FastActorResponseV1::Decision(decision) = response else {
        panic!("real decision, observed {response:?}");
    };
    decision
}
fn action(prompt: &HumanDecisionJuggernautV25, prefix: &str) -> HumanActionRequestV1 {
    let row = prompt
        .actions
        .iter()
        .find(|row| row.label.starts_with(prefix))
        .unwrap();
    HumanActionRequestV1 {
        prompt_seq: prompt.prompt_seq,
        action_index: row.action_index,
    }
}
fn safe(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, child) in fields {
                assert!(!matches!(
                    key.as_str(),
                    "arena_id"
                        | "zone_change_count"
                        | "event_history"
                        | "event_index"
                        | "card_db_id"
                        | "binding"
                        | "engine_indexes"
                        | "projection_sha256"
                        | "producer_sha256"
                ));
                safe(child);
            }
        }
        serde_json::Value::Array(rows) => {
            for row in rows {
                safe(row);
            }
        }
        _ => {}
    }
}
#[test]
fn registered_ordinary_predecessor_menu_privacy_submit_and_retry_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let base = FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Lightning Bolt"));
        let old = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let mut session = HumanJuggernautSessionV25::from_session(base).unwrap();
        let current = expected(&session);
        let prior = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&old, current)
            .unwrap();
        let mut projector = HumanJuggernautProjectorV25::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, prior.actions);
        assert_eq!(prompt.state.public, prior.state.public);
        assert_eq!(prompt.state.extensions.predecessor, prior.state.extensions);
        assert!(prompt.state.extensions.combat_rules.is_empty());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = HumanJuggernautSessionV25::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            HumanJuggernautProjectorV25::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        let request = action(&prompt, "Play Forest");
        let receipt = projector.submit(&mut session, request.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(projector.submit(&mut session, request).unwrap(), receipt);
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
    }
}
#[test]
fn registered_owning_restore_foreign_owner_generation_and_old_future_requests_refuse_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut session = HumanJuggernautSessionV25::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Lightning Bolt")),
        )
        .unwrap();
        let mut projector = HumanJuggernautProjectorV25::new(actor.into());
        let first = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let snapshot = session.snapshot_v25();
        let paid = action(&first, "Play Forest");
        projector.submit(&mut session, paid.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_request = action(&future, "Pass priority");
        session.restore_v25(&snapshot).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, paid),
            Err(Error::StaleDecision)
        );
        assert_eq!(
            projector.submit(&mut session, future_request),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let refreshed = projector
            .project_current(&session, expected(&session))
            .unwrap();
        assert!(refreshed.prompt_seq > future.prompt_seq);
        let foreign = HumanJuggernautSessionV25::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            session.restore_v25(&foreign.snapshot_v25()),
            Err(Error::StaleDecision)
        );
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v25(&snapshot),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[test]
#[ignore = "requires genuine root-owned Juggernaut registration; source-only fixture"]
fn actual_juggernaut_combat_scan_public_frozen_refusal_submit_and_stale_facts_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = basic(actor, "Lightning Bolt");
        let source = put(&mut state, actor, "Juggernaut", Zone::Battlefield);
        // A mature board boundary. Printed casting and natural sickness are
        // independently exercised by fdn_juggernaut_v1's genuine cast fixture.
        state.objects.get_mut(source).summoning_sick = false;
        state.step = crate::state::Step::DeclareAttackers;
        let before = serde_json::to_vec(&state).unwrap();
        assert!(crate::rl::observe_policy_v8(
            &state,
            &crate::policy_surface_v5::PolicySurfaceV5::new_for_session(),
            actor,
            0,
            0,
            0,
            1
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        let manifest = crate::juggernaut_public_v1::observe_v1(
            &state,
            &crate::policy_surface_v5::PolicySurfaceV5::new_for_session(),
            actor,
            0,
            0,
            0,
            1,
        )
        .unwrap();
        assert_eq!(manifest.roots.len(), 2);
        let selector = crate::rules_meaning_juggernaut_v1::live_selector().unwrap();
        for (index, root) in [
            crate::rules_meaning_juggernaut_v1::RootV1::MustAttackEachCombat,
            crate::rules_meaning_juggernaut_v1::RootV1::CannotBeBlockedByWalls,
        ]
        .into_iter()
        .enumerate()
        {
            let selected = crate::rules_meaning_juggernaut_v1::meaning(
                &selector,
                crate::card_def::card_id_by_name("Juggernaut").unwrap(),
                root,
            )
            .unwrap();
            assert_eq!(manifest.roots[index].card_id, selected.card_id);
            assert_eq!(usize::from(manifest.roots[index].ability_index), index);
            assert_eq!(manifest.roots[index].root_sha256, selected.root_sha256);
        }
        let base = FastActorSessionV1::from_juggernaut_v25_fixture_state(state);
        let mut session = HumanJuggernautSessionV25::from_session(base).unwrap();
        let current = expected(&session);
        let mut projector = HumanJuggernautProjectorV25::new(actor.into());
        let prompt = projector.project_current(&session, current).unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.state.extensions.combat_rules.len(), 1);
        let rule = &prompt.state.extensions.combat_rules[0];
        assert!(rule.required_in_current_declaration && rule.wall_restriction_active);
        assert!(!prompt
            .actions
            .iter()
            .any(|action| action.label.starts_with("Do not attack with Juggernaut")));
        let request = action(&prompt, "Attack with Juggernaut");
        let saved = session.snapshot_v25();
        session
            .inner
            .juggernaut_fixture_state_mut()
            .objects
            .get_mut(source)
            .tapped = true;
        let unchanged = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, request.clone()),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), unchanged);
        session.restore_v25(&saved).unwrap();
        assert_eq!(
            projector.submit(&mut session, request),
            Err(Error::StaleDecision)
        );
        let refreshed = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let accepted = action(&refreshed, "Attack with Juggernaut");
        let receipt = projector.submit(&mut session, accepted.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(projector.submit(&mut session, accepted).unwrap(), receipt);
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
    }
}

// Future actual printed groups. These require the genuine Juggernaut definition;
// they are compiled source preparation, not registered gameplay qualification.

fn ward_state(actor: PlayerId) -> crate::state::GameState {
    use crate::engine::{self, Action, Decision};
    use crate::mana::ManaColor;
    use crate::state::Target;
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    let terror = put(
        &mut state,
        actor.opponent(),
        "Tolarian Terror",
        Zone::Battlefield,
    );
    let spells =
        ["Lightning Bolt", "Lightning Bolt"].map(|name| put(&mut state, actor, name, Zone::Hand));
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 8;
    for spell in spells {
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        engine::step(&mut state, Action::ChooseTarget(Target::Object(terror))).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
    }
    state
}
#[test]
fn inherited_registered_generic_ward_menu_is_preserved_and_submission_replays() {
    for (actor, pay) in [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .flat_map(|actor| [false, true].map(|pay| (actor, pay)))
    {
        let mut state = ward_state(actor);
        crate::policy_observation_v6::tests::reach_ward_payment(&mut state);
        let base = FastActorSessionV1::from_human_v8_fixture_state(state);
        let previous = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let mut session = HumanJuggernautSessionV25::from_session(base).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanJuggernautProjectorV25::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert_eq!(prompt.actions, old.actions);
        safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = action(&prompt, prefix);
        let snapshot = session.snapshot_v25();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v25(&snapshot).unwrap();
        let mut replay = HumanJuggernautProjectorV25::new(e.acting_player);
        assert_eq!(
            replay
                .project_current(&session, expected(&session))
                .unwrap(),
            prompt
        );
        replay.submit(&mut session, input).unwrap();
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
    }
}
