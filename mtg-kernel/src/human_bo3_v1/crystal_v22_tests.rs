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
fn expected(session: &HumanCrystalSessionV22) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(decision) = session.current_response() else {
        panic!("real decision");
    };
    decision
}
fn action(prompt: &HumanDecisionCrystalV22, prefix: &str) -> HumanActionRequestV1 {
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
        let mut session = HumanCrystalSessionV22::from_session(base).unwrap();
        let current = expected(&session);
        let prior = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&old, current)
            .unwrap();
        let mut projector = HumanCrystalProjectorV22::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, prior.actions);
        assert_eq!(prompt.state.public, prior.state.public);
        assert_eq!(prompt.state.extensions.predecessor, prior.state.extensions);
        assert!(prompt.state.extensions.static_sources.is_empty());
        assert!(prompt.state.extensions.prevention_menu.is_none());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = HumanCrystalSessionV22::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            HumanCrystalProjectorV22::new(actor.into())
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
        let mut session = HumanCrystalSessionV22::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Lightning Bolt")),
        )
        .unwrap();
        let mut projector = HumanCrystalProjectorV22::new(actor.into());
        let first = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let snapshot = session.snapshot_v22();
        let paid = action(&first, "Play Forest");
        projector.submit(&mut session, paid.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_request = action(&future, "Pass priority");
        session.restore_v22(&snapshot).unwrap();
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
        let foreign = HumanCrystalSessionV22::from_session(
            FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Island")),
        )
        .unwrap();
        assert_eq!(
            session.restore_v22(&foreign.snapshot_v22()),
            Err(Error::StaleDecision)
        );
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v22(&snapshot),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
    }
}

// Future actual printed groups. These require the genuine Crystal definition;
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
        let mut session = HumanCrystalSessionV22::from_session(base).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanCrystalProjectorV22::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert_eq!(prompt.actions, old.actions);
        safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = action(&prompt, prefix);
        let snapshot = session.snapshot_v22();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v22(&snapshot).unwrap();
        let mut replay = HumanCrystalProjectorV22::new(e.acting_player);
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

#[test]
#[ignore = "Crystal Barricade is unregistered; explicit future printed qualification"]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn future_printed_bolt_mixed_menu_public_human_submit_and_owning_restore_both_seats() {
    use crate::engine::{self, Action, Decision};
    use crate::event::{ActiveReplacement, ReplacementEffectKind};
    use crate::mana::ManaColor;
    use crate::state::Target;
    for actor in [PlayerId::P0, PlayerId::P1] {
        let caster = actor.opponent();
        let mut state = ready_state();
        state.active_player = caster;
        state.priority_player = caster;
        // Physical static setup. Root ownership is still the real registered Bolt.
        let wall = put(&mut state, actor, "Crystal Barricade", Zone::Battlefield);
        let target = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let bolt = put(&mut state, caster, "Lightning Bolt", Zone::Hand);
        state.players[caster.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        state.engine.next_replacement_id = 1;
        state.engine.active_replacements.push(ActiveReplacement {
            id: 1,
            source: wall,
            kind: ReplacementEffectKind::PreventNextDamage {
                target: Target::Object(target),
                remaining: 5,
            },
        });
        assert!(
            matches!(engine::advance_until_decision(&mut state), Decision::CastSpellOrPass { player, .. } if player == caster)
        );
        engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::ChooseTargets { .. }
        ));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        let _ = engine::advance_until_decision(&mut state); // Genuine finalization.
        for _ in 0..8 {
            let d = engine::advance_until_decision(&mut state);
            if matches!(d, Decision::ChooseEffectOption { player, .. } if player == actor) {
                break;
            }
            assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::ChooseEffectOption { player, .. } if player == actor));
        assert!(crate::rl::observe_policy_v8(
            &state,
            &crate::policy_surface_v5::PolicySurfaceV5::new(),
            actor,
            0,
            0,
            0,
            1
        )
        .is_err());
        let base = FastActorSessionV1::from_crystal_v22_fixture_state(state);
        let mut session = HumanCrystalSessionV22::from_session(base).unwrap();
        let current = expected(&session);
        let public = session.public_observation(current).unwrap();
        assert_eq!(
            public.prevention_menu.as_ref().unwrap().chooser,
            actor.into()
        );
        assert_eq!(public.prevention_menu.as_ref().unwrap().options.len(), 2);
        let mut projector = HumanCrystalProjectorV22::new(actor.into());
        let prompt = projector.project_current(&session, current).unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        let request = action(&prompt, "Apply Crystal Barricade");
        let snapshot = session.snapshot_v22();
        let receipt = projector.submit(&mut session, request.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(session.inner.state.objects.get(target).damage, 0);
        assert!(matches!(
            session.inner.state.engine.active_replacements[0].kind,
            ReplacementEffectKind::PreventNextDamage { remaining: 5, .. }
        ));
        assert_eq!(
            projector.submit(&mut session, request.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v22(&snapshot).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, request.clone()),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let mut replay = HumanCrystalProjectorV22::new(actor.into());
        let p = replay
            .project_current(&session, expected(&session))
            .unwrap();
        assert_eq!(p, prompt);
        replay.submit(&mut session, request).unwrap();
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
    }
}
