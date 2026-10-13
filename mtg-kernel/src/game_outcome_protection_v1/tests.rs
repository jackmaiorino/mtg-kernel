use super::*;
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use crate::engine::{self, Action, Decision};
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use crate::mana::ManaColor;
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use crate::policy_observation_v6::tests::put;
use crate::policy_observation_v6::tests::ready_state_for;

#[test]
fn catalog_without_outcome_protection_keeps_state_and_loss_rules_both_seats() {
    if CARD_DEFS
        .iter()
        .any(|def| def.game_outcome_protection_v1.is_some())
    {
        return;
    }
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state_for(actor);
        let before = serde_json::to_vec(&state).unwrap();
        let hash = state.state_hash();
        assert!(!cannot_lose(&state, actor));
        assert!(!cannot_win(&state, actor));
        let protected = protected_players(&state);
        assert_eq!(protected, [false, false]);
        assert!(!consume_protected_empty_draws(&mut state, protected));
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        assert_eq!(state.state_hash(), hash);
        state.players[actor.index()].life = 0;
        crate::trigger::sba_fixed_point(&mut state);
        assert!(state.players[actor.index()].has_lost);
    }
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) fn cast_herald(actor: PlayerId) -> (GameState, ObjectId) {
    cast_herald_on_turn(actor, actor)
}

#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn cast_herald_on_turn(actor: PlayerId, active_player: PlayerId) -> (GameState, ObjectId) {
    let mut state = ready_state_for(active_player);
    for owner in [active_player, active_player.opponent()] {
        for _ in 0..16 {
            put(&mut state, owner, "Forest", Zone::Library);
        }
    }
    put(&mut state, actor, "Forest", Zone::Battlefield);
    let source = put(&mut state, actor, "Herald of Eternal Dawn", Zone::Hand);
    let def = &CARD_DEFS[usize::from(state.objects.get(source).card_def)];
    assert!(def.is_executable());
    assert_eq!(def.cost.generic, 4);
    assert_eq!(def.power, Some(6));
    assert_eq!(def.toughness, Some(6));
    assert!(def.keywords.contains(crate::card_def::Keywords::FLASH));
    assert!(def.keywords.contains(crate::card_def::Keywords::FLYING));
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
    state.players[actor.index()].mana_pool[ManaColor::W.pool_index()] = 3;
    assert!(matches!(
        engine::advance_until_decision(&mut state),
        Decision::CastSpellOrPass { .. }
    ));
    if active_player != actor {
        engine::step(&mut state, Action::Pass).unwrap();
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { player, .. } if player == actor));
    }
    engine::step(&mut state, Action::CastSpell(source)).unwrap();
    for _ in 0..12 {
        let decision = engine::advance_until_decision(&mut state);
        if state.objects.get(source).zone == Zone::Battlefield {
            assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
            assert!(active(&state, source));
            return (state, source);
        }
        assert!(
            matches!(decision, Decision::CastSpellOrPass { .. }),
            "{decision:?}"
        );
        engine::step(&mut state, Action::Pass).unwrap();
    }
    panic!("genuine Herald cast did not resolve");
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

fn actual_paid_herald_prevents_life_poison_and_empty_draw_losses_with_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for cause in 0..3 {
            let (mut state, source) = cast_herald(actor);
            match cause {
                0 => state.players[actor.index()].life = 0,
                1 => state.players[actor.index()].poison_counters.0 = 10,
                2 => {
                    crate::event::propose_and_commit(
                        &mut state,
                        crate::event::ProposedEvent::draw(actor),
                    );
                    assert!(state.players[actor.index()].drew_from_empty);
                }
                _ => unreachable!(),
            }
            let mut replay: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut replay] {
                crate::trigger::sba_fixed_point(branch);
                assert!(!branch.players[actor.index()].has_lost);
                assert!(cannot_lose(branch, actor));
                assert!(cannot_win(branch, actor.opponent()));
                assert!(!cannot_win(branch, actor));
                assert!(!branch.players[actor.index()].drew_from_empty);
                crate::event::propose_and_commit(
                    branch,
                    crate::event::ProposedEvent::zone_change(source, Zone::Graveyard),
                );
                crate::trigger::sba_fixed_point(branch);
                assert_eq!(branch.players[actor.index()].has_lost, cause != 2);
                assert!(!cannot_lose(branch, actor));
                assert!(!cannot_win(branch, actor.opponent()));
            }
            assert_eq!(state, replay);
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

fn actual_herald_control_and_simultaneous_lethal_departure_preserve_loss_scope_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = cast_herald(actor);
        // Named physical control prerequisite: keep both battlefield indexes
        // consistent while exercising the live controller query.
        state.players[actor.index()]
            .battlefield
            .retain(|id| *id != source);
        state.players[actor.opponent().index()]
            .battlefield
            .push(source);
        state.objects.get_mut(source).controller = actor.opponent();
        assert!(!cannot_lose(&state, actor));
        assert!(cannot_lose(&state, actor.opponent()));
        state.players[actor.index()].life = 0;
        state.players[actor.opponent().index()].life = 0;
        crate::trigger::sba_fixed_point(&mut state);
        assert!(state.players[actor.index()].has_lost);
        assert!(!state.players[actor.opponent().index()].has_lost);
        state.objects.get_mut(source).damage = 6;
        crate::trigger::sba_fixed_point(&mut state);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        assert!(state.players[actor.opponent().index()].has_lost);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

fn actual_empty_draw_and_herald_lethal_damage_in_one_sba_pass_do_not_replay_the_prevented_loss() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, source) = cast_herald(actor);
        crate::event::propose_and_commit(&mut state, crate::event::ProposedEvent::draw(actor));
        assert!(state.players[actor.index()].drew_from_empty);
        // Named physical lethal-damage prerequisite. Both conditions exist
        // before the first SBA pass, rather than removing the source later.
        state.objects.get_mut(source).damage = 6;
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut replay] {
            crate::trigger::sba_fixed_point(branch);
            assert_eq!(branch.objects.get(source).zone, Zone::Graveyard);
            assert!(!branch.players[actor.index()].drew_from_empty);
            assert!(!branch.players[actor.index()].has_lost);
            assert!(!cannot_lose(branch, actor));
            // A new empty draw after the protection ends is a real new loss.
            crate::event::propose_and_commit(branch, crate::event::ProposedEvent::draw(actor));
            crate::trigger::sba_fixed_point(branch);
            assert!(branch.players[actor.index()].has_lost);
        }
        assert_eq!(state, replay);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

fn actual_opponent_turn_flash_payment_and_paid_counter_response_restore_both_seats() {
    for caster in [PlayerId::P0, PlayerId::P1] {
        for counter in [false, true] {
            let active_player = caster.opponent();
            let mut state = ready_state_for(active_player);
            let herald = put(&mut state, caster, "Herald of Eternal Dawn", Zone::Hand);
            let counterspell = put(&mut state, active_player, "Essence Scatter", Zone::Hand);
            state.players[caster.index()].mana_pool[ManaColor::C.pool_index()] = 4;
            state.players[caster.index()].mana_pool[ManaColor::W.pool_index()] = 2;
            assert!(matches!(engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { player, .. } if player == active_player));
            engine::step(&mut state, Action::Pass).unwrap();
            assert!(matches!(engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { player, ref castable_spells, .. }
                if player == caster && !castable_spells.contains(&herald)));
            let before = state.clone();
            assert!(engine::step(&mut state, Action::CastSpell(herald)).is_err());
            assert_eq!(state, before);
            state.players[caster.index()].mana_pool[ManaColor::W.pool_index()] = 3;
            assert!(matches!(engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { player, ref castable_spells, .. }
                if player == caster && castable_spells.contains(&herald)));
            engine::step(&mut state, Action::CastSpell(herald)).unwrap();
            assert!(matches!(
                engine::advance_until_decision(&mut state),
                Decision::CastSpellOrPass { .. }
            ));
            assert_eq!(state.active_player, active_player);
            assert_eq!(state.players[caster.index()].mana_pool, [0; 6]);
            assert_eq!(state.objects.get(herald).zone, Zone::Stack);
            assert!(!cannot_lose(&state, caster));
            engine::step(&mut state, Action::Pass).unwrap();
            if counter {
                state.players[active_player.index()].mana_pool[ManaColor::C.pool_index()] = 1;
                state.players[active_player.index()].mana_pool[ManaColor::U.pool_index()] = 1;
                assert!(matches!(engine::advance_until_decision(&mut state),
                    Decision::CastSpellOrPass { player, ref castable_spells, .. }
                    if player == active_player && castable_spells.contains(&counterspell)));
                engine::step(&mut state, Action::CastSpell(counterspell)).unwrap();
                assert!(matches!(engine::advance_until_decision(&mut state),
                    Decision::ChooseTargets { ref legal_targets, .. }
                    if legal_targets == &[crate::state::Target::Object(herald)]));
                engine::step(
                    &mut state,
                    Action::ChooseTarget(crate::state::Target::Object(herald)),
                )
                .unwrap();
                assert!(matches!(
                    engine::advance_until_decision(&mut state),
                    Decision::CastSpellOrPass { .. }
                ));
                assert_eq!(state.players[active_player.index()].mana_pool, [0; 6]);
            }
            let mut replay: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for branch in [&mut state, &mut replay] {
                for _ in 0..16 {
                    let decision = engine::advance_until_decision(branch);
                    assert!(
                        matches!(decision, Decision::CastSpellOrPass { .. }),
                        "{decision:?}"
                    );
                    if branch.stack.is_empty() && branch.engine.pending_cast.is_none() {
                        break;
                    }
                    engine::step(branch, Action::Pass).unwrap();
                }
                assert!(branch.stack.is_empty());
                assert_eq!(
                    branch.objects.get(herald).zone,
                    if counter {
                        Zone::Graveyard
                    } else {
                        Zone::Battlefield
                    }
                );
                assert_eq!(cannot_lose(branch, caster), !counter);
                assert_eq!(cannot_win(branch, active_player), !counter);
                if counter {
                    let treasures = branch.players[caster.index()]
                        .battlefield
                        .iter()
                        .filter(|id| {
                            CARD_DEFS[usize::from(branch.objects.get(**id).card_def)].name
                                == "Treasure Token"
                        })
                        .count();
                    assert_eq!(branch.objects.get(counterspell).zone, Zone::Graveyard);
                    assert_eq!(treasures, 0);
                }
            }
            assert_eq!(state, replay);
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

fn actual_witness_protection_removes_herald_prohibition_and_restore_keeps_the_loss_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, herald) = cast_herald(actor);
        let selector = crate::rules_meaning_game_outcome_protection_v1::live_selector().unwrap();
        let before_public =
            crate::game_outcome_protection_public_v1::capture(&state, &selector).unwrap();
        assert!(before_public.sources[0].printed_rules_active);
        let witness = put(&mut state, actor, "Witness Protection", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::G.pool_index()] = 1;
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { ref castable_spells, .. } if castable_spells.contains(&witness)));
        engine::step(&mut state, Action::CastSpell(witness)).unwrap();
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::ChooseTargets { ref legal_targets, .. }
            if legal_targets.contains(&crate::state::Target::Object(herald))));
        engine::step(
            &mut state,
            Action::ChooseTarget(crate::state::Target::Object(herald)),
        )
        .unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut replay] {
            for _ in 0..16 {
                let decision = engine::advance_until_decision(branch);
                assert!(
                    matches!(decision, Decision::CastSpellOrPass { .. }),
                    "{decision:?}"
                );
                if branch.objects.get(witness).zone == Zone::Battlefield {
                    break;
                }
                engine::step(branch, Action::Pass).unwrap();
            }
            assert_eq!(branch.objects.get(witness).zone, Zone::Battlefield);
            assert_eq!(
                branch.objects.get(witness).v4.attached_to.unwrap().object,
                herald
            );
            assert!(
                !crate::continuous_characteristics_v1::printed_abilities_active(branch, herald)
            );
            assert!(!active(branch, herald));
            assert!(!cannot_lose(branch, actor));
            assert!(!cannot_win(branch, actor.opponent()));
            let public =
                crate::game_outcome_protection_public_v1::capture(branch, &selector).unwrap();
            assert_eq!(public.sources.len(), 1);
            assert!(!public.sources[0].printed_rules_active);
            assert_eq!(public.cannot_lose, [false; 2]);
            assert_eq!(public.cannot_win, [false; 2]);
            assert_eq!(
                crate::game_outcome_protection_public_v1::validate(branch, &before_public),
                Err(crate::game_outcome_protection_public_v1::ErrorV1::IdentityMismatch)
            );
            crate::event::propose_and_commit(
                branch,
                crate::event::ProposedEvent::life_loss(actor, 20),
            );
            crate::trigger::sba_fixed_point(branch);
            assert!(branch.players[actor.index()].has_lost);
        }
        assert_eq!(state, replay);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

fn actual_paid_herald_public_scope_rejects_forgery_and_hides_private_identity_with_restore() {
    use crate::game_outcome_protection_public_v1::{capture, validate, ErrorV1};
    let selector = crate::rules_meaning_game_outcome_protection_v1::live_selector().unwrap();
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (state, source) = cast_herald(actor);
        let before = serde_json::to_vec(&state).unwrap();
        let record = capture(&state, &selector).unwrap();
        assert_eq!(record.sources.len(), 1);
        assert_eq!(record.sources[0].source.arena_id, source.0);
        assert!(record.sources[0].printed_rules_active);
        assert!(record.cannot_lose[actor.index()]);
        assert!(!record.cannot_lose[actor.opponent().index()]);
        assert!(record.cannot_win[actor.opponent().index()]);
        assert!(!record.cannot_win[actor.index()]);
        let restored: GameState = serde_json::from_slice(&before).unwrap();
        assert_eq!(capture(&restored, &selector).unwrap(), record);
        for field in 0..7 {
            let mut forged = record.clone();
            match field {
                0 => forged.sources.clear(),
                1 => forged.sources.push(record.sources[0].clone()),
                2 => forged.sources[0].source.controller = actor.opponent().into(),
                3 => forged.sources[0].source.zone_change_count += 1,
                4 => forged.sources[0].printed_rules_active = false,
                5 => forged.sources[0].printed_meaning.opponents_cannot_win_game = false,
                _ => forged.cannot_win[actor.index()] = true,
            }
            assert_eq!(validate(&state, &forged), Err(ErrorV1::IdentityMismatch));
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
        // Named private-zone fixture: changing a concealed hand identity must
        // not add a row or alter these public static facts.
        let mut private_herald = state.clone();
        put(
            &mut private_herald,
            actor.opponent(),
            "Herald of Eternal Dawn",
            Zone::Hand,
        );
        let mut private_forest = state.clone();
        put(&mut private_forest, actor.opponent(), "Forest", Zone::Hand);
        assert_eq!(capture(&private_herald, &selector).unwrap(), record);
        assert_eq!(capture(&private_forest, &selector).unwrap(), record);
        // Named face-down battlefield primitive, not a printed hide producer.
        // Its concealed definition must not change rows or aggregate status.
        for name in ["Herald of Eternal Dawn", "Llanowar Elves"] {
            let mut hidden = state.clone();
            let concealed = put(&mut hidden, actor.opponent(), name, Zone::Battlefield);
            hidden.objects.get_mut(concealed).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                disguised: false,
                lookers: 1 << actor.opponent().index(),
                hidden_by: None,
                hidden_by_source: None,
            });
            assert_eq!(capture(&hidden, &selector).unwrap(), record);
            assert!(!active(&hidden, concealed));
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

fn actual_paid_herald_legend_pass_preserves_predeparture_loss_scope_and_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for lethal in [false, true] {
            for cause in 0..3 {
                let (mut state, herald) = cast_herald(actor);
                // Named registered physical legend prerequisite. The legend
                // choice and simultaneous SBA pass themselves use the engine.
                let first = put(&mut state, actor, "Krenko, Mob Boss", Zone::Battlefield);
                let second = put(&mut state, actor, "Krenko, Mob Boss", Zone::Battlefield);
                match cause {
                    0 => state.players[actor.index()].life = 0,
                    1 => state.players[actor.index()].poison_counters.0 = 10,
                    _ => {
                        crate::event::propose_and_commit(
                            &mut state,
                            crate::event::ProposedEvent::draw(actor),
                        );
                        assert!(state.players[actor.index()].drew_from_empty);
                    }
                }
                if lethal {
                    // Named physical lethal-damage prerequisite before the pass.
                    state.objects.get_mut(herald).damage = 6;
                }
                crate::trigger::sba_fixed_point(&mut state);
                assert!(!state.players[actor.index()].has_lost);
                assert!(matches!(engine::advance_until_decision(&mut state),
                    Decision::ChooseLegendPermanent { player, ref candidates }
                    if player == actor && candidates == &[first, second]));
                let mut replay: GameState =
                    serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
                for branch in [&mut state, &mut replay] {
                    engine::step(branch, Action::ChooseLegendPermanent(first)).unwrap();
                    crate::trigger::sba_fixed_point(branch);
                    assert_eq!(branch.objects.get(first).zone, Zone::Battlefield);
                    assert_eq!(branch.objects.get(second).zone, Zone::Graveyard);
                    assert_eq!(
                        branch.objects.get(herald).zone,
                        if lethal {
                            Zone::Graveyard
                        } else {
                            Zone::Battlefield
                        }
                    );
                    assert_eq!(branch.players[actor.index()].has_lost, lethal && cause != 2);
                    assert!(!branch.players[actor.index()].drew_from_empty);
                    assert!(!branch.players[actor.opponent().index()].has_lost);
                }
                assert_eq!(state, replay);
            }
        }
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

fn actual_paid_herald_copy_allocator_primitive_retains_prohibition_after_original_leaves() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let (mut state, herald) = cast_herald(actor);
        // The existing exact copy allocator is exercised as a named primitive,
        // without claiming a paid, registered Self-Reflection spell.
        let copy_source = put(&mut state, actor, "Counterspell", Zone::Hand);
        let mut ctx = crate::effect::ExecCtx::no_targets(copy_source, actor);
        ctx.targets = vec![crate::state::Target::Object(herald)];
        ctx.target_contracts = vec![crate::state::StackTargetContractV4::capture(
            &state,
            ctx.targets[0],
        )];
        crate::self_reflection_v1::execute(&mut state, &ctx);
        let token = *state.players[actor.index()].battlefield.last().unwrap();
        assert_ne!(token, herald);
        assert!(state.objects.get(token).v4.is_token);
        assert_eq!(
            state.objects.get(token).card_def,
            state.objects.get(herald).card_def
        );
        assert!(active(&state, token));
        let selector = crate::rules_meaning_game_outcome_protection_v1::live_selector().unwrap();
        let both = crate::game_outcome_protection_public_v1::capture(&state, &selector).unwrap();
        assert_eq!(both.sources.len(), 2);
        state.objects.get_mut(herald).damage = 6;
        state.players[actor.index()].life = 0;
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut replay] {
            crate::trigger::sba_fixed_point(branch);
            assert_eq!(branch.objects.get(herald).zone, Zone::Graveyard);
            assert!(active(branch, token));
            assert!(!branch.players[actor.index()].has_lost);
            let public =
                crate::game_outcome_protection_public_v1::capture(branch, &selector).unwrap();
            assert_eq!(public.sources.len(), 1);
            assert_eq!(public.sources[0].source.arena_id, token.0);
            branch.objects.get_mut(token).damage = 6;
            crate::trigger::sba_fixed_point(branch);
            assert!(!cannot_lose(branch, actor));
            assert!(branch.players[actor.index()].has_lost);
            assert!(
                crate::game_outcome_protection_public_v1::capture(branch, &selector)
                    .unwrap()
                    .sources
                    .is_empty()
            );
        }
        assert_eq!(state, replay);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]

fn actual_involuntary_employment_moves_loss_protection_to_new_controller_with_restore_both_seats() {
    for owner in [PlayerId::P0, PlayerId::P1] {
        let thief = owner.opponent();
        let (mut state, herald) = cast_herald_on_turn(owner, thief);
        assert_eq!(state.active_player, thief);
        let spell = put(&mut state, thief, "Involuntary Employment", Zone::Hand);
        state.players[thief.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        state.players[thief.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { player, ref castable_spells, .. }
            if player == thief && castable_spells.contains(&spell)));
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::ChooseTargets { ref legal_targets, .. }
            if legal_targets.contains(&crate::state::Target::Object(herald))));
        engine::step(
            &mut state,
            Action::ChooseTarget(crate::state::Target::Object(herald)),
        )
        .unwrap();
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { .. }
        ));
        assert_eq!(state.players[thief.index()].mana_pool, [0; 6]);
        let mut replay: GameState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        for branch in [&mut state, &mut replay] {
            for _ in 0..16 {
                let decision = engine::advance_until_decision(branch);
                assert!(
                    matches!(decision, Decision::CastSpellOrPass { .. }),
                    "{decision:?}"
                );
                if branch.stack.is_empty() && branch.engine.pending_effect.is_none() {
                    break;
                }
                engine::step(branch, Action::Pass).unwrap();
            }
            assert_eq!(branch.objects.get(spell).zone, Zone::Graveyard);
            assert_eq!(branch.objects.get(herald).owner, owner);
            assert_eq!(branch.objects.get(herald).controller, thief);
            assert!(branch.players[thief.index()].battlefield.contains(&herald));
            assert!(!branch.players[owner.index()].battlefield.contains(&herald));
            assert!(!cannot_lose(branch, owner));
            assert!(cannot_lose(branch, thief));
            assert!(cannot_win(branch, owner));
            assert!(!cannot_win(branch, thief));
            crate::event::propose_and_commit(
                branch,
                crate::event::ProposedEvent::life_loss(owner, 20),
            );
            crate::event::propose_and_commit(
                branch,
                crate::event::ProposedEvent::life_loss(thief, 20),
            );
            crate::trigger::sba_fixed_point(branch);
            assert!(branch.players[owner.index()].has_lost);
            assert!(!branch.players[thief.index()].has_lost);
        }
        assert_eq!(state, replay);
    }
}
