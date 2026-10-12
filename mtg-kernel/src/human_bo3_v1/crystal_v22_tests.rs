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
        assert_eq!(
            session
                .inner
                .crystal_fixture_state()
                .objects
                .get(target)
                .damage,
            0
        );
        assert!(matches!(
            session
                .inner
                .crystal_fixture_state()
                .engine
                .active_replacements[0]
                .kind,
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

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn future_incinerating_private_human_discard_decline_and_owning_restore_both_seats() {
    use crate::engine::{self, Action, Decision};
    use crate::event::{ActiveReplacement, ReplacementEffectKind};
    use crate::mana::ManaColor;
    use crate::state::Target;
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let mut state = ready_state();
            state.active_player = actor;
            state.priority_player = actor;
            let wall = put(
                &mut state,
                actor.opponent(),
                "Crystal Barricade",
                Zone::Battlefield,
            );
            let target = put(
                &mut state,
                actor.opponent(),
                "Llanowar Elves",
                Zone::Battlefield,
            );
            let card = put(&mut state, actor, "Forest", Zone::Hand);
            put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
            let spell = put(&mut state, actor, "Incinerating Blast", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
            state.engine.next_replacement_id = 1;
            state.engine.active_replacements.push(ActiveReplacement {
                id: 1,
                source: wall,
                kind: ReplacementEffectKind::PreventNextDamage {
                    target: Target::Object(target),
                    remaining: 1,
                },
            });
            assert!(
                matches!(engine::advance_until_decision(&mut state),Decision::CastSpellOrPass {player,..} if player==actor)
            );
            engine::step(&mut state, Action::CastSpell(spell)).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::ChooseTargets { .. }
            ));
            engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
            let _ = engine::advance_until_decision(&mut state);
            for _ in 0..10 {
                let d = engine::advance_until_decision(&mut state);
                if matches!(d, Decision::ChooseEffectOption { .. }) {
                    break;
                }
                assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
                engine::step(&mut state, Action::Pass).unwrap();
            }
            let Some(crate::effect::PendingEffectChoice::ChooseOption {
                purpose: crate::effect::EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
                ..
            }) = state
                .engine
                .pending_effect
                .as_ref()
                .and_then(|p| p.choice.as_ref())
            else {
                panic!("real prevention");
            };
            let selected = choice
                .options
                .iter()
                .position(|o| {
                    matches!(
                        o,
                        crate::effect::crystal_prevention_v1::PreventionV1::Barricade(_)
                    )
                })
                .unwrap();
            engine::step(
                &mut state,
                Action::ChooseEffectOption(u16::try_from(selected).unwrap()),
            )
            .unwrap();
            assert!(
                matches!(engine::advance_until_decision(&mut state),Decision::ChooseEffectTargets {player,source,..}
                if player==actor && source==spell)
            );
            let before_hand = state.players[actor.index()].hand.len();
            let before_library = state.players[actor.index()].library.len();
            let base = FastActorSessionV1::from_crystal_v22_fixture_state(state);
            let mut session = HumanCrystalSessionV22::from_session(base).unwrap();
            let current = expected(&session);
            let public = session.public_observation(current).unwrap();
            assert_eq!(
                public.optional_discard_menu.as_ref().unwrap().chooser,
                actor.into()
            );
            assert_eq!(
                public
                    .optional_discard_menu
                    .as_ref()
                    .unwrap()
                    .source
                    .arena_id,
                spell.0
            );
            let mut projector = HumanCrystalProjectorV22::new(actor.into());
            let prompt = projector.project_current(&session, current).unwrap();
            safe(&serde_json::to_value(&prompt).unwrap());
            let private_before = session.inner.privileged_core_environment_hash();
            assert!(HumanCrystalProjectorV22::new(actor.opponent().into())
                .project_current(&session, current)
                .is_err());
            assert_eq!(
                session.inner.privileged_core_environment_hash(),
                private_before
            );
            let request = action(
                &prompt,
                if pay {
                    "Discard Forest"
                } else {
                    "Decline the optional discard"
                },
            );
            let snapshot = session.snapshot_v22();
            let receipt = projector.submit(&mut session, request.clone()).unwrap();
            let after = session.inner.privileged_core_environment_hash();
            let actual = session.inner.crystal_fixture_state();
            assert_eq!(
                actual.players[actor.index()].library.len(),
                before_library - usize::from(pay)
            );
            assert_eq!(actual.players[actor.index()].hand.len(), before_hand);
            assert_eq!(
                actual.objects.get(card).zone,
                if pay { Zone::Graveyard } else { Zone::Hand }
            );
            assert_eq!(actual.objects.get(target).damage, 0);
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
            let prompt_again = replay
                .project_current(&session, expected(&session))
                .unwrap();
            assert_eq!(prompt_again, prompt);
            replay.submit(&mut session, request).unwrap();
            assert_eq!(session.inner.privileged_core_environment_hash(), after);
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn future_bite_human_binds_resolving_spell_separately_from_actual_damage_source() {
    use crate::engine::{self, Action, Decision};
    use crate::event::{ActiveReplacement, ReplacementEffectKind};
    use crate::mana::ManaColor;
    use crate::state::Target;
    for caster in [PlayerId::P0, PlayerId::P1] {
        let actor = caster.opponent();
        let mut state = ready_state();
        state.active_player = caster;
        state.priority_player = caster;
        let wall = put(&mut state, actor, "Crystal Barricade", Zone::Battlefield);
        let dealer = put(&mut state, caster, "Llanowar Elves", Zone::Battlefield);
        let recipient = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let spell = put(&mut state, caster, "Bite Down", Zone::Hand);
        state.players[caster.index()].mana_pool[ManaColor::G.pool_index()] = 1;
        state.players[caster.index()].mana_pool[ManaColor::C.pool_index()] = 1;
        state.engine.next_replacement_id = 1;
        state.engine.active_replacements.push(ActiveReplacement {
            id: 1,
            source: wall,
            kind: ReplacementEffectKind::PreventNextDamage {
                target: Target::Object(recipient),
                remaining: 1,
            },
        });
        engine::advance_until_decision(&mut state);
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        for target in [dealer, recipient] {
            assert!(
                matches!(engine::advance_until_decision(&mut state),Decision::ChooseTargets {legal_targets,..}
                if legal_targets.contains(&Target::Object(target)))
            );
            engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        }
        engine::advance_until_decision(&mut state);
        for _ in 0..10 {
            let d = engine::advance_until_decision(&mut state);
            if matches!(d, Decision::ChooseEffectOption { .. }) {
                break;
            }
            assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert!(
            matches!(engine::advance_until_decision(&mut state),Decision::ChooseEffectOption {player,source,..}
            if player==actor && source==spell)
        );
        let base = FastActorSessionV1::from_crystal_v22_fixture_state(state);
        let mut session = HumanCrystalSessionV22::from_session(base).unwrap();
        let current = expected(&session);
        let public = session.public_observation(current).unwrap();
        let menu = public.prevention_menu.as_ref().unwrap();
        assert_eq!(menu.source.arena_id, dealer.0);
        assert_eq!(menu.resolving_source.arena_id, spell.0);
        let mut projector = HumanCrystalProjectorV22::new(actor.into());
        let prompt = projector.project_current(&session, current).unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        let request = action(&prompt, "Apply Crystal Barricade");
        projector.submit(&mut session, request.clone()).unwrap();
        assert_eq!(
            session
                .inner
                .crystal_fixture_state()
                .objects
                .get(recipient)
                .damage,
            0
        );
        let after = session.inner.privileged_core_environment_hash();
        projector.submit(&mut session, request).unwrap();
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn future_indrik_program_menu_public_human_submit_retry_and_owning_restore() {
    use crate::engine::{self, Action, Decision};
    use crate::event::{self, ActiveReplacement, ProposedEvent, ReplacementEffectKind};
    use crate::state::Target;
    for actor in [PlayerId::P0, PlayerId::P1] {
        for accept in [false, true] {
            let mut state = basic(actor, "Lightning Bolt");
            let wall = put(
                &mut state,
                actor.opponent(),
                "Crystal Barricade",
                Zone::Battlefield,
            );
            let target = put(
                &mut state,
                actor.opponent(),
                "Shivan Dragon",
                Zone::Battlefield,
            );
            let indrik = put(&mut state, actor, "Affectionate Indrik", Zone::Hand);
            state.engine.active_replacements.push(ActiveReplacement {
                id: 1,
                source: wall,
                kind: ReplacementEffectKind::PreventNextDamage {
                    target: Target::Object(target),
                    remaining: 1,
                },
            });
            state.engine.next_replacement_id = 2;
            // Typed entry setup publishes the real definition-owned trigger.
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(indrik, Zone::Battlefield),
            );
            for _ in 0..20 {
                let decision = engine::advance_until_decision(&mut state);
                match decision {
                    Decision::ChooseTargets { legal_targets, .. } => {
                        assert!(legal_targets.contains(&Target::Object(target)));
                        engine::step(&mut state, Action::ChooseTarget(Target::Object(target)))
                            .unwrap();
                    }
                    Decision::ChooseEffectOption { .. } => break,
                    Decision::CastSpellOrPass { .. } => {
                        engine::step(&mut state, Action::Pass).unwrap()
                    }
                    _ => panic!("real Indrik prompt {decision:?}"),
                }
            }
            assert!(crate::effect::crystal_prevention_v1::is_program_choice(
                &state
            ));
            let base = FastActorSessionV1::from_crystal_v22_fixture_state(state);
            let mut session = HumanCrystalSessionV22::from_session(base).unwrap();
            let e = expected(&session);
            let public = session.public_observation(e).unwrap();
            let menu = public.optional_fight_menu.as_ref().unwrap();
            assert_eq!(menu.chooser, actor.into());
            assert!(menu.requires_original_source_creature);
            let mut projector = HumanCrystalProjectorV22::new(actor.into());
            let prompt = projector.project_current(&session, e).unwrap();
            safe(&serde_json::to_value(&prompt).unwrap());
            let request = action(
                &prompt,
                if accept {
                    "Have Affectionate Indrik"
                } else {
                    "Decline the optional fight"
                },
            );
            let snapshot = session.snapshot_v22();
            let receipt = projector.submit(&mut session, request.clone()).unwrap();
            let after = session.inner.privileged_core_environment_hash();
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
            assert_eq!(
                replay
                    .project_current(&session, expected(&session))
                    .unwrap(),
                prompt
            );
            replay.submit(&mut session, request).unwrap();
            assert_eq!(session.inner.privileged_core_environment_hash(), after);
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn future_firebrand_fixed_history_human_menu_retry_rollback_and_safe_capture_both_seats() {
    use crate::engine::{self, Action, Decision};
    use crate::event::{ActiveReplacement, ReplacementEffectKind};
    use crate::state::Target;
    for caster in [PlayerId::P0, PlayerId::P1] {
        let chooser = caster.opponent();
        let mut core = ready_state();
        core.active_player = caster;
        core.priority_player = caster;
        // Physical setup of real registered definitions, then a genuine printed activation.
        let wall = put(&mut core, chooser, "Crystal Barricade", Zone::Battlefield);
        let target = put(&mut core, chooser, "Llanowar Elves", Zone::Battlefield);
        let fire = put(&mut core, caster, "Fanatical Firebrand", Zone::Battlefield);
        put(&mut core, chooser, "Lightning Bolt", Zone::Hand);
        put(&mut core, caster, "Lightning Bolt", Zone::Hand);
        for player in [PlayerId::P0, PlayerId::P1] {
            core.players[player.index()].mana_pool[crate::mana::ManaColor::R.pool_index()] = 1;
        }
        core.objects.get_mut(fire).v4.lifelink_keyword_counters = 1;
        core.engine.next_replacement_id += 1;
        core.engine.active_replacements.push(ActiveReplacement {
            id: core.engine.next_replacement_id,
            source: wall,
            kind: ReplacementEffectKind::PreventNextDamage {
                target: Target::Object(target),
                remaining: 1,
            },
        });
        assert!(
            matches!(engine::advance_until_decision(&mut core),Decision::CastSpellOrPass{player,..} if player==caster)
        );
        engine::step(&mut core, Action::ActivateAbility(fire, 0)).unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut core),
            Decision::ChooseTargets { .. }
        ));
        engine::step(&mut core, Action::ChooseTarget(Target::Object(target))).unwrap();
        let _ = engine::advance_until_decision(&mut core);
        for _ in 0..8 {
            let d = engine::advance_until_decision(&mut core);
            if matches!(d,Decision::ChooseEffectOption{player,..} if player==chooser) {
                break;
            }
            assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
            engine::step(&mut core, Action::Pass).unwrap();
        }
        assert!(
            matches!(engine::advance_until_decision(&mut core),Decision::ChooseEffectOption{player,..} if player==chooser)
        );
        let base = FastActorSessionV1::from_crystal_v22_fixture_state(core);
        let mut session = HumanCrystalSessionV22::from_session(base).unwrap();
        let current = expected(&session);
        let public = session.public_observation(current).unwrap();
        let menu = public.prevention_menu.as_ref().unwrap();
        assert!(menu.source_history.is_none());
        let fixed = menu.fixed_source_history.as_ref().unwrap();
        assert_eq!(fixed.printed_damage, 1);
        assert_eq!(fixed.captured_controller, caster.into());
        assert!(fixed.lifelink);
        let mut projector = HumanCrystalProjectorV22::new(chooser.into());
        let prompt = projector.project_current(&session, current).unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(
            prompt
                .state
                .extensions
                .prevention_menu
                .as_ref()
                .unwrap()
                .fixed_source_history
                .as_ref()
                .unwrap(),
            fixed
        );
        let request = action(&prompt, "Apply Crystal Barricade");
        let snapshot = session.snapshot_v22();
        let receipt = projector.submit(&mut session, request.clone()).unwrap();
        let after = session.inner.privileged_core_environment_hash();
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
        let mut replay = HumanCrystalProjectorV22::new(chooser.into());
        let again = replay
            .project_current(&session, expected(&session))
            .unwrap();
        assert_eq!(again, prompt);
        replay.submit(&mut session, request).unwrap();
        assert_eq!(session.inner.privileged_core_environment_hash(), after);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn future_printed_gorehorn_menu_public_human_safe_facts_and_owning_rollback_both_seats() {
    use crate::engine::{self, Action, Decision};
    use crate::event::{ActiveReplacement, ReplacementEffectKind};
    use crate::mana::ManaColor;
    use crate::state::{Step, Target};
    for caster in [PlayerId::P0, PlayerId::P1] {
        let chooser = caster.opponent();
        let mut state = ready_state();
        state.active_player = caster;
        state.priority_player = caster;
        let wall = put(&mut state, chooser, "Crystal Barricade", Zone::Battlefield);
        let target = put(&mut state, chooser, "Shivan Dragon", Zone::Battlefield);
        let attacker = put(&mut state, caster, "Llanowar Elves", Zone::Battlefield);
        put(&mut state, caster, "Mountain", Zone::Battlefield);
        put(&mut state, chooser, "Mountain", Zone::Battlefield);
        put(&mut state, caster, "Lightning Bolt", Zone::Hand);
        put(&mut state, chooser, "Lightning Bolt", Zone::Hand);
        state.step = Step::DeclareAttackers;
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::DeclareAttackers { .. }
        ));
        engine::step(&mut state, Action::DeclareAttackers(vec![attacker])).unwrap();
        for _ in 0..40 {
            let d = engine::advance_until_decision(&mut state);
            if state.step == Step::Main2
                && matches!(d,Decision::CastSpellOrPass { player, .. } if player==caster)
            {
                break;
            }
            match d {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                Decision::DeclareBlockers { .. } => {
                    engine::step(&mut state, Action::DeclareBlockers(vec![])).unwrap()
                }
                other => panic!("actual combat {other:?}"),
            }
        }
        assert_eq!(state.step, Step::Main2);
        let gorehorn = put(&mut state, caster, "Gorehorn Raider", Zone::Hand);
        state.players[caster.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        state.players[caster.index()].mana_pool[ManaColor::C.pool_index()] = 4;
        engine::step(&mut state, Action::CastSpell(gorehorn)).unwrap();
        let _ = engine::advance_until_decision(&mut state);
        for _ in 0..12 {
            let d = engine::advance_until_decision(&mut state);
            if matches!(d, Decision::ChooseTargets { .. }) {
                break;
            }
            assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::ChooseTargets { .. }
        ));
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        let _ = engine::advance_until_decision(&mut state);
        state.engine.next_replacement_id = 1;
        state.engine.active_replacements.push(ActiveReplacement {
            id: 1,
            source: wall,
            kind: ReplacementEffectKind::PreventNextDamage {
                target: Target::Object(target),
                remaining: 5,
            },
        });
        state.objects.get_mut(gorehorn).v4.lifelink_keyword_counters = 1;
        for _ in 0..12 {
            let d = engine::advance_until_decision(&mut state);
            if matches!(d,Decision::ChooseEffectOption { player, .. } if player==chooser) {
                break;
            }
            assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert!(
            matches!(engine::advance_until_decision(&mut state),Decision::ChooseEffectOption { player, .. } if player==chooser)
        );
        let base = FastActorSessionV1::from_crystal_v22_fixture_state(state);
        let mut session = HumanCrystalSessionV22::from_session(base).unwrap();
        let current = expected(&session);
        let public = session.public_observation(current).unwrap();
        let facts = public
            .prevention_menu
            .as_ref()
            .unwrap()
            .raid_source_history
            .as_ref()
            .unwrap();
        assert_eq!(facts.ability_controller, caster.into());
        assert_eq!(facts.damage_controller, caster.into());
        assert!(!facts.historical);
        assert!(facts.lifelink);
        let mut projector = HumanCrystalProjectorV22::new(chooser.into());
        let prompt = projector.project_current(&session, current).unwrap();
        safe(&serde_json::to_value(&prompt).unwrap());
        assert_eq!(
            prompt
                .state
                .extensions
                .prevention_menu
                .as_ref()
                .unwrap()
                .raid_source_history
                .as_ref(),
            Some(facts)
        );
        let request = action(&prompt, "Apply Crystal Barricade");
        let snapshot = session.snapshot_v22();
        let receipt = projector.submit(&mut session, request.clone()).unwrap();
        assert_eq!(
            projector.submit(&mut session, request.clone()).unwrap(),
            receipt
        );
        assert_eq!(
            session
                .inner
                .crystal_fixture_state()
                .objects
                .get(target)
                .damage,
            0
        );
        assert_eq!(
            session
                .inner
                .crystal_fixture_state()
                .engine
                .event_history
                .iter()
                .filter(|e| matches!(
                    e,
                    crate::event::CommittedEvent::GorehornCrystalCompletionV1 { .. }
                ))
                .count(),
            1
        );
        session.restore_v22(&snapshot).unwrap();
        let before = session.inner.privileged_core_environment_hash();
        assert_eq!(
            projector.submit(&mut session, request),
            Err(Error::StaleDecision)
        );
        assert_eq!(before, session.inner.privileged_core_environment_hash());
        let mut fresh = HumanCrystalProjectorV22::new(chooser.into());
        let current = expected(&session);
        let restored = fresh.project_current(&session, current).unwrap();
        let request = action(&restored, "Apply Crystal Barricade");
        fresh.submit(&mut session, request).unwrap();
        assert_eq!(
            session
                .inner
                .crystal_fixture_state()
                .objects
                .get(target)
                .damage,
            0
        );
    }
}
