//! Actual registered cards and actual engine/Human transitions, not admission probes.
use super::*;
use crate::card_def::{card_id_by_name, CARD_DEFS};
use crate::engine::{self, Action, Decision};
use crate::event::CommittedEvent;
use crate::ids::ObjectId;
use crate::inspiration_observation_v1::PhaseV1;
use crate::mana::ManaColor;
use crate::rules_meaning_inspiration_v1::CastRootV1;
use crate::state::{Step, Target};

fn ready(actor: PlayerId, library: &[&str]) -> GameState {
    let own = library
        .iter()
        .map(|name| card_id_by_name(name).unwrap())
        .collect::<Vec<_>>();
    let other = vec![card_id_by_name("Forest").unwrap(); 16];
    let (p0, p1) = if actor == PlayerId::P0 {
        (&own, &other)
    } else {
        (&other, &own)
    };
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        p0,
        p1,
        crate::rl::card_name,
        110_370,
        actor,
    );
    state.step = Step::Main1;
    state
}
fn next(state: &mut GameState) -> Decision {
    let result = engine::advance_until_decision(state);
    assert!(!matches!(result, Decision::Halted { .. }), "{result:?}");
    result
}
fn settle(state: &mut GameState) {
    for _ in 0..24 {
        assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
        if state.stack.is_empty() && state.engine.pending_effect.is_none() {
            return;
        }
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("real priority did not drain");
}
fn cast(state: &mut GameState, actor: PlayerId, source: ObjectId, generic: u8, blue: u8) {
    state.players[actor.index()].mana_pool = [0; 6];
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = generic;
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = blue;
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&source))
    );
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.engine.pending_cast.is_none());
    assert_eq!(state.stack.last().unwrap().source, source);
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
}
fn return_choice(state: &mut GameState, actor: PlayerId) {
    for _ in 0..16 {
        let decision = next(state);
        if let Some(choice) =
            crate::inspiration_observation_v1::current_choice(state, actor).unwrap()
        {
            assert_eq!(choice.phase, PhaseV1::ReturnCard);
            assert!(
                matches!(decision, Decision::ChooseEffectTargets { player, .. } if player == actor)
            );
            return;
        }
        assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("short-library return stage not reached");
}
fn source(state: &mut GameState, actor: PlayerId, flashback: bool) -> ObjectId {
    put(
        state,
        actor,
        "Inspiration from Beyond",
        if flashback {
            Zone::Graveyard
        } else {
            Zone::Hand
        },
    )
}
fn final_zone(flashback: bool) -> Zone {
    if flashback {
        Zone::Exile
    } else {
        Zone::Graveyard
    }
}

#[test]
fn zero_and_one_card_libraries_keep_real_mandatory_return_finish_and_owning_restore_both_seats() {
    // An empty library must still offer a preexisting eligible card; a single
    // milled spell must join that same non-target selection without ordering.
    for actor in [PlayerId::P0, PlayerId::P1] {
        for flashback in [false, true] {
            for (library, prior) in [
                (Vec::new(), false),
                (Vec::new(), true),
                (vec!["Forest"], false),
                (vec!["Forest"], true),
                (vec!["Lightning Bolt"], false),
            ] {
                let mut state = ready(actor, &library);
                let milled = state.players[actor.index()].library.clone();
                let chosen = if prior {
                    Some(put(&mut state, actor, "Ponder", Zone::Graveyard))
                } else if library == ["Lightning Bolt"] {
                    Some(milled[0])
                } else {
                    None
                };
                let opposing = put(&mut state, actor.opponent(), "Ponder", Zone::Graveyard);
                let spell = source(&mut state, actor, flashback);
                cast(
                    &mut state,
                    actor,
                    spell,
                    if flashback { 5 } else { 2 },
                    if flashback { 2 } else { 1 },
                );
                return_choice(&mut state, actor);
                assert!(state.players[actor.index()].library.is_empty());
                assert!(milled
                    .iter()
                    .all(|id| state.objects.get(*id).zone == Zone::Graveyard));
                assert_eq!(state.objects.get(spell).zone, Zone::Stack);
                let before = state.clone();
                assert!(engine::step(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(opposing))
                )
                .is_err());
                if chosen.is_some() {
                    assert!(engine::step(&mut state, Action::FinishEffectSelection).is_err());
                }
                assert_eq!(state, before);

                let mut session = owned(state);
                let mut projector = HumanInspirationProjectorV37::new(actor.into());
                let current = expected(&session);
                let public = session.public_observation(current).unwrap();
                let prompt = projector.project_current(&session, current).unwrap();
                assert_safe(&serde_json::to_value(&prompt).unwrap());
                let choice = public.choice.as_ref().unwrap();
                assert_eq!(choice.phase, PhaseV1::ReturnCard);
                assert_eq!(
                    choice.cast_root,
                    if flashback {
                        CastRootV1::Flashback
                    } else {
                        CastRootV1::Normal
                    }
                );
                let bound = u16::from(chosen.is_some());
                assert_eq!((choice.minimum, choice.maximum), (bound, bound));
                assert_eq!(choice.can_finish, chosen.is_none());
                assert_eq!(choice.legal.len(), usize::from(bound));
                assert!(choice.instant_or_sorcery_only && choice.choice_is_not_a_target);
                if let Some(id) = chosen {
                    assert_eq!(choice.legal[0].arena_id, id.0);
                }
                let input = request(
                    &prompt,
                    if chosen.is_some() {
                        "Return "
                    } else {
                        "Finish Inspiration selection"
                    },
                );
                let saved = session.snapshot_v37();
                let original = session
                    .inner
                    .inspiration_fixture_state_mut()
                    .engine
                    .event_history
                    .clone();
                let receipt = projector.submit(&mut session, input.clone()).unwrap();
                let after = session.inner.privileged_core_environment_hash();
                assert_eq!(
                    projector.submit(&mut session, input.clone()).unwrap(),
                    receipt
                );
                assert_eq!(session.inner.privileged_core_environment_hash(), after);
                let completed = session.inner.inspiration_fixture_state_mut().clone();
                assert_eq!(completed.objects.get(spell).zone, final_zone(flashback));
                assert!(completed.engine.pending_effect.is_none());
                assert_eq!(completed.engine.event_history[..original.len()], original);
                if let Some(id) = chosen {
                    assert_eq!(completed.objects.get(id).zone, Zone::Hand);
                }
                assert_eq!(completed.objects.get(opposing).zone, Zone::Graveyard);
                assert!(completed.engine.event_history.iter().any(|event| matches!(event,
                    CommittedEvent::InspirationResolutionInputV1 { input }
                    if if chosen.is_some() { matches!(input.kind, crate::effect::InspirationInputKindV1::Target(_)) }
                       else { matches!(input.kind, crate::effect::InspirationInputKindV1::Finish) })));
                // Repeated owning restore retains the original operation journals,
                // invalidates accepted requests, and produces the same real result.
                for _ in 0..2 {
                    session.restore_v37(&saved).unwrap();
                    let before = session.inner.privileged_core_environment_hash();
                    assert_eq!(
                        projector.submit(&mut session, input.clone()),
                        Err(Error::StaleDecision)
                    );
                    assert_eq!(session.inner.privileged_core_environment_hash(), before);
                    let fresh = projector
                        .project_current(&session, expected(&session))
                        .unwrap();
                    assert_eq!(fresh.state, prompt.state);
                    projector
                        .submit(
                            &mut session,
                            request(
                                &fresh,
                                if chosen.is_some() {
                                    "Return "
                                } else {
                                    "Finish Inspiration selection"
                                },
                            ),
                        )
                        .unwrap();
                    assert_eq!(session.inner.inspiration_fixture_state_mut(), &completed);
                }
            }
        }
    }
}

#[test]
fn registered_an_offer_response_counters_before_owned_mill_and_restores_real_treasures_both_seats()
{
    for actor in [PlayerId::P0, PlayerId::P1] {
        for flashback in [false, true] {
            let mut state = ready(actor, &["Lightning Bolt"]);
            let library = state.players[actor.index()].library.clone();
            let spell = source(&mut state, actor, flashback);
            let offer = put(
                &mut state,
                actor.opponent(),
                "An Offer You Can't Refuse",
                Zone::Hand,
            );
            cast(
                &mut state,
                actor,
                spell,
                if flashback { 5 } else { 2 },
                if flashback { 2 } else { 1 },
            );
            let original_item = state.stack.last().unwrap().clone();
            engine::step(&mut state, Action::Pass).unwrap();
            state.players[actor.opponent().index()].mana_pool[ManaColor::U.pool_index()] = 1;
            assert!(
                matches!(next(&mut state), Decision::CastSpellOrPass { player, castable_spells, .. }
                if player == actor.opponent() && castable_spells.contains(&offer))
            );
            engine::step(&mut state, Action::CastSpell(offer)).unwrap();
            let Decision::ChooseTargets { legal_targets, .. } = next(&mut state) else {
                panic!("real Offer target");
            };
            assert_eq!(legal_targets, vec![Target::Object(spell)]);
            engine::step(&mut state, Action::ChooseTarget(Target::Object(spell))).unwrap();
            assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
            assert_eq!(state.stack.last().unwrap().source, offer);
            assert_eq!(
                state.stack.last().unwrap().targets,
                vec![Target::Object(spell)]
            );
            assert_eq!(state.players[actor.opponent().index()].mana_pool, [0; 6]);
            assert!(state.stack.contains(&original_item));
            let original = state.engine.event_history.clone();
            let surface = crate::policy_surface_v5::PolicySurfaceV5::new_for_session();
            let viewer = state.priority_player;
            let public =
                crate::inspiration_observation_v1::observe_v1(&state, &surface, viewer, 0, 0, 0, 1)
                    .unwrap();
            assert!(public.choice.is_none());
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                settle(branch);
                assert_eq!(branch.players[actor.index()].library, library);
                assert_eq!(branch.objects.get(spell).zone, final_zone(flashback));
                assert_eq!(branch.objects.get(offer).zone, Zone::Graveyard);
                assert!(branch.engine.pending_effect.is_none());
                assert_eq!(branch.engine.event_history[..original.len()], original);
                assert!(!branch.engine.event_history.iter().any(|e| matches!(
                    e,
                    CommittedEvent::InspirationResolutionBoundaryV1 { .. }
                        | CommittedEvent::InspirationResolutionInputV1 { .. }
                        | CommittedEvent::InspirationResolutionOperationV1 { .. }
                        | CommittedEvent::InspirationEngineDepartureV1 { .. }
                )));
                let treasures = branch.players[actor.index()]
                    .battlefield
                    .iter()
                    .filter(|id| {
                        let object = branch.objects.get(**id);
                        CARD_DEFS[object.card_def as usize].name == "Treasure Token"
                            && object.controller == actor
                    })
                    .count();
                assert_eq!(treasures, 2);
            }
            assert_eq!(state, restored);
            let session = owned(state);
            let mut projector = HumanInspirationProjectorV37::new(expected(&session).acting_player);
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            assert!(prompt.state.extensions.choice.is_none());
            assert_safe(&serde_json::to_value(prompt).unwrap());
        }
    }
}

#[test]
fn real_mocking_sprite_cast_reduces_payment_without_changing_printed_meaning_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for flashback in [false, true] {
            let mut state = ready(actor, &[]);
            let sprite = put(&mut state, actor, "Mocking Sprite", Zone::Hand);
            cast(&mut state, actor, sprite, 2, 1);
            settle(&mut state);
            assert_eq!(state.objects.get(sprite).zone, Zone::Battlefield);
            let chosen = put(&mut state, actor, "Ponder", Zone::Graveyard);
            let spell = source(&mut state, actor, flashback);
            let root = if flashback {
                CastRootV1::Flashback
            } else {
                CastRootV1::Normal
            };
            let meaning = crate::rules_meaning_inspiration_v1::meaning(
                &crate::rules_meaning_inspiration_v1::live_selector().unwrap(),
                state.objects.get(spell).card_def,
                root,
            )
            .unwrap();
            assert_eq!(
                meaning.rule.printed_cast_cost,
                if flashback { "{5}{U}{U}" } else { "{2}{U}" }
            );
            let reduced_generic = if flashback { 4 } else { 1 };
            let blue = if flashback { 2 } else { 1 };
            state.players[actor.index()].mana_pool = [0; 6];
            state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = reduced_generic - 1;
            state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = blue;
            let before = state.clone();
            assert!(engine::step(&mut state, Action::CastSpell(spell)).is_err());
            assert_eq!(state, before);
            cast(&mut state, actor, spell, reduced_generic, blue);
            assert_eq!(state.stack.last().unwrap().is_flashback, flashback);
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut restored] {
                return_choice(branch, actor);
            }
            assert_eq!(state, restored);
            let mut session = owned(restored);
            let mut projector = HumanInspirationProjectorV37::new(actor.into());
            let public = session.public_observation(expected(&session)).unwrap();
            assert_eq!(public.choice.as_ref().unwrap().cast_root, root);
            let prompt = projector
                .project_current(&session, expected(&session))
                .unwrap();
            assert_eq!(
                (
                    prompt.state.extensions.choice.as_ref().unwrap().minimum,
                    prompt.state.extensions.choice.as_ref().unwrap().maximum
                ),
                (1, 1)
            );
            let saved = session.snapshot_v37();
            projector
                .submit(&mut session, request(&prompt, "Return Ponder"))
                .unwrap();
            let completed = session.inner.inspiration_fixture_state_mut().clone();
            assert_eq!(completed.objects.get(chosen).zone, Zone::Hand);
            assert_eq!(completed.objects.get(spell).zone, final_zone(flashback));
            assert_eq!(completed.objects.get(sprite).zone, Zone::Battlefield);
            session.restore_v37(&saved).unwrap();
            let fresh = projector
                .project_current(&session, expected(&session))
                .unwrap();
            projector
                .submit(&mut session, request(&fresh, "Return Ponder"))
                .unwrap();
            assert_eq!(session.inner.inspiration_fixture_state_mut(), &completed);
            assert_eq!(
                crate::rules_meaning_inspiration_v1::meaning(
                    &meaning.selector,
                    meaning.card_id,
                    root
                )
                .unwrap(),
                meaning
            );
        }
    }
}
