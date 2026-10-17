//! Paid Limited native Time Stop cases. Counter, damage and duration setups
//! explicitly labelled below are native primitives, not invented paid spells.
use super::*;
use crate::policy_observation_v6::tests::put;

fn id(name: &str) -> u16 {
    card_def::card_id_by_name(name).unwrap()
}
pub(crate) fn ready(player: PlayerId) -> GameState {
    ready_hidden(player, "Forest")
}
pub(crate) fn ready_hidden(player: PlayerId, hidden: &str) -> GameState {
    let own = [id("Island"); 80];
    let other = [id(hidden); 80];
    let (a, b) = if player == PlayerId::P0 {
        (&own, &other)
    } else {
        (&other, &own)
    };
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        a,
        b,
        crate::rl::card_name,
        110_486,
        player,
    );
    state.step = Step::Main1;
    for p in [player, player.opponent()] {
        for name in ["Island", "Forest", "Mountain", "Plains", "Swamp"] {
            for _ in 0..16 {
                put(&mut state, p, name, Zone::Battlefield);
            }
        }
    }
    state
}
pub(crate) fn restore(state: &GameState) -> GameState {
    let result: GameState = serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
    assert_eq!(state, &result);
    assert_eq!(state.state_hash(), result.state_hash());
    result
}
pub(crate) fn next(state: &mut GameState) -> Decision {
    let d = advance_until_decision(state);
    assert!(state.engine.halted.is_none(), "{:?}", state.engine.halted);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}
pub(crate) fn pass(state: &mut GameState, decision: Decision) {
    let action = match decision {
        Decision::CastSpellOrPass { .. } => Action::Pass,
        Decision::DeclareAttackers { .. } => Action::DeclareAttackers(vec![]),
        Decision::DeclareBlockers { .. } => Action::DeclareBlockers(vec![]),
        Decision::OrderTriggers { pending, .. } => {
            Action::OrderTriggers((0..pending.len()).collect())
        }
        other => panic!("unexpected native decision {other:?}"),
    };
    step(state, action).unwrap();
}
pub(crate) fn priority(state: &mut GameState, player: PlayerId) {
    for _ in 0..64 {
        let d = next(state);
        if matches!(d,Decision::CastSpellOrPass{player:p,..} if p==player) {
            return;
        }
        pass(state, d);
    }
    panic!("priority absent")
}
pub(crate) fn tapped(state: &GameState, player: PlayerId) -> usize {
    state.players[player.index()]
        .battlefield
        .iter()
        .filter(|o| state.objects.get(**o).tapped)
        .count()
}
pub(crate) fn cast(
    state: &mut GameState,
    player: PlayerId,
    name: &str,
    targets: &[Target],
    mana: usize,
) -> ObjectId {
    priority(state, player);
    let card = put(state, player, name, Zone::Hand);
    let before = tapped(state, player);
    assert!(
        matches!(next(state),Decision::CastSpellOrPass{castable_spells,..} if castable_spells.contains(&card))
    );
    step(state, Action::CastSpell(card)).unwrap();
    let mut chosen = 0;
    for _ in 0..32 {
        match next(state) {
            Decision::ChooseTargets {
                player: p,
                legal_targets,
                ..
            } => {
                assert_eq!(p, player);
                assert!(legal_targets.contains(&targets[chosen]));
                step(state, Action::ChooseTarget(targets[chosen])).unwrap();
                chosen += 1;
            }
            Decision::CastSpellOrPass { player: p, .. } => {
                assert_eq!(p, player);
                assert_eq!(chosen, targets.len());
                assert!(state.engine.pending_cast.is_none());
                assert_eq!(tapped(state, player) - before, mana);
                assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
                assert!(state
                    .stack
                    .iter()
                    .any(|row| row.source == card && row.kind == StackItemKind::Spell));
                assert!(state
                    .engine
                    .event_history
                    .iter()
                    .any(|e| matches!(e,CommittedEvent::SpellCast{spell,..} if *spell==card)));
                return card;
            }
            other => panic!("paid cast {name}: {other:?}"),
        }
    }
    panic!("cast never published")
}
fn until(state: &mut GameState, done: impl Fn(&GameState) -> bool) -> Decision {
    for _ in 0..128 {
        let d = next(state);
        if done(state) {
            return d;
        }
        pass(state, d);
    }
    panic!("native boundary absent")
}
pub(crate) fn finish(state: &mut GameState) {
    until(state, |s| {
        s.stack.is_empty()
            && s.engine.pending_effect.is_none()
            && s.engine.pending_triggers.is_empty()
    });
}
pub(crate) fn end_turn(state: &mut GameState) {
    until(state, |s| {
        s.engine.time_stop_v1.is_empty()
            && s.engine
                .event_history
                .has_any(&[crate::event_history_v1::EventKindV1::TimeStopJournalV1])
    });
}
fn refuses(mut state: GameState) {
    let bytes = serde_json::to_vec(&state).unwrap();
    assert!(validate(&state).is_err());
    assert!(step(&mut state, Action::Pass).is_err());
    assert_eq!(serde_json::to_vec(&state).unwrap(), bytes);
    assert!(validate(&restore(&state)).is_err());
}

#[test]
fn paid_stack_exile_skips_end_step_and_preserves_exact_restore_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let lower = cast(&mut state, player, "Think Twice", &[], 2);
        let stop = cast(&mut state, player, "Time Stop", &[], 6);
        let original = state.stack.clone();
        let mut replay = restore(&state);
        let start = state.engine.event_history.len();
        for branch in [&mut state, &mut replay] {
            end_turn(branch);
            assert_eq!(branch.objects.get(lower).zone, Zone::Exile);
            assert_eq!(branch.objects.get(stop).zone, Zone::Exile);
            assert_eq!(branch.active_player, player.opponent());
            assert!(!branch.engine.event_history[start..]
                .iter()
                .any(|e| matches!(e, CommittedEvent::BeginningEndStep { .. })));
            for row in &original {
                assert!(disposed(branch, row).unwrap());
            }
            validate(branch).unwrap();
            for viewer in [player, player.opponent()] {
                assert!(crate::rl::observe_v1(branch, viewer, 0).is_err());
            }
        }
        assert_eq!(state, replay);
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}

#[test]
fn paid_uncounterable_spell_is_exiled_and_countered_time_stop_does_not_end_turn_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let koma = cast(&mut state, player, "Koma, World-Eater", &[], 7);
        let counter = cast(
            &mut state,
            player,
            "Counterspell",
            &[Target::Object(koma)],
            2,
        );
        until(&mut state, |s| {
            s.objects.get(counter).zone == Zone::Graveyard
        });
        assert_eq!(state.objects.get(koma).zone, Zone::Stack);
        cast(&mut state, player, "Time Stop", &[], 6);
        end_turn(&mut state);
        assert_eq!(state.objects.get(koma).zone, Zone::Exile);

        let mut state = ready(player);
        let stop = cast(&mut state, player, "Time Stop", &[], 6);
        cast(
            &mut state,
            player.opponent(),
            "Counterspell",
            &[Target::Object(stop)],
            2,
        );
        finish(&mut state);
        assert_eq!(state.objects.get(stop).zone, Zone::Graveyard);
        assert_eq!(state.active_player, player);
        assert_eq!(state.step, Step::Main1);
        assert!(!state
            .engine
            .event_history
            .has_any(&[crate::event_history_v1::EventKindV1::TimeStopJournalV1]));
    }
}

#[test]
fn paid_cleanup_discard_precedes_damage_and_duration_expiration_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let creature = put(&mut state, player, "Axgard Cavalry", Zone::Battlefield);
        cast(
            &mut state,
            player,
            "Giant Growth",
            &[Target::Object(creature)],
            1,
        );
        finish(&mut state);
        // Native noncombat damage primitive, not a paid removal spell.
        event::propose_and_commit(
            &mut state,
            ProposedEvent::damage(creature, Target::Object(creature), 1),
        );
        // The native library constructor starts with an empty hand.
        // Twelve real hand cards leave the exact five-card cleanup obligation.
        for _ in 0..12 {
            put(&mut state, player, "Forest", Zone::Hand);
        }
        cast(&mut state, player, "Time Stop", &[], 6);
        let decision = until(&mut state, |s| {
            s.engine
                .pending_discard
                .as_ref()
                .is_some_and(|d| d.resume == DiscardResume::TimeStopCleanupV1)
        });
        let Decision::Discard {
            player: p,
            count,
            choices,
        } = decision
        else {
            panic!("cleanup discard absent")
        };
        assert_eq!(p, player);
        assert_eq!(count, 5);
        assert_eq!(state.step, Step::Cleanup);
        assert_eq!(state.objects.get(creature).damage, 1);
        assert_eq!(checked_effective_pt(&state, creature).unwrap(), (5, 5));
        let before = restore(&state);
        let mut replay = restore(&state);
        let chosen = choices[..count as usize].to_vec();
        for branch in [&mut state, &mut replay] {
            step(branch, Action::Discard(chosen.clone())).unwrap();
            end_turn(branch);
            assert_eq!(branch.objects.get(creature).damage, 0);
            assert_eq!(checked_effective_pt(branch, creature).unwrap(), (2, 2));
            assert_eq!(branch.players[player.index()].hand.len(), 7);
        }
        assert_eq!(state, replay);
        let mut original = before.clone();
        let saved = original
            .engine
            .event_history
            .iter_mut()
            .find_map(|e| match e {
                CommittedEvent::TimeStopJournalV1 { journal }
                    if matches!(journal.operation, OperationV1::Dispose { .. }) =>
                {
                    Some(journal)
                }
                _ => None,
            })
            .unwrap();
        let mut input: GameState = serde_json::from_slice(&saved.original.bytes).unwrap();
        input.stack[0].controller = player.opponent();
        saved.original.bytes = serde_json::to_vec(&input).unwrap();
        refuses(original);
        let mut duplicated = before.clone();
        let saved = duplicated
            .engine
            .event_history
            .iter()
            .find(|e| matches!(e, CommittedEvent::TimeStopJournalV1 { .. }))
            .unwrap()
            .clone();
        duplicated.engine.event_history.push(saved);
        refuses(duplicated);
        let mut phase = before.clone();
        phase.engine.time_stop_v1.0.as_mut().unwrap().phase = PhaseV1::Expiration;
        assert_ne!(before.state_hash(), phase.state_hash());
        refuses(phase);
        let mut erased = before.clone();
        erased.engine.pending_discard = None;
        refuses(erased);
        let mut changed = before.clone();
        let event = changed
            .engine
            .event_history
            .iter_mut()
            .rev()
            .find_map(|e| match e {
                CommittedEvent::TimeStopJournalV1 { journal } => Some(journal),
                _ => None,
            })
            .unwrap();
        event.output.bytes.push(0);
        refuses(changed);
        let mut log = before;
        let journal = log
            .engine
            .event_history
            .iter()
            .find(|e| matches!(e, CommittedEvent::TimeStopJournalV1 { .. }))
            .unwrap()
            .clone();
        log.engine.event_log.push(journal);
        refuses(log);
    }
}

#[test]
fn paid_cleanup_death_trigger_gets_priority_then_another_cleanup_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let creature = cast(&mut state, player, "Clockwork Percussionist", &[], 1);
        finish(&mut state);
        cast(
            &mut state,
            player,
            "Giant Growth",
            &[Target::Object(creature)],
            1,
        );
        finish(&mut state);
        // Native -1/-1 counter setup. The paid +3/+3 keeps this actual creature
        // alive until 514.2 expiration. Its printed dies ability remains active.
        state.objects.get_mut(creature).counters.minus1_minus1 = 2;
        let original_library = state.players[player.index()].library.len();
        cast(&mut state, player, "Time Stop", &[], 6);
        until(&mut state, |s| at_priority(s) && !s.stack.is_empty());
        assert_eq!(state.step, Step::Cleanup);
        assert_eq!(state.objects.get(creature).zone, Zone::Graveyard);
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            end_turn(branch);
            assert_eq!(branch.objects.get(creature).zone, Zone::Graveyard);
            assert_eq!(
                branch.players[player.index()].library.len(),
                original_library - 1
            );
            assert_eq!(branch.objects.get(creature).counters.minus1_minus1, 0);
            assert!(branch.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::TimeStopJournalV1{journal} if matches!(journal.operation,OperationV1::Repeat))));
            assert_eq!(branch.active_player, player.opponent());
        }
        assert_eq!(state, replay);
    }
}

#[test]
fn paid_hatcher_and_garruk_publications_end_without_countered_relabelling_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let hatcher = cast(&mut state, player, "Drake Hatcher", &[], 2);
        finish(&mut state);
        // Existing genuine native combat-commit primitive, not a claimed
        // declared attack: it captures final damage and the original source.
        commit_combat_damage_events(
            &mut state,
            vec![ProposedEvent::damage(
                hatcher,
                Target::Player(player.opponent()),
                1,
            )],
        );
        until(&mut state, |s| {
            s.stack
                .iter()
                .any(|r| r.source == hatcher && r.kind == StackItemKind::TriggeredAbility)
        });
        let original = state.stack.clone();
        cast(&mut state, player, "Time Stop", &[], 6);
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            end_turn(branch);
            crate::drake_hatcher_v1::custody::validate(branch).unwrap();
            for row in &original {
                assert!(disposed(branch, row).unwrap());
            }
        }
        assert_eq!(state, replay);
        // Warm genuine Drake validation, then mutate the active-state owner.
        // The new Some state contributes to state_hash and is still validated
        // before any memo-backed family check or native mutation.
        crate::drake_hatcher_v1::custody::validate(&state).unwrap();
        let mut warm = state.clone();
        warm.engine.time_stop_v1 = StateV1(Some(CleanupV1 {
            disposal: 0,
            turn: warm.turn_key_v1(),
            active: warm.active_player,
            phase: PhaseV1::Priority,
        }));
        assert_ne!(state.state_hash(), warm.state_hash());
        refuses(warm);

        let mut state = ready(player);
        cast(&mut state, player, "Garruk's Uprising", &[], 3);
        finish(&mut state);
        cast(&mut state, player, "Serra Angel", &[], 5);
        until(&mut state, |s| {
            s.stack
                .iter()
                .any(|r| r.kind == StackItemKind::TriggeredAbility)
        });
        let original = state.stack.clone();
        cast(&mut state, player, "Time Stop", &[], 6);
        end_turn(&mut state);
        crate::garruks_uprising_v1::validate_state(&state).unwrap();
        for row in &original {
            assert!(disposed(&state, row).unwrap());
        }
    }
}

#[test]
fn paid_storm_virtual_time_stop_exiles_original_and_copy_then_copy_ceases_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        cast(&mut state, player, "Thousand-Year Storm", &[], 6);
        finish(&mut state);
        cast(&mut state, player, "Think Twice", &[], 2);
        finish(&mut state);
        let original = cast(&mut state, player, "Time Stop", &[], 6);
        until(&mut state, |s| {
            s.stack.iter().any(|r| {
                r.is_copy && r.source != original && definition(s.objects.get(r.source).card_def)
            })
        });
        let copy = state
            .stack
            .iter()
            .find(|r| r.is_copy && definition(state.objects.get(r.source).card_def))
            .unwrap()
            .clone();
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            end_turn(branch);
            assert_eq!(branch.objects.get(original).zone, Zone::Exile);
            assert!(!branch.exile.contains(&copy.source));
            assert!(branch.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::ZoneChange{object,from:Zone::Stack,to:Zone::Exile,..} if *object==copy.source)));
            assert!(disposed(branch, &copy).unwrap());
        }
        assert_eq!(state, replay);
    }
}

#[test]
fn paid_independent_hidden_worlds_and_public_direct_program_refusal_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut a = ready_hidden(player, "Forest");
        let mut b = ready_hidden(player, "Time Stop");
        assert!(!serde_json::to_string(&a)
            .unwrap()
            .contains("\"time_stop_v1\""));
        assert_eq!(
            crate::rl::observe_v1(&a, player, 0).unwrap(),
            crate::rl::observe_v1(&b, player, 0).unwrap()
        );
        let ca = cast(&mut a, player, "Time Stop", &[], 6);
        let cb = cast(&mut b, player, "Time Stop", &[], 6);
        assert_eq!(ca, cb);
        assert_eq!(a.stack, b.stack);
        let mut foreign = a.clone();
        let mut row = foreign.stack.last().unwrap().clone();
        row.controller = player.opponent();
        let bytes = serde_json::to_vec(&foreign).unwrap();
        assert!(begin(&mut foreign, &row).is_err());
        assert_eq!(serde_json::to_vec(&foreign).unwrap(), bytes);
        let mut direct = a.clone();
        let ctx = effect::ExecCtx::no_targets(ca, player);
        effect::execute(
            &effect::EffectOp::StandardV1(crate::standard_cards_v1::StandardOpV1::EndTurnV1),
            &ctx,
            &mut direct,
        );
        assert!(
            matches!(direct.engine.halted,Some((UnsupportedMechanic::InvalidEffectContinuation,source)) if source==ca)
        );
        assert!(direct.engine.time_stop_v1.is_empty());
        end_turn(&mut a);
        end_turn(&mut b);
        assert_eq!(a.objects.get(ca).zone, b.objects.get(cb).zone);
        assert_eq!(a.active_player, b.active_player);
        assert_eq!(
            a.players[player.index()].life,
            b.players[player.index()].life
        );
    }
}

#[test]
fn paid_ajani_boost_expires_at_actual_cleanup_without_invented_end_step_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let ajani = cast(&mut state, player, "Ajani, Caller of the Pride", &[], 3);
        finish(&mut state);
        let target = cast(&mut state, player, "Axgard Cavalry", &[], 2);
        finish(&mut state);
        priority(&mut state, player);
        assert!(
            matches!(next(&mut state),Decision::CastSpellOrPass{activatable_abilities,..} if activatable_abilities.contains(&(ajani,1)))
        );
        step(&mut state, Action::ActivateAbility(ajani, 1)).unwrap();
        assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
        step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        finish(&mut state);
        assert!(has_effective_keyword(
            &state,
            target,
            card_def::Keywords::FLYING
        ));
        assert!(has_effective_keyword(
            &state,
            target,
            card_def::Keywords::DOUBLE_STRIKE
        ));
        cast(&mut state, player, "Time Stop", &[], 6);
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            end_turn(branch);
            assert!(!has_effective_keyword(
                branch,
                target,
                card_def::Keywords::FLYING
            ));
            assert!(!has_effective_keyword(
                branch,
                target,
                card_def::Keywords::DOUBLE_STRIKE
            ));
            crate::ajani_full_v42::lifecycle::validate_state(branch).unwrap();
            assert!(branch.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::AjaniJournalV42{journal} if matches!(journal.as_ref(),crate::ajani_full_v42::lifecycle::JournalV42::EndTurnCleanupInput{..}))));
        }
        assert_eq!(state, replay);
    }
}

#[test]
fn paid_delayed_sacrifice_waits_for_next_real_end_step_after_time_stop_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        let target = cast(&mut state, player, "Axgard Cavalry", &[], 2);
        finish(&mut state);
        cast(
            &mut state,
            player,
            "Electroduplicate",
            &[Target::Object(target)],
            3,
        );
        finish(&mut state);
        let token = *state.players[player.index()]
            .battlefield
            .iter()
            .find(|o| state.objects.get(**o).v4.copied_end_step_sacrifices_v1 == 1)
            .unwrap();
        cast(&mut state, player, "Time Stop", &[], 6);
        end_turn(&mut state);
        assert_eq!(state.objects.get(token).zone, Zone::Battlefield);
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            until(branch, |s| {
                !s.players[player.index()].battlefield.contains(&token)
            });
            assert!(branch.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::BeginningEndStep{active_player:p,..} if *p==player.opponent())));
        }
        assert_eq!(state, replay);
    }
}

#[test]
fn paid_heartfire_disposal_cannot_gain_a_second_finished_or_live_outcome_both_seats() {
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(player);
        // Native opposing-target setup; both new spells and the activation
        // use their real native announcement, target and paid-cost paths.
        let target = put(
            &mut state,
            player.opponent(),
            "Axgard Cavalry",
            Zone::Battlefield,
        );
        let source = cast(&mut state, player, "Heartfire Immolator", &[], 2);
        finish(&mut state);
        priority(&mut state, player);
        let taps = tapped(&state, player);
        assert!(
            matches!(next(&mut state),Decision::CastSpellOrPass{activatable_abilities,..} if activatable_abilities.contains(&(source,0)))
        );
        step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
        assert!(
            matches!(next(&mut state),Decision::ChooseTargets{legal_targets,..} if legal_targets.contains(&Target::Object(target)))
        );
        step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        assert!(matches!(next(&mut state),Decision::CastSpellOrPass{player:p,..} if p==player));
        assert!(state.engine.pending_activation.is_none());
        assert_eq!(tapped(&state, player) - taps, 1);
        assert_eq!(state.players[player.index()].mana_pool, [0; 6]);
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        let original = state
            .stack
            .iter()
            .find(|r| r.source == source && r.kind == StackItemKind::ActivatedAbility)
            .unwrap()
            .clone();
        cast(&mut state, player, "Time Stop", &[], 6);
        let mut replay = restore(&state);
        for branch in [&mut state, &mut replay] {
            end_turn(branch);
            assert!(disposed(branch, &original).unwrap());
            crate::source_power_activation_v1::validate_state(branch).unwrap();
            assert_eq!(branch.players[player.opponent().index()].life, 20);
            assert_eq!(branch.objects.get(target).zone, Zone::Battlefield);
            assert_eq!(branch.objects.get(target).damage, 0);
        }
        assert_eq!(state, replay);
        // Append only a forged suffix. All original paid history, disposal
        // inputs, outputs and their actual row remain byte-for-byte intact.
        for finished in [true, false] {
            let mut forged = restore(&state);
            if finished {
                forged
                    .engine
                    .event_history
                    .push(CommittedEvent::SourcePowerActivationFinishedV1 {
                        stack_item_id: original.v4.stack_item_id,
                    });
            } else {
                forged.stack.push(original.clone());
            }
            let bytes = serde_json::to_vec(&forged).unwrap();
            assert!(crate::source_power_activation_v1::validate_state(&forged).is_err());
            assert!(step(&mut forged, Action::Pass).is_err());
            assert_eq!(serde_json::to_vec(&forged).unwrap(), bytes);
            assert!(crate::source_power_activation_v1::validate_state(&restore(&forged)).is_err());
        }
    }
}
