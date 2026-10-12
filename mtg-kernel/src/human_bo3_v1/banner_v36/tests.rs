use super::*;
use crate::ids::PlayerId;
use crate::policy_observation_v6::tests::{put, ready_state};
use crate::state::{GameState, Zone};

fn expected(session: &HumanBannerSessionV36) -> FastActorDecisionV1 {
    let FastActorResponseV1::Decision(expected) = session.current_response() else {
        panic!("real fixture must reach a decision");
    };
    expected
}
fn owned(state: GameState) -> HumanBannerSessionV36 {
    let base = FastActorSessionV1::from_banner_v36_fixture_state(state);
    let FastActorResponseV1::Decision(expected) = base.current_response() else {
        panic!("actual actor decision");
    };
    HumanBannerSessionV36::from_session(base, expected.acting_player).unwrap()
}

fn basic(actor: PlayerId, hidden: &str) -> GameState {
    let mut state = ready_state();
    state.active_player = actor;
    state.priority_player = actor;
    put(&mut state, actor, "Forest", Zone::Hand);
    put(&mut state, actor.opponent(), hidden, Zone::Hand);
    state
}
fn request(prompt: &HumanDecisionBannerV36, prefix: &str) -> HumanActionRequestV1 {
    let row = prompt
        .actions
        .iter()
        .find(|row| row.label.starts_with(prefix))
        .unwrap_or_else(|| panic!("missing {prefix}: {:?}", prompt.actions));
    HumanActionRequestV1 {
        prompt_seq: prompt.prompt_seq,
        action_index: row.action_index,
    }
}
fn assert_safe(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, child) in fields {
                assert!(!matches!(
                    key.as_str(),
                    "arena_id"
                        | "zone_change_count"
                        | "card_db_id"
                        | "recipient_snapshot"
                        | "event_history"
                        | "engine_indexes"
                        | "environment_revision"
                        | "producer_sha256"
                        | "projection_sha256"
                        | "binding"
                ));
                assert_safe(child);
            }
        }
        serde_json::Value::Array(rows) => {
            for row in rows {
                assert_safe(row);
            }
        }
        _ => {}
    }
}

#[test]
fn ordinary_menu_state_privacy_and_real_submit_retry_parity_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let base = FastActorSessionV1::from_human_v8_fixture_state(basic(actor, "Lightning Bolt"));
        let predecessor = HumanRaidSessionV9::from_session(base.clone()).unwrap();
        let mut session = HumanBannerSessionV36::from_session(base, actor.into()).unwrap();
        let current = expected(&session);
        let old = HumanDecisionProjectorV9::new(actor.into())
            .project_current(&predecessor, current)
            .unwrap();
        let mut projector = HumanBannerProjectorV36::new(actor.into());
        let before = session.inner.privileged_core_environment_hash();
        let prompt = projector.project_current(&session, current).unwrap();
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(prompt.actions, old.actions);
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert!(prompt.state.extensions.death_replacements.is_empty());
        assert_eq!(
            projector.project_current(&session, current).unwrap(),
            prompt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let hidden = owned(basic(actor, "Island"));
        assert_eq!(
            HumanBannerProjectorV36::new(actor.into())
                .project_current(&hidden, expected(&hidden))
                .unwrap(),
            prompt
        );
        assert_eq!(
            HumanBannerProjectorV36::new(actor.opponent().into())
                .project_current(&session, current),
            Err(Error::NotHumanTurn)
        );
        let input = request(&prompt, "Play Forest");
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(projector.submit(&mut session, input).unwrap(), receipt);
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
    }
}

#[test]
fn owning_restore_rejects_old_pending_accepted_future_and_foreign_owner_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut session = owned(basic(actor, "Lightning Bolt"));
        let mut projector = HumanBannerProjectorV36::new(actor.into());
        let prompt = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let saved = session.snapshot_v36();
        let mut stale = HumanBannerProjectorV36::new(actor.into());
        stale.project_current(&session, expected(&session)).unwrap();
        let input = request(&prompt, "Play Forest");
        projector.submit(&mut session, input.clone()).unwrap();
        let future = projector
            .project_current(&session, expected(&session))
            .unwrap();
        let future_input = request(&future, "Pass priority");
        session.restore_v36(&saved).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()),
            Err(Error::StaleDecision)
        );
        assert_eq!(stale.submit(&mut session, input), Err(Error::StaleDecision));
        assert_eq!(
            projector.submit(&mut session, future_input.clone()),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let fresh = projector
            .project_current(&session, expected(&session))
            .unwrap();
        assert!(fresh.prompt_seq > future.prompt_seq);
        projector
            .submit(&mut session, request(&fresh, "Play Forest"))
            .unwrap();
        // Recreate exactly the formerly issued future core. Authority remains
        // newer even when the same action is still a legal next choice.
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, future_input),
            Err(Error::StaleDecision)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        let foreign = owned(basic(actor, "Lightning Bolt")).snapshot_v36();
        assert_eq!(session.restore_v36(&foreign), Err(Error::StaleDecision));
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session.authority.generation = u64::MAX;
        assert_eq!(
            session.restore_v36(&saved),
            Err(Error::PromptSequenceExhausted)
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
    }
}

fn ward_state(actor: PlayerId) -> GameState {
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
        let mut session = HumanBannerSessionV36::from_session(base, actor.into()).unwrap();
        let e = expected(&session);
        let old = HumanDecisionProjectorV9::new(e.acting_player)
            .project_current(&previous, e)
            .unwrap();
        let mut projector = HumanBannerProjectorV36::new(e.acting_player);
        let prompt = projector.project_current(&session, e).unwrap();
        assert_eq!(prompt.state.public, old.state.public);
        assert_eq!(prompt.state.extensions.predecessor, old.state.extensions);
        assert_eq!(prompt.actions, old.actions);
        assert_safe(&serde_json::to_value(&prompt).unwrap());
        let prefix = if pay { "Pay" } else { "Decline to pay" };
        let input = request(&prompt, prefix);
        let snapshot = session.snapshot_v36();
        let receipt = projector.submit(&mut session, input.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, input.clone()).unwrap(),
            receipt
        );
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
        session.restore_v36(&snapshot).unwrap();
        let mut replay = HumanBannerProjectorV36::new(e.acting_player);
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
fn fixed_human_seat_has_no_opponent_public_hand_view_and_trusted_callback_binds_real_menu() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let core =
            FastActorSessionV1::from_banner_v36_fixture_state(basic(actor, "Lightning Bolt"));
        let mut session =
            HumanBannerSessionV36::from_session(core, actor.opponent().into()).unwrap();
        let current = expected(&session);
        assert_eq!(current.acting_player, actor.into());
        assert!(session.public_observation(current).is_err());
        let mut projector = HumanBannerProjectorV36::new(actor.opponent().into());
        assert_eq!(
            projector.project_current(&session, current),
            Err(Error::NotHumanTurn)
        );
        let before = session.inner.privileged_core_environment_hash();
        assert!(session
            .step_opponent_with(current, |_, actions| Ok(actions.len() as u32))
            .is_err());
        assert_eq!(session.inner.privileged_core_environment_hash(), before);
        session
            .step_opponent_with(current, |observation, actions| {
                assert_eq!(observation.acting_player, actor.into());
                let index = actions
                    .iter()
                    .position(|a| matches!(a, ActionSemanticV1::Pass { .. }))
                    .unwrap();
                Ok(index as u32)
            })
            .unwrap();
        assert_ne!(session.inner.privileged_core_environment_hash(), before);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod future_printed {
    use super::*;
    use crate::card_def::CARD_DEFS;
    use crate::engine::{self, Action, Decision};
    use crate::mana::ManaColor;
    fn ready(actor: PlayerId) -> GameState {
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries_with_starting_player_v1(
            &[forest; 16],
            &[forest; 16],
            |id| CARD_DEFS[id as usize].name.into(),
            11036,
            actor,
        );
        state.step = crate::state::Step::Main1;
        state
    }
    #[test]
    #[ignore = "requires genuine Heraldic Banner registration and native qualification"]
    fn actual_cast_full_five_choice_owner_restore_retry_anthem_mana_and_forged_color_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut core = ready(actor);
            let banner = put(&mut core, actor, "Heraldic Banner", Zone::Hand);
            let elf = put(&mut core, actor, "Llanowar Elves", Zone::Battlefield);
            let opposing = put(
                &mut core,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            core.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
            let mut session = owned(core);
            let mut projector = HumanBannerProjectorV36::new(actor.into());
            let begin = projector
                .project_current(&session, expected(&session))
                .unwrap();
            projector
                .submit(
                    &mut session,
                    request(&begin, "Begin casting Heraldic Banner"),
                )
                .unwrap();
            for _ in 0..12 {
                let current = expected(&session);
                if session
                    .public_observation(current)
                    .unwrap()
                    .entry_choice
                    .is_some()
                {
                    break;
                }
                if current.acting_player == actor.into() {
                    let prompt = projector.project_current(&session, current).unwrap();
                    projector
                        .submit(&mut session, request(&prompt, "Pass priority"))
                        .unwrap();
                } else {
                    session
                        .step_opponent_with(current, |_, actions| {
                            Ok(actions
                                .iter()
                                .position(|a| matches!(a, ActionSemanticV1::Pass { .. }))
                                .unwrap() as u32)
                        })
                        .unwrap();
                }
            }
            let current = expected(&session);
            let prompt = projector.project_current(&session, current).unwrap();
            assert_eq!(prompt.actions.len(), 5);
            assert_eq!(
                prompt
                    .state
                    .extensions
                    .entry_choice
                    .as_ref()
                    .unwrap()
                    .colors,
                crate::heraldic_banner_v1::COLORS
            );
            assert_safe(&serde_json::to_value(&prompt).unwrap());
            let public = session.public_observation(current).unwrap();
            assert_eq!(public.roots.len(), 4);
            for root in &public.roots {
                assert_eq!(
                    root.root_sha256,
                    crate::rules_meaning_heraldic_banner_v1::meaning(
                        &crate::rules_meaning_heraldic_banner_v1::live_selector().unwrap(),
                        root.card_id,
                        root.root
                    )
                    .unwrap()
                    .root_sha256
                );
            }
            let saved = session.snapshot_v36();
            let choice = request(&prompt, "Choose green");
            let receipt = projector.submit(&mut session, choice.clone()).unwrap();
            let after = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, choice.clone()).unwrap(),
                receipt
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), after);
            session.restore_v36(&saved).unwrap();
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, choice),
                Err(Error::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            let fresh = projector
                .project_current(&session, expected(&session))
                .unwrap();
            projector
                .submit(&mut session, request(&fresh, "Choose green"))
                .unwrap();
            let mana_prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            projector
                .submit(&mut session, request(&mana_prompt, "Tap Heraldic Banner"))
                .unwrap();
            let core = session.inner.banner_fixture_state_mut();
            assert!(core.objects.get(banner).tapped);
            assert_eq!(
                core.players[actor.index()].mana_pool[ManaColor::G.pool_index()],
                1
            );
            assert_eq!(core.objects.get(banner).zone, Zone::Battlefield);
            assert_eq!(engine::effective_power(core, elf), 2);
            assert_eq!(engine::effective_toughness(core, elf), 1);
            assert_eq!(engine::effective_power(core, opposing), 1);
            let mut bad = core.clone();
            bad.objects.get_mut(banner).v4.chosen_color = Some(ManaColor::R);
            let bytes = serde_json::to_vec(&bad).unwrap();
            assert!(engine::step(&mut bad, Action::Pass).is_err());
            assert!(crate::banner_observation_v1::observe_v1(
                &bad,
                &crate::policy_surface_v5::PolicySurfaceV5::new_for_session(),
                actor,
                0,
                0,
                0,
                1
            )
            .is_err());
            assert_eq!(serde_json::to_vec(&bad).unwrap(), bytes);
            assert!(crate::rl::observe_v1(core, actor, 0).is_err());
            assert!(crate::rl::observe_v2(
                core,
                &crate::surface_v2::HarnessSurfaceV2::new(),
                actor,
                0
            )
            .is_err());
            crate::standard_cards_v1::gain_control(core, banner, actor.opponent());
            assert_eq!(engine::effective_power(core, elf), 1);
            assert_eq!(engine::effective_power(core, opposing), 2);
            crate::heraldic_banner_v1::validate_state(core).unwrap();
        }
    }
    #[test]
    #[ignore = "requires genuine Heraldic Banner registration and native qualification"]
    fn real_public_entry_menu_uses_no_opponent_hand_identity_and_rejects_old_consumer_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut prompts = Vec::new();
            for secret in ["Lightning Bolt", "Island"] {
                let mut state = ready(actor);
                let banner = put(&mut state, actor, "Heraldic Banner", Zone::Hand);
                put(&mut state, actor.opponent(), secret, Zone::Hand);
                crate::event::propose_and_commit(
                    &mut state,
                    crate::event::ProposedEvent::zone_change(banner, Zone::Battlefield),
                );
                assert!(
                    matches!(engine::advance_until_decision(&mut state),Decision::ChooseEntryColorV36{player,..} if player==actor)
                );
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
                let session = owned(state);
                let prompt = HumanBannerProjectorV36::new(actor.into())
                    .project_current(&session, expected(&session))
                    .unwrap();
                assert_safe(&serde_json::to_value(&prompt).unwrap());
                prompts.push(prompt);
            }
            assert_eq!(prompts[0], prompts[1]);
        }
    }
}
