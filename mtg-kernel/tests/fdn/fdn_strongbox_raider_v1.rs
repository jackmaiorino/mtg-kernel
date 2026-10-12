//! Unregistered future printed Strongbox acceptance. No fabricated producing stack rows.
#![cfg(all(
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
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
    let source = put(state, actor, "Strongbox Raider", Zone::Hand);
    state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 2;
    state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 2;
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

#[test]
#[ignore = "Strongbox definition remains unregistered; root owns qualification"]
fn genuine_both_seat_raid_exile_exact_one_and_restored_answer_permissions() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, 5);
        raid(&mut state, actor, true);
        let source = cast(&mut state, actor);
        let decision = settle(&mut state).expect("actual Strongbox choice");
        let Decision::ChooseEffectTargets {
            legal_targets,
            min_targets,
            max_targets,
            ..
        } = decision
        else {
            panic!("{decision:?}")
        };
        assert_eq!((min_targets, max_targets, legal_targets.len()), (1, 1, 2));
        let Target::Object(chosen) = legal_targets[1] else {
            panic!("public exile object")
        };
        let mut restored = replay(&state);
        for current in [&mut state, &mut restored] {
            assert_eq!(current.objects.get(source).zone, Zone::Battlefield);
            assert_eq!(current.objects.get(chosen).zone, Zone::Exile);
            engine::step(current, Action::ChooseEffectTarget(Target::Object(chosen))).unwrap();
            settle(current);
            let selected: Vec<_> = current
                .engine
                .exile_play_permissions
                .iter()
                .filter(|p| p.holder == actor)
                .collect();
            assert_eq!(selected.len(), 1);
            assert_eq!(selected[0].object, chosen);
            assert_eq!(
                selected[0].zone_change_generation,
                current.objects.get(chosen).zone_change_count
            );
        }
        assert_eq!(
            state.diagnostic_state_hash(),
            restored.diagnostic_state_hash()
        );
    }
}
#[test]
#[ignore = "Strongbox definition remains unregistered; root owns qualification"]
fn genuine_empty_declaration_never_exiles_or_grants() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready(actor, 5);
        raid(&mut state, actor, false);
        let library = state.players[actor.index()].library.clone();
        cast(&mut state, actor);
        assert!(settle(&mut state).is_none());
        assert_eq!(state.players[actor.index()].library, library);
        assert!(state.engine.exile_play_permissions.is_empty());
    }
}

fn granted(actor: PlayerId) -> GameState {
    let mut state = ready(actor, 5);
    raid(&mut state, actor, true);
    cast(&mut state, actor);
    let decision = settle(&mut state).expect("actual selection");
    let Decision::ChooseEffectTargets { legal_targets, .. } = decision else {
        panic!("{decision:?}")
    };
    engine::step(&mut state, Action::ChooseEffectTarget(legal_targets[0])).unwrap();
    settle(&mut state);
    state
}
fn natural_pass(state: &mut GameState) {
    match next(state) {
        Decision::CastSpellOrPass { .. } => engine::step(state, Action::Pass).unwrap(),
        Decision::DeclareAttackers { .. } => {
            engine::step(state, Action::DeclareAttackers(vec![])).unwrap()
        }
        Decision::DeclareBlockers { .. } => {
            engine::step(state, Action::DeclareBlockers(vec![])).unwrap()
        }
        other => panic!("unexpected natural boundary {other:?}"),
    }
}
#[test]
#[ignore = "Strongbox remains unregistered; root owns actual qualification"]
fn actual_holder_next_turn_cleanup_permissions_survive_opponent_and_restore_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = granted(actor);
        let round = state.turn;
        let mut saw_opponent = false;
        let mut saw_next_holder = false;
        for _ in 0..160 {
            next(&mut state);
            if state.active_player == actor.opponent() && state.turn == round {
                saw_opponent = true;
                assert_eq!(state.engine.exile_play_permissions.len(), 1);
            }
            if state.active_player == actor && state.turn == round + 1 && state.step == Step::Main1
            {
                saw_next_holder = true;
                assert_eq!(state.engine.exile_play_permissions.len(), 1);
                assert!(matches!(
                    state.engine.exile_play_permissions[0].expiry,
                    engine::PlayPermissionExpiry::UntilHoldersNextTurn {
                        holder_turn_started: true
                    }
                ));
                let mut restored = replay(&state);
                natural_pass(&mut state);
                natural_pass(&mut restored);
                assert_eq!(
                    serde_json::to_vec(&state).unwrap(),
                    serde_json::to_vec(&restored).unwrap()
                );
            }
            if state.engine.event_history.iter().any(|event| {
                matches!(
                    event,
                    mtg_kernel::event::CommittedEvent::StrongboxPermissionExpiredV1 { .. }
                )
            }) {
                assert!(saw_opponent && saw_next_holder);
                assert!(state.engine.exile_play_permissions.is_empty());
                break;
            }
            natural_pass(&mut state);
        }
        assert!(state.engine.event_history.iter().any(|event| matches!(
            event,
            mtg_kernel::event::CommittedEvent::StrongboxPermissionExpiredV1 { .. }
        )));
    }
}
#[test]
#[ignore = "Strongbox remains unregistered; root owns actual qualification"]
fn erased_permission_and_coherent_early_expiry_refuse_before_action_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let state = granted(actor);
        let installation = state
            .engine
            .event_history
            .iter()
            .find_map(|event| match event {
                mtg_kernel::event::CommittedEvent::StrongboxPermissionGrantedV1 {
                    installation,
                } => Some(installation.clone()),
                _ => None,
            })
            .unwrap();
        for mode in [0, 1] {
            let mut bad = replay(&state);
            bad.engine.exile_play_permissions.clear();
            if mode == 1 {
                bad.engine.event_history.push(
                    mtg_kernel::event::CommittedEvent::StrongboxPermissionExpiredV1 {
                        installation: installation.index,
                        cleanup: installation.index,
                    },
                );
            }
            let before = serde_json::to_vec(&bad).unwrap();
            assert!(engine::step(&mut bad, Action::Pass).is_err());
            assert_eq!(before, serde_json::to_vec(&bad).unwrap());
            let mut restored = replay(&bad);
            assert!(matches!(
                engine::advance_until_decision(&mut restored),
                Decision::Halted { .. }
            ));
            restored.engine.halted = None;
            assert_eq!(before, serde_json::to_vec(&restored).unwrap());
        }
    }
}

#[test]
#[ignore = "Strongbox remains unregistered; root owns actual qualification"]
fn actual_printed_meaning_reconstructs_full_root_and_refuses_each_edited_fact() {
    use mtg_kernel::rules_meaning_strongbox_v1 as meaning;
    let selector = meaning::live_selector().unwrap();
    let id = card_id_by_name("Strongbox Raider").unwrap();
    let record = meaning::meaning(&selector, id, 0).unwrap();
    meaning::validate(&selector, &record).unwrap();
    assert!(meaning::meaning(&selector, id, 1).is_err());
    let value = serde_json::to_value(&record).unwrap();
    for (name, fact) in value.as_object().unwrap() {
        if let Some(actual) = fact.as_bool() {
            let mut forged = value.clone();
            forged[name] = serde_json::Value::Bool(!actual);
            let artifact = serde_json::from_value(forged).unwrap();
            assert!(meaning::validate(&selector, &artifact).is_err(), "{name}");
        }
    }
    let mut unknown = value;
    unknown["hidden_library"] = serde_json::Value::Bool(true);
    assert!(serde_json::from_value::<meaning::MeaningV1>(unknown).is_err());
}
