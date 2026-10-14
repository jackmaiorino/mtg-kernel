//! Future printed acceptance requires actual registration, without injected roots.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CardType, Keywords, Subtype, CARD_DEFS};
use mtg_kernel::effect::{self, EffectCond, EffectOp};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::{ManaColor, Pip};
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name)
        .unwrap_or_else(|| panic!("printed acceptance requires registration: {name}"));
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner,
        controller: owner,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[owner.index()].hand.push(id),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
        _ => panic!("fixture zone"),
    }
    id
}
fn ready(actor: PlayerId, size: usize) -> GameState {
    let cards = ["Forest", "Plains", "Island", "Mountain", "Swamp"]
        .map(|name| card_id_by_name(name).unwrap());
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &cards[..size],
        &cards[..size],
        |id| CARD_DEFS[id as usize].name.into(),
        11006,
        actor,
    );
    state.step = Step::Main1;
    for seat in [actor, actor.opponent()] {
        put(&mut state, seat, "Mountain", Zone::Battlefield);
        put(&mut state, seat, "Lightning Bolt", Zone::Hand);
    }
    state
}
fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}
fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..40 {
        let decision = next(state);
        match decision {
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None
            }
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => return Some(other),
        }
    }
    panic!("resolution did not settle");
}
fn raid(state: &mut GameState, actor: PlayerId, nonempty: bool) {
    let attacker = put(state, actor, "Llanowar Elves", Zone::Battlefield);
    state.step = Step::DeclareAttackers;
    assert!(matches!(next(state), Decision::DeclareAttackers { player, .. } if player == actor));
    engine::step(
        state,
        Action::DeclareAttackers(if nonempty { vec![attacker] } else { vec![] }),
    )
    .unwrap();
    for _ in 0..40 {
        let decision = next(state);
        if state.step == Step::Main2
            && matches!(decision, Decision::CastSpellOrPass { player, .. } if player == actor)
        {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::DeclareBlockers { .. } => {
                engine::step(state, Action::DeclareBlockers(vec![])).unwrap()
            }
            other => panic!("unexpected combat decision {other:?}"),
        }
    }
    panic!("Main2 did not retain a castable decision");
}
fn cast(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let source = put(state, actor, "Gorehorn Raider", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 4;
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&source))
    );
    engine::step(state, Action::CastSpell(source)).unwrap();
    assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
    assert!(state.stack.iter().any(|item| item.source == source));
    source
}
fn replay(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let restored: GameState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&restored).unwrap());
    assert_eq!(state.state_hash(), restored.state_hash());
    restored
}

fn target_trigger(state: &mut GameState, actor: PlayerId, target: Target) {
    assert!(
        matches!(settle(state), Some(Decision::ChooseTargets { player, legal_targets, .. })
        if player == actor && legal_targets.contains(&target))
    );
    engine::step(state, Action::ChooseTarget(target)).unwrap();
    let _ = next(state);
    assert!(state
        .stack
        .iter()
        .any(|item| item.kind == mtg_kernel::state::StackItemKind::TriggeredAbility));
}
#[test]
fn printed_raid_root_and_precise_meaning_are_exact_and_rewrites_refuse() {
    let id = card_id_by_name("Gorehorn Raider").expect("actual registration required");
    let def = &CARD_DEFS[id as usize];
    assert_eq!(def.types, &[CardType::Creature]);
    assert_eq!(def.subtypes, &[Subtype::Minotaur, Subtype::Pirate]);
    assert_eq!(
        (def.cost.generic, def.mana_value, def.power, def.toughness),
        (4, 5, Some(4), Some(4))
    );
    assert_eq!(def.cost.pips, &[Pip::Colored(ManaColor::R)]);
    assert_eq!(def.keywords, Keywords::NONE);
    assert_eq!(
        (mtg_kernel::trigger::triggers_for(id)[0].effect)(),
        EffectOp::Conditional {
            cond: EffectCond::ControllerAttackedThisTurn,
            then: Box::new(EffectOp::DealDamage {
                target: mtg_kernel::effect::TargetRef::Target(0),
                amount: 2
            }),
            else_: Box::new(EffectOp::Sequence(vec![]))
        }
    );
    let selector = mtg_kernel::rules_meaning_raid_entry_v1::selector_v1().unwrap();
    let meaning = mtg_kernel::rules_meaning_raid_entry_v1::meaning_v1(&selector, id, 0).unwrap();
    assert!(mtg_kernel::rules_meaning_raid_entry_v1::meaning_v1(&selector, id, 1).is_err());
    for mode in 0..4 {
        let mut bad = meaning.clone();
        match mode {
            0 => bad.original_controller = false,
            1 => bad.resolution_recheck = false,
            2 => bad.selector.card_db_hash ^= 1,
            _ => {
                bad.instruction =
                    mtg_kernel::rules_meaning_raid_entry_v1::InstructionV1::DamageAnyTarget {
                        amount: 3,
                        players: true,
                        creatures: true,
                        planeswalkers: true,
                    }
            }
        }
        assert!(mtg_kernel::rules_meaning_raid_entry_v1::validate_v1(&bad).is_err());
    }
}
#[test]
fn real_both_seat_cast_and_raid_target_player_or_creature_restore_damage_two() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for creature in [false, true] {
            let mut state = ready(actor, 5);
            raid(&mut state, actor, true);
            let recipient = put(
                &mut state,
                actor.opponent(),
                "Serra Angel",
                Zone::Battlefield,
            );
            cast(&mut state, actor);
            let target = if creature {
                Target::Object(recipient)
            } else {
                Target::Player(actor.opponent())
            };
            target_trigger(&mut state, actor, target);
            let before = state.players[actor.opponent().index()].life;
            let mut restored = replay(&state);
            assert!(settle(&mut state).is_none());
            assert!(settle(&mut restored).is_none());
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            if creature {
                assert_eq!(state.objects.get(recipient).damage, 2);
                assert_eq!(state.players[actor.opponent().index()].life, before);
            } else {
                assert_eq!(state.players[actor.opponent().index()].life, before - 2);
                assert_eq!(state.objects.get(recipient).damage, 0);
            }
        }
    }
}
#[test]
fn real_empty_or_missing_attack_has_no_trigger_and_stale_raid_fails_resolution() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for mode in 0..3 {
            let mut state = ready(actor, 5);
            if mode > 0 {
                raid(&mut state, actor, mode == 2);
            }
            cast(&mut state, actor);
            if mode == 2 {
                target_trigger(&mut state, actor, Target::Player(actor.opponent()));
                state.attack_declaration_turn_v1 = None;
            }
            let before = state.players[actor.opponent().index()].life;
            assert!(settle(&mut state).is_none());
            assert_eq!(state.players[actor.opponent().index()].life, before);
        }
    }
}
#[test]
fn captured_source_departure_does_not_cancel_damage_but_target_blink_does() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for blink in [false, true] {
            let mut state = ready(actor, 5);
            raid(&mut state, actor, true);
            let recipient = put(
                &mut state,
                actor.opponent(),
                "Serra Angel",
                Zone::Battlefield,
            );
            let source = cast(&mut state, actor);
            target_trigger(&mut state, actor, Target::Object(recipient));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Graveyard),
            );
            if blink {
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(recipient, Zone::Exile),
                );
                event::propose_and_commit(
                    &mut state,
                    ProposedEvent::zone_change(recipient, Zone::Battlefield),
                );
            }
            let mut restored = replay(&state);
            assert!(settle(&mut state).is_none());
            assert!(settle(&mut restored).is_none());
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            assert_eq!(
                state.objects.get(recipient).damage,
                if blink { 0 } else { 2 }
            );
        }
    }
}
#[test]
fn original_entry_controller_receipt_rejects_coherent_live_trigger_rewrite_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, 5);
        raid(&mut state, actor, true);
        cast(&mut state, actor);
        target_trigger(&mut state, actor, Target::Player(actor.opponent()));
        let item = state
            .stack
            .iter_mut()
            .find(|item| item.kind == mtg_kernel::state::StackItemKind::TriggeredAbility)
            .unwrap();
        item.controller = actor.opponent();
        item.v4.ability_source_contract.as_mut().unwrap().controller = actor.opponent();
        let mut restored = replay(&state);
        let before = serde_json::to_vec(&restored).unwrap();
        assert!(effect::validate_pending_effect_choice(&restored).is_err());
        assert!(engine::step(&mut restored, Action::Pass).is_err());
        assert_eq!(before, serde_json::to_vec(&restored).unwrap());
    }
}

fn grant(state: &mut GameState, object: ObjectId, keyword: Keywords) {
    let controller = state.objects.get(object).controller;
    effect::execute(
        &EffectOp::GrantKeywordTargetUntilEndOfTurn {
            object: mtg_kernel::effect::ObjectRef::ThisSource,
            keyword,
        },
        &mtg_kernel::effect::ExecCtx::no_targets(object, controller),
        state,
    );
    assert!(state.engine.halted.is_none());
}
/// Existing definition-independent typed control primitive, not a printed
/// Lightning Bolt control spell. Actual Gorehorn cast/root/resolution remain real.
fn control_primitive(state: &mut GameState, object: ObjectId, controller: PlayerId) {
    let source = put(state, controller, "Lightning Bolt", Zone::Hand);
    let mut ctx = mtg_kernel::effect::ExecCtx::no_targets(source, controller);
    ctx.targets.push(Target::Object(object));
    ctx.target_contracts
        .push(mtg_kernel::state::StackTargetContractV4::capture(
            state,
            Target::Object(object),
        ));
    effect::execute(
        &EffectOp::GainControlOfTargetCreatureUntilEndOfTurn { target_index: 0 },
        &ctx,
        state,
    );
    assert!(state.engine.halted.is_none());
    assert_eq!(state.objects.get(object).controller, controller);
}
fn resolve_one(state: &mut GameState) {
    let count = state.stack.len();
    for _ in 0..16 {
        if state.stack.len() < count {
            return;
        }
        assert!(matches!(next(state), Decision::CastSpellOrPass { .. }));
        engine::step(state, Action::Pass).unwrap();
    }
    panic!("top item did not resolve");
}
fn bounce_with_real_snap(state: &mut GameState, actor: PlayerId, source: ObjectId) {
    let own_land = put(state, actor, "Island", Zone::Battlefield);
    let opponent_land = put(state, actor.opponent(), "Island", Zone::Battlefield);
    for land in [own_land, opponent_land] {
        event::propose_and_commit(state, ProposedEvent::tap(land));
    }
    let bounce = put(state, actor, "Snap", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 1;
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { player, castable_spells, .. }
        if player == actor && castable_spells.contains(&bounce))
    );
    engine::step(state, Action::CastSpell(bounce)).unwrap();
    assert!(
        matches!(next(state), Decision::ChooseTargets { legal_targets, .. }
        if legal_targets.contains(&Target::Object(source)))
    );
    engine::step(state, Action::ChooseTarget(Target::Object(source))).unwrap();
    let _ = next(state); // Finalize the genuine cast before ordinary resolution.
    let mut choice = None;
    for _ in 0..16 {
        match next(state) {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            decision @ Decision::ChooseEffectTargets { .. } => {
                choice = Some(decision);
                break;
            }
            other => panic!("Snap resolution returned {other:?}"),
        }
    }
    let choice = choice.expect("real Snap land continuation did not appear");
    assert!(matches!(choice, Decision::ChooseEffectTargets {
        player, source: chosen_source, selected_count: 0, min_targets: 0, max_targets: 2,
        legal_targets, can_finish: true
    } if player == actor && chosen_source == bounce
        && legal_targets.contains(&Target::Object(own_land))
        && legal_targets.contains(&Target::Object(opponent_land))));
    assert_eq!(state.objects.get(source).zone, Zone::Hand);
    let mut restored = replay(state);
    for current in [&mut *state, &mut restored] {
        engine::step(
            current,
            Action::ChooseEffectTarget(Target::Object(own_land)),
        )
        .unwrap();
        assert!(matches!(next(current), Decision::ChooseEffectTargets {
            player, source: chosen_source, selected_count: 1, min_targets: 0, max_targets: 2,
            legal_targets, can_finish: true
        } if player == actor && chosen_source == bounce
            && legal_targets.contains(&Target::Object(opponent_land))));
        engine::step(
            current,
            Action::ChooseEffectTarget(Target::Object(opponent_land)),
        )
        .unwrap();
        let _ = next(current);
        assert_eq!(current.objects.get(bounce).zone, Zone::Graveyard);
        assert!(!current.objects.get(own_land).tapped);
        assert!(!current.objects.get(opponent_land).tapped);
        assert!(current.engine.pending_effect.is_none());
    }
    assert_eq!(
        serde_json::to_vec(state).unwrap(),
        serde_json::to_vec(&restored).unwrap()
    );
}
#[test]
fn actual_bounce_reentry_uses_original_departure_lifelink_deathtouch_not_new_incarnation() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, 5);
        raid(&mut state, actor, true);
        let recipient = put(
            &mut state,
            actor.opponent(),
            "Serra Angel",
            Zone::Battlefield,
        );
        let source = cast(&mut state, actor);
        target_trigger(&mut state, actor, Target::Object(recipient));
        grant(&mut state, source, Keywords::LIFELINK);
        grant(&mut state, source, Keywords::DEATHTOUCH);
        bounce_with_real_snap(&mut state, actor, source);
        // Genuine event primitive reentry, with Raid off to isolate the old trigger.
        let attack = state.attack_declaration_turn_v1.take();
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change_to_battlefield_under_controller(source, actor.opponent()),
        );
        state.attack_declaration_turn_v1 = attack;
        assert!(state.objects.get(source).zone_change_count > 1);
        assert_eq!(state.objects.get(source).controller, actor.opponent());
        assert!(!engine::has_effective_keyword(
            &state,
            source,
            Keywords::LIFELINK
        ));
        let before = state.players[actor.index()].life;
        let opponent = state.players[actor.opponent().index()].life;
        let mut restored = replay(&state);
        assert!(settle(&mut state).is_none());
        assert!(settle(&mut restored).is_none());
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        assert_eq!(state.players[actor.index()].life, before + 2);
        assert_eq!(state.players[actor.opponent().index()].life, opponent);
        assert_eq!(state.objects.get(recipient).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(source).controller, actor.opponent());
    }
}
#[test]
fn live_source_current_controller_and_keywords_change_without_changing_ability_controller() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, 5);
        raid(&mut state, actor, true);
        let source = cast(&mut state, actor);
        target_trigger(&mut state, actor, Target::Player(actor.opponent()));
        grant(&mut state, source, Keywords::LIFELINK);
        control_primitive(&mut state, source, actor.opponent());
        let item = state.stack.last().unwrap();
        assert_eq!(item.controller, actor);
        assert_eq!(item.v4.ability_source_contract.unwrap().controller, actor);
        let actor_life = state.players[actor.index()].life;
        let opponent_life = state.players[actor.opponent().index()].life;
        let mut restored = replay(&state);
        assert!(settle(&mut state).is_none());
        assert!(settle(&mut restored).is_none());
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
        assert_eq!(state.players[actor.index()].life, actor_life);
        assert_eq!(state.players[actor.opponent().index()].life, opponent_life);
        assert!(state
            .engine
            .event_history
            .iter()
            .any(|event| matches!(event,
   event::CommittedEvent::LifeGain{player,amount:2} if *player==actor.opponent())));
    }
}
#[test]
fn original_red_departure_obeys_color_prevention_and_complete_numeric_shield_after_restore() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for numeric in [false, true] {
            let mut state = ready(actor, 5);
            raid(&mut state, actor, true);
            let source = cast(&mut state, actor);
            target_trigger(&mut state, actor, Target::Player(actor.opponent()));
            grant(&mut state, source, Keywords::LIFELINK);
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(source, Zone::Graveyard),
            );
            if numeric {
                state.engine.next_replacement_id += 1;
                state
                    .engine
                    .active_replacements
                    .push(event::ActiveReplacement {
                        id: state.engine.next_replacement_id,
                        source,
                        kind: event::ReplacementEffectKind::PreventNextDamage {
                            target: Target::Player(actor.opponent()),
                            remaining: 2,
                        },
                    });
            } else {
                event::install_color_damage_prevention(&mut state, source, ManaColor::R).unwrap();
            }
            let before = [state.players[0].life, state.players[1].life];
            let mut restored = replay(&state);
            assert!(settle(&mut state).is_none());
            assert!(settle(&mut restored).is_none());
            assert_eq!(before, [state.players[0].life, state.players[1].life]);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
            assert!(state
                .engine
                .event_history
                .iter()
                .any(|event| matches!(event,
    event::CommittedEvent::GorehornDamageOperationV1{operation} if operation.output.is_none())));
        }
    }
}
#[test]
fn gorehorn_publication_preparation_and_actual_damage_operation_erasure_refuse_readonly() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut original = ready(actor, 5);
        raid(&mut original, actor, true);
        cast(&mut original, actor);
        target_trigger(&mut original, actor, Target::Player(actor.opponent()));
        for mode in 0..3 {
            let mut bad = replay(&original);
            match mode {
                0 => {
                    bad.stack.last_mut().unwrap().v4.gorehorn_published_v1.0 = None;
                    bad.engine.event_history.retain(|e| {
                        !matches!(e, event::CommittedEvent::GorehornPublishedV1 { .. })
                    });
                }
                1 => {
                    bad.stack.clear();
                }
                _ => {
                    bad.stack.last_mut().unwrap().targets = vec![Target::Player(actor)];
                    bad.stack.last_mut().unwrap().v4.target_contracts =
                        vec![mtg_kernel::state::StackTargetContractV4::capture(
                            &original,
                            Target::Player(actor),
                        )];
                }
            }
            let before = serde_json::to_vec(&bad).unwrap();
            assert!(effect::validate_pending_effect_choice(&bad).is_err());
            assert!(engine::step(&mut bad, Action::Pass).is_err());
            assert_eq!(before, serde_json::to_vec(&bad).unwrap());
        }
        assert!(settle(&mut original).is_none());
        for mode in 0..3 {
            let mut bad = replay(&original);
            match mode {
                0 => bad.engine.event_history.retain(|e| {
                    !matches!(e, event::CommittedEvent::GorehornDamageOperationV1 { .. })
                }),
                1 => bad
                    .engine
                    .event_history
                    .retain(|e| !matches!(e, event::CommittedEvent::GorehornPreparedV1 { .. })),
                _ => bad
                    .engine
                    .event_history
                    .retain(|e| !matches!(e, event::CommittedEvent::GorehornOperationV1 { .. })),
            }
            let before = serde_json::to_vec(&bad).unwrap();
            assert!(engine::step(&mut bad, Action::Pass).is_err());
            assert_eq!(before, serde_json::to_vec(&bad).unwrap());
        }
    }
}

#[test]
fn genuine_simultaneous_departure_preserves_prebatch_suppressed_lifelink_in_both_orders() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for witness_first in [false, true] {
            let mut state = ready(actor, 5);
            raid(&mut state, actor, true);
            let source = cast(&mut state, actor);
            target_trigger(&mut state, actor, Target::Player(actor.opponent()));
            grant(&mut state, source, Keywords::LIFELINK);
            assert!(engine::has_effective_keyword(
                &state,
                source,
                Keywords::LIFELINK
            ));
            // A registered flash enabler makes the actual enchantment response legal.
            put(&mut state, actor, "High Fae Trickster", Zone::Battlefield);
            let witness = put(&mut state, actor, "Witness Protection", Zone::Hand);
            state.players[actor.index()].mana_pool[ManaColor::U.pool_index()] = 1;
            assert!(
                matches!(next(&mut state),Decision::CastSpellOrPass{player,..} if player==actor)
            );
            engine::step(&mut state, Action::CastSpell(witness)).unwrap();
            assert!(
                matches!(next(&mut state),Decision::ChooseTargets{legal_targets,..}
    if legal_targets.contains(&Target::Object(source)))
            );
            engine::step(&mut state, Action::ChooseTarget(Target::Object(source))).unwrap();
            let _ = next(&mut state);
            resolve_one(&mut state);
            assert_eq!(state.objects.get(witness).zone, Zone::Battlefield);
            assert!(!engine::has_effective_keyword(
                &state,
                source,
                Keywords::LIFELINK
            ));
            let ids = if witness_first {
                [witness, source]
            } else {
                [source, witness]
            };
            event::propose_and_commit_batch(
                &mut state,
                ids.into_iter()
                    .map(|id| ProposedEvent::zone_change(id, Zone::Graveyard))
                    .collect(),
            );
            let captures = state
                .engine
                .event_history
                .iter()
                .filter_map(|e| match e {
                    event::CommittedEvent::GorehornDepartureV1 { departure }
                        if departure.facts.source.source == source =>
                    {
                        Some(departure)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(captures.len(), 1);
            assert!(!captures[0].facts.lifelink);
            let before = state.players[actor.index()].life;
            let opponent = state.players[actor.opponent().index()].life;
            let mut restored = replay(&state);
            assert!(settle(&mut state).is_none());
            assert!(settle(&mut restored).is_none());
            assert_eq!(state.players[actor.index()].life, before);
            assert_eq!(state.players[actor.opponent().index()].life, opponent - 2);
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}

#[test]
fn actual_rod_equip_and_noncreature_cast_on_gorehorn_preserves_unrelated_granted_trigger() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, 5);
        let source = cast(&mut state, actor);
        assert!(settle(&mut state).is_none());
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        let rod = put(&mut state, actor, "Black Mage's Rod", Zone::Battlefield);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        let _ = next(&mut state);
        engine::step(&mut state, Action::ActivateAbility(rod, 0)).unwrap();
        assert!(
            matches!(next(&mut state),Decision::ChooseTargets{legal_targets,..}
   if legal_targets.contains(&Target::Object(source)))
        );
        engine::step(&mut state, Action::ChooseTarget(Target::Object(source))).unwrap();
        let _ = next(&mut state);
        assert!(settle(&mut state).is_none());
        assert_eq!(
            state.objects.get(rod).v4.attached_to.unwrap().object,
            source
        );
        assert!(state.objects.get(source).attachments.contains(&rod));
        let bolt = state.players[actor.index()]
            .hand
            .iter()
            .copied()
            .find(|id| CARD_DEFS[state.objects.get(*id).card_def as usize].name == "Lightning Bolt")
            .unwrap();
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        let _ = next(&mut state);
        engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
        assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
        engine::step(
            &mut state,
            Action::ChooseTarget(Target::Player(actor.opponent())),
        )
        .unwrap();
        let _ = next(&mut state);
        assert!(state.stack.iter().any(|item| item.source == source
            && item.v4.granted_by.is_some_and(|grant| grant.source == rod)));
        assert!(!state
            .engine
            .event_history
            .iter()
            .any(|e| matches!(e, event::CommittedEvent::GorehornPublishedV1 { .. })));
        let before = state.players[actor.opponent().index()].life;
        let mut restored = replay(&state);
        assert!(settle(&mut state).is_none());
        assert!(settle(&mut restored).is_none());
        assert_eq!(state.players[actor.opponent().index()].life, before - 4);
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&restored).unwrap()
        );
    }
}
