//! Future actual printed acceptance, requiring parent-owned real registration.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, Keywords, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};

fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let def = card_id_by_name(name).expect("true future definition registration is required");
    let id = state.objects.push(GameObject {
        card_def: def,
        name: CARD_DEFS[usize::from(def)].object_name.into(),
        owner,
        controller: owner,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: vec![],
        v4: ObjectStateV4::from_card_def(def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[owner.index()].hand.push(id),
        Zone::Battlefield => state.players[owner.index()].battlefield.push(id),
        _ => panic!("unsupported fixture placement"),
    }
    id
}

fn ready(actor: PlayerId) -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state = GameState::new_from_libraries_with_starting_player_v1(
        &[forest; 40],
        &[forest; 40],
        |_| "Forest".into(),
        1127,
        actor,
    );
    state.step = Step::Main1;
    for player in [PlayerId::P0, PlayerId::P1] {
        put(&mut state, player, "Mountain", Zone::Battlefield);
        put(&mut state, player, "Lightning Bolt", Zone::Hand);
    }
    state
}

fn next(state: &mut GameState) -> Decision {
    let d = engine::advance_until_decision(state);
    assert!(!matches!(d, Decision::Halted { .. }), "{d:?}");
    d
}

fn priority(state: &mut GameState, actor: PlayerId) {
    for _ in 0..4 {
        let d = next(state);
        if matches!(d, Decision::CastSpellOrPass { player, .. } if player == actor) {
            return;
        }
        match d {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            Decision::ChooseEffectTargets {
                can_finish: true, ..
            } => engine::step(state, Action::FinishEffectSelection).unwrap(),
            other => panic!("unexpected completion decision {other:?}"),
        }
    }
    panic!("missing priority");
}

fn finish(state: &mut GameState) {
    for _ in 0..30 {
        let d = next(state);
        if state.stack.is_empty()
            && state.engine.pending_triggers.is_empty()
            && matches!(d, Decision::CastSpellOrPass { .. })
        {
            return;
        }
        match d {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            Decision::ChooseEffectTargets {
                can_finish: true, ..
            } => engine::step(state, Action::FinishEffectSelection).unwrap(),
            other => panic!("unexpected completion decision {other:?}"),
        }
    }
    panic!("resolution did not complete");
}

fn restore(state: &GameState) -> GameState {
    let bytes = serde_json::to_vec(state).unwrap();
    let value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(serde_json::to_vec(&value).unwrap(), bytes);
    value
}

fn cast(state: &mut GameState, actor: PlayerId, name: &str, target: Option<Target>) -> ObjectId {
    let card = put(state, actor, name, Zone::Hand);
    let (color, colored, generic) = match name {
        "Bloodthirsty Conqueror" => (ManaColor::B, 2, 3),
        "Lightning Bolt" => (ManaColor::R, 1, 0),
        "Witness Protection" => (ManaColor::U, 1, 0),
        "Snap" => (ManaColor::U, 1, 1),
        "Involuntary Employment" => (ManaColor::R, 1, 3),
        _ => panic!("fixture needs actual known printed cost"),
    };
    state.players[actor.index()].mana_pool[color.pool_index()] = colored;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = generic;
    priority(state, actor);
    engine::step(state, Action::CastSpell(card)).unwrap();
    if let Some(target) = target {
        assert!(
            matches!(next(state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&target))
        );
        engine::step(state, Action::ChooseTarget(target)).unwrap();
    }
    next(state); // Genuine announcement finalization, not only pending targets.
    assert!(state.stack.iter().any(|item| item.source == card));
    card
}
fn conqueror(state: &mut GameState, actor: PlayerId) -> ObjectId {
    let source = cast(state, actor, "Bloodthirsty Conqueror", None);
    finish(state);
    assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
    assert!(state.objects.get(source).summoning_sick);
    let definition = &CARD_DEFS[usize::from(state.objects.get(source).card_def)];
    assert_eq!(definition.power, Some(5));
    assert_eq!(definition.toughness, Some(5));
    assert_eq!(definition.keywords, Keywords::FLYING | Keywords::DEATHTOUCH);
    source
}
fn captures(state: &GameState) -> usize {
    state
        .engine
        .event_history
        .iter()
        .filter(|event| matches!(event, event::CommittedEvent::ConquerorCapturedV49 { .. }))
        .count()
}
fn until_spell_gone(state: &mut GameState, spell: ObjectId) {
    for _ in 0..40 {
        if !state.stack.iter().any(|item| item.source == spell)
            && state.objects.get(spell).zone != Zone::Stack
        {
            return;
        }
        match next(state) {
            Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
            Decision::ChooseEffectTargets {
                can_finish: true, ..
            } => engine::step(state, Action::FinishEffectSelection).unwrap(),
            Decision::OrderTriggers { pending, .. } => {
                engine::step(state, Action::OrderTriggers((0..pending.len()).collect())).unwrap()
            }
            other => panic!("unexpected real response {other:?}"),
        }
    }
    panic!("real response failed to finish");
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn actual_paid_conqueror_real_bolt_saved_gain_and_registered_pridemate_side_effect_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let pridemate = put(&mut state, actor, "Ajani's Pridemate", Zone::Battlefield);
        conqueror(&mut state, actor);
        let bolt = cast(
            &mut state,
            actor,
            "Lightning Bolt",
            Some(Target::Player(actor.opponent())),
        );
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            finish(current);
            assert_eq!(current.objects.get(bolt).zone, Zone::Graveyard);
            assert_eq!(current.players[actor.index()].life, 23);
            assert_eq!(current.players[actor.opponent().index()].life, 17);
            assert_eq!(current.objects.get(pridemate).counters.plus1_plus1, 1);
            assert_eq!(captures(current), 1);
            assert_eq!(current.engine.event_history.iter().filter(|event| matches!(event, event::CommittedEvent::LifeGain { player, amount: 3 } if *player == actor)).count(), 1);
        }
        assert_eq!(state, replay);
    }
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn actual_simultaneous_player_losses_sum_once_and_real_control_transfer_preserves_original_beneficiary(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        put(
            &mut state,
            actor.opponent(),
            "High Fae Trickster",
            Zone::Battlefield,
        );
        let source = conqueror(&mut state, actor);
        // Actual printed response changes current controller after original capture.
        event::propose_and_commit_batch(
            &mut state,
            vec![
                ProposedEvent::life_loss(actor.opponent(), 2),
                ProposedEvent::life_payment(actor.opponent(), 3),
            ],
        );
        next(&mut state);
        assert_eq!(captures(&state), 1);
        let employment = cast(
            &mut state,
            actor.opponent(),
            "Involuntary Employment",
            Some(Target::Object(source)),
        );
        until_spell_gone(&mut state, employment);
        assert_eq!(state.objects.get(source).controller, actor.opponent());
        assert_eq!(state.players[actor.index()].life, 20);
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            finish(current);
            assert_eq!(current.players[actor.index()].life, 25);
            assert_eq!(current.players[actor.opponent().index()].life, 15);
            event::propose_and_commit(current, ProposedEvent::life_loss(actor, 2));
            finish(current);
            assert_eq!(current.players[actor.index()].life, 23);
            assert_eq!(current.players[actor.opponent().index()].life, 17);
            assert_eq!(captures(current), 2);
        }
        assert_eq!(state, replay);
    }
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn actual_witness_response_does_not_cancel_published_gain_but_suppresses_later_losses() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        put(&mut state, actor, "High Fae Trickster", Zone::Battlefield);
        let source = conqueror(&mut state, actor);
        event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 2));
        next(&mut state);
        let witness = cast(
            &mut state,
            actor,
            "Witness Protection",
            Some(Target::Object(source)),
        );
        until_spell_gone(&mut state, witness);
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            finish(current);
            assert_eq!(current.players[actor.index()].life, 22);
            event::propose_and_commit(current, ProposedEvent::life_loss(actor.opponent(), 3));
            finish(current);
            assert_eq!(captures(current), 1);
            assert_eq!(current.players[actor.index()].life, 22);
        }
        assert_eq!(state, replay);
    }
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn actual_snap_and_real_flash_recast_keep_old_generation_gain_independent_of_new_controller() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        put(&mut state, actor, "High Fae Trickster", Zone::Battlefield);
        let source = conqueror(&mut state, actor);
        let original_generation = state.objects.get(source).zone_change_count;
        event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor.opponent(), 2));
        next(&mut state);
        let original_facts = mtg_kernel::conqueror_public_v49::capture(&state).unwrap();
        assert_eq!(
            original_facts.occurrences[0].source.zone_change_count,
            original_generation
        );
        let snap = cast(&mut state, actor, "Snap", Some(Target::Object(source)));
        until_spell_gone(&mut state, snap);
        assert_eq!(state.objects.get(source).zone, Zone::Hand);
        let hidden_departure = mtg_kernel::conqueror_public_v49::capture(&state).unwrap();
        assert_eq!(
            hidden_departure.occurrences[0].source,
            original_facts.occurrences[0].source
        );
        assert_eq!(
            hidden_departure.occurrences[0].current_original_source,
            None
        );

        state.players[actor.index()].mana_pool[ManaColor::B.pool_index()] = 2;
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        priority(&mut state, actor);
        engine::step(&mut state, Action::CastSpell(source)).unwrap();
        next(&mut state);
        for _ in 0..20 {
            if state.objects.get(source).zone == Zone::Battlefield {
                break;
            }
            engine::step(&mut state, Action::Pass).unwrap();
            next(&mut state);
        }
        assert_eq!(state.objects.get(source).zone, Zone::Battlefield);
        assert!(state.objects.get(source).zone_change_count > original_generation);
        // Exact physical control primitive: no new printing or generated spell is substituted.
        state.players[actor.index()]
            .battlefield
            .retain(|id| *id != source);
        state.players[actor.opponent().index()]
            .battlefield
            .push(source);
        state.objects.get_mut(source).controller = actor.opponent();
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            event::propose_and_commit(current, ProposedEvent::life_loss(actor, 3));
            finish(current);
            assert_eq!(current.players[actor.index()].life, 19);
            assert_eq!(current.players[actor.opponent().index()].life, 21);
            assert_eq!(captures(current), 2);
            let public = mtg_kernel::conqueror_public_v49::capture(current).unwrap();
            assert_eq!(
                public.occurrences[0].source,
                original_facts.occurrences[0].source
            );
            assert_eq!(public.occurrences[0].current_original_source, None);
            assert_ne!(
                public.occurrences[0].source.zone_change_count,
                public.occurrences[1].source.zone_change_count
            );
            assert_eq!(public.occurrences[0].original_controller, actor.into());
            assert_eq!(
                public.occurrences[1].original_controller,
                actor.opponent().into()
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
fn actual_source_lethal_combat_batch_observes_unblocked_loss_before_sba_and_gains_after_death() {
    let turn_key = |state: &GameState| {
        state
            .extra_turn_v1
            .as_ref()
            .map_or(state.turn, |turn| turn.occurrence)
    };
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = conqueror(&mut state, actor);
        let elf = put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
        let dragon = put(
            &mut state,
            actor.opponent(),
            "Shivan Dragon",
            Zone::Battlefield,
        );
        let original_turn = turn_key(&state);
        for _ in 0..160 {
            let decision = next(&mut state);
            if turn_key(&state) != original_turn
                && state.active_player == actor
                && state.step == Step::Main1
            {
                break;
            }
            match decision {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                Decision::DeclareAttackers { .. } => {
                    engine::step(&mut state, Action::DeclareAttackers(Vec::new())).unwrap()
                }
                Decision::DeclareBlockers { .. } => {
                    engine::step(&mut state, Action::DeclareBlockers(Vec::new())).unwrap()
                }
                other => panic!("actual next-turn traversal {other:?}"),
            }
        }
        assert_ne!(turn_key(&state), original_turn);
        assert_eq!(state.active_player, actor);
        assert!(!state.objects.get(source).summoning_sick);
        for _ in 0..20 {
            if matches!(next(&mut state), Decision::DeclareAttackers { .. }) {
                break;
            }
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert!(matches!(
            next(&mut state),
            Decision::DeclareAttackers { .. }
        ));
        engine::step(&mut state, Action::DeclareAttackers(vec![source, elf])).unwrap();
        for _ in 0..10 {
            match next(&mut state) {
                Decision::DeclareBlockers { .. } => {
                    engine::step(&mut state, Action::DeclareBlockers(vec![(dragon, source)]))
                        .unwrap();
                    break;
                }
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                other => panic!("real blockers {other:?}"),
            }
        }
        for _ in 0..20 {
            if state.objects.get(source).zone == Zone::Graveyard {
                break;
            }
            match next(&mut state) {
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                other => panic!("actual combat damage {other:?}"),
            }
        }
        assert_eq!(state.objects.get(source).zone, Zone::Graveyard);
        assert_eq!(state.objects.get(dragon).zone, Zone::Graveyard);
        assert_eq!(captures(&state), 1);
        let mut replay = restore(&state);
        for current in [&mut state, &mut replay] {
            finish(current);
            assert_eq!(current.players[actor.index()].life, 21);
            assert_eq!(current.players[actor.opponent().index()].life, 19);
        }
        assert_eq!(state, replay);
    }
}

#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn real_bolt_crystal_and_typed_finite_shield_order_prevent_creature_damage_without_manufacturing_life_loss(
) {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor);
        let source = conqueror(&mut state, actor);
        let barricade = put(&mut state, actor, "Crystal Barricade", Zone::Battlefield);
        // Existing typed finite-shield primitive; this does not assert a Forest printed shield ability.
        let shield_source = put(&mut state, actor, "Forest", Zone::Battlefield);
        state
            .engine
            .active_replacements
            .push(event::ActiveReplacement {
                id: 901,
                source: shield_source,
                kind: event::ReplacementEffectKind::PreventNextDamage {
                    target: Target::Object(source),
                    remaining: 1,
                },
            });
        let bolt = cast(
            &mut state,
            actor.opponent(),
            "Lightning Bolt",
            Some(Target::Object(source)),
        );
        let mut offered = false;
        for _ in 0..20 {
            match next(&mut state) {
                Decision::ChooseEffectOption {
                    player,
                    option_count: 2,
                    ..
                } => {
                    assert_eq!(player, actor);
                    offered = true;
                    break;
                }
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                other => panic!("actual mixed prevention prompt {other:?}"),
            }
        }
        assert!(offered);
        let saved = restore(&state);
        let mut remaining = Vec::new();
        for choice in [0, 1] {
            let mut replay = restore(&saved);
            engine::step(&mut replay, Action::ChooseEffectOption(choice)).unwrap();
            // Choosing the finite shield leaves the actual Barricade replacement
            // as a mandatory one-option continuation for the same damage event.
            match next(&mut replay) {
                Decision::ChooseEffectOption {
                    player,
                    source: pending_source,
                    option_count,
                } => {
                    assert_eq!(player, actor);
                    assert_eq!(pending_source, bolt);
                    assert_eq!(option_count, 1);
                    engine::step(&mut replay, Action::ChooseEffectOption(0)).unwrap();
                }
                Decision::CastSpellOrPass { .. } => {}
                other => panic!("unexpected prevention continuation {other:?}"),
            }
            finish(&mut replay);
            assert_eq!(replay.objects.get(bolt).zone, Zone::Graveyard);
            assert_eq!(replay.objects.get(source).damage, 0);
            assert_eq!(captures(&replay), 0);
            assert_eq!(replay.players[actor.index()].life, 20);
            assert_eq!(replay.objects.get(barricade).zone, Zone::Battlefield);
            remaining.push(
                replay
                    .engine
                    .active_replacements
                    .iter()
                    .filter_map(|replacement| match &replacement.kind {
                        event::ReplacementEffectKind::PreventNextDamage { remaining, .. } => {
                            Some(*remaining)
                        }
                        _ => None,
                    })
                    .sum::<i32>(),
            );
            event::propose_and_commit(&mut replay, ProposedEvent::life_loss(actor.opponent(), 4));
            finish(&mut replay);
            assert_eq!(replay.players[actor.index()].life, 24);
            assert_eq!(captures(&replay), 1);
        }
        remaining.sort();
        assert_eq!(remaining, [0, 1]);
    }
}
#[test]
#[cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
fn real_sire_ward_life_payment_triggers_original_opposing_conqueror_but_decline_does_not() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for pay in [false, true] {
            let mut state = ready(actor);
            put(
                &mut state,
                actor.opponent(),
                "High Fae Trickster",
                Zone::Battlefield,
            );
            conqueror(&mut state, actor.opponent());
            let sire = put(
                &mut state,
                actor.opponent(),
                "Sire of Seven Deaths",
                Zone::Battlefield,
            );
            let bolt = cast(
                &mut state,
                actor,
                "Lightning Bolt",
                Some(Target::Object(sire)),
            );
            let mut offered = false;
            for _ in 0..30 {
                match next(&mut state) {
                    Decision::ChooseEffectBoolean { player, .. } => {
                        assert_eq!(player, actor);
                        offered = true;
                        break;
                    }
                    Decision::CastSpellOrPass { .. } => {
                        engine::step(&mut state, Action::Pass).unwrap()
                    }
                    other => panic!("real Ward cost {other:?}"),
                }
            }
            assert!(offered);
            let mut replay = restore(&state);
            for current in [&mut state, &mut replay] {
                engine::step(current, Action::ChooseEffectBoolean(pay)).unwrap();
                finish(current);
                assert_eq!(current.objects.get(bolt).zone, Zone::Graveyard);
                assert_eq!(
                    current.players[actor.index()].life,
                    if pay { 13 } else { 20 }
                );
                assert_eq!(
                    current.players[actor.opponent().index()].life,
                    if pay { 27 } else { 20 }
                );
                assert_eq!(captures(current), usize::from(pay));
                assert_eq!(current.objects.get(sire).damage, if pay { 3 } else { 0 });
            }
            assert_eq!(state, replay);
        }
    }
}
