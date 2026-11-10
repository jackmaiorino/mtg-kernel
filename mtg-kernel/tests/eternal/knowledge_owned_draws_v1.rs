//! Actual paid Accumulated Knowledge owns its two sequential draw instructions.
#![cfg(feature = "eternal-fixtures")]
use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;
fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

fn game(library: &[&str]) -> GameState {
    let ids: Vec<u16> = library.iter().map(|name| id(name)).collect();
    let mut state = GameState::new_from_libraries(
        &ids,
        &ids,
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x4539_4221_0001,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
}

fn plains() -> GameState {
    game(&["Plains"; 20])
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = id(name);
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: false,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    let seat = &mut state.players[player.index()];
    match zone {
        Zone::Hand => seat.hand.push(object),
        Zone::Battlefield => seat.battlefield.push(object),
        Zone::Graveyard => seat.graveyard.push(object),
        Zone::Exile => state.exile.push(object),
        Zone::Library => seat.library.insert(0, object),
        _ => panic!("helper zone"),
    }
    object
}

fn pool(state: &mut GameState, player: PlayerId, colored: &[ManaColor], colorless: u8) {
    let pool = &mut state.players[player.index()].mana_pool;
    for color in colored {
        pool[color.pool_index()] += 1;
    }
    pool[ManaColor::C.pool_index()] += colorless;
}

fn zone(state: &GameState, object: ObjectId) -> Zone {
    state.objects.get(object).zone
}

fn named(state: &GameState, player: PlayerId, name: &str) -> Vec<ObjectId> {
    [P0, P1]
        .into_iter()
        .flat_map(|seat| state.players[seat.index()].battlefield.iter().copied())
        .filter(|&object| {
            let live = state.objects.get(object);
            live.controller == player && CARD_DEFS[live.card_def as usize].name == name
        })
        .collect()
}

fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(
        !matches!(decision, Decision::Halted { .. }),
        "{decision:?}\nstack: {:#?}\npending triggers: {:#?}",
        state.stack,
        state.engine.pending_triggers
    );
    decision
}

fn act(state: &mut GameState, action: Action) {
    engine::step(state, action.clone()).unwrap_or_else(|error| panic!("{action:?}: {error}"));
}

fn settle(state: &mut GameState) -> Option<Decision> {
    for _ in 0..200 {
        match next(state) {
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. }
                if state.stack.is_empty() && state.engine.pending_triggers.is_empty() =>
            {
                return None
            }
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => return Some(other),
        }
    }
    panic!("did not settle");
}

fn settled(state: &mut GameState) {
    if let Some(other) = settle(state) {
        panic!("unexpected choice: {other:?}");
    }
}

fn cast_jace(state: &mut GameState, player: PlayerId) -> ObjectId {
    state.active_player = player;
    state.priority_player = player;
    let jace = put(state, player, "Jace, Wielder of Mysteries", Zone::Hand);
    pool(
        state,
        player,
        &[ManaColor::U, ManaColor::U, ManaColor::U],
        1,
    );
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&jace))
    );
    act(state, Action::CastSpell(jace));
    settled(state);
    jace
}

fn restored(state: &GameState) -> GameState {
    let restored: GameState = serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
    assert_eq!(state.state_hash(), restored.state_hash());
    mtg_kernel::effect::validate_pending_effect_choice(&restored).unwrap();
    restored
}

fn cast(
    state: &mut GameState,
    actor: PlayerId,
    name: &str,
    target: Option<PlayerId>,
    colored: &[ManaColor],
    colorless: u8,
) -> ObjectId {
    let card = put(state, actor, name, Zone::Hand);
    pool(state, actor, colored, colorless);
    assert!(
        matches!(next(state), Decision::CastSpellOrPass { player, castable_spells, .. } if player == actor && castable_spells.contains(&card))
    );
    act(state, Action::CastSpell(card));
    for _ in 0..12 {
        match next(state) {
            Decision::ChooseTargets { legal_targets, .. } => {
                let target = Target::Player(target.expect("printed target"));
                assert!(legal_targets.contains(&target));
                act(state, Action::ChooseTarget(target));
            }
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            Decision::CastSpellOrPass { .. } => {
                assert!(state.engine.pending_cast.is_none());
                assert!(state.stack.iter().any(|row| row.source == card
                    && !row.is_copy
                    && row
                        .v4
                        .source_contract
                        .is_some_and(|source| source.spell_cast_origin.is_some())));
                assert_eq!(zone(state, card), Zone::Stack);
                return card;
            }
            other => panic!("cast {name}: {other:?}"),
        }
    }
    panic!("paid cast did not finish");
}

fn entries(value: &serde_json::Value, key: &str) -> usize {
    match value {
        serde_json::Value::Object(map) => {
            usize::from(map.contains_key(key))
                + map.values().map(|v| entries(v, key)).sum::<usize>()
        }
        serde_json::Value::Array(rows) => rows.iter().map(|v| entries(v, key)).sum(),
        _ => 0,
    }
}

fn receipt_count(state: &GameState, key: &str) -> usize {
    entries(
        &serde_json::to_value(&state.engine.event_history).unwrap(),
        key,
    )
}

fn alter(
    value: &mut serde_json::Value,
    variant: &str,
    field: &str,
    replacement: serde_json::Value,
) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(row) = map.get_mut(variant) {
                row[field] = replacement;
                return true;
            }
            map.values_mut()
                .any(|v| alter(v, variant, field, replacement.clone()))
        }
        serde_json::Value::Array(rows) => rows
            .iter_mut()
            .any(|v| alter(v, variant, field, replacement.clone())),
        _ => false,
    }
}

fn knowledge(state: &mut GameState, actor: PlayerId) -> ObjectId {
    cast(
        state,
        actor,
        "Accumulated Knowledge",
        None,
        &[ManaColor::U],
        1,
    )
}
#[test]
fn paid_knowledge_ordinary_both_seats_zero_and_two_graveyards_restore_v5() {
    for actor in [P0, P1] {
        for previous in [0, 2] {
            let mut state = plains();
            state.active_player = actor;
            state.priority_player = actor;
            if previous > 0 {
                put(&mut state, actor, "Accumulated Knowledge", Zone::Graveyard);
                put(
                    &mut state,
                    actor.opponent(),
                    "Accumulated Knowledge",
                    Zone::Graveyard,
                );
            }
            let source = knowledge(&mut state, actor);
            assert_eq!(receipt_count(&state, "KnowledgeV1"), 1);
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                mtg_kernel::rl::observe_policy_v5(
                    branch,
                    &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
                    actor,
                    0,
                    0,
                    0,
                    1,
                )
                .unwrap();
                settled(branch);
                assert_eq!(branch.players[actor.index()].hand.len(), 1 + previous);
                assert_eq!(zone(branch, source), Zone::Graveyard);
                assert_eq!(receipt_count(branch, "Completed"), 1 + previous);
                assert_eq!(receipt_count(branch, "Second"), 1);
                mtg_kernel::rl::observe_policy_v5(
                    branch,
                    &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
                    actor,
                    0,
                    0,
                    0,
                    1,
                )
                .unwrap();
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}
#[test]
fn paid_knowledge_first_dredge_changes_second_count_later_dredge_does_not() {
    for actor in [P0, P1] {
        let mut state = game(&[
            "Accumulated Knowledge",
            "Plains",
            "Plains",
            "Plains",
            "Accumulated Knowledge",
            "Plains",
            "Plains",
            "Plains",
            "Plains",
            "Plains",
        ]);
        state.active_player = actor;
        state.priority_player = actor;
        put(
            &mut state,
            actor.opponent(),
            "Hullbreacher",
            Zone::Battlefield,
        );
        put(
            &mut state,
            actor.opponent(),
            "Accumulated Knowledge",
            Zone::Graveyard,
        );
        let first = put(&mut state, actor, "Golgari Thug", Zone::Graveyard);
        let second = put(&mut state, actor, "Golgari Thug", Zone::Graveyard);
        knowledge(&mut state, actor);
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { legal_targets, .. }) if legal_targets.contains(&Target::Object(first)))
        );
        let mut before_first = restored(&state);
        for branch in [&mut state, &mut before_first] {
            act(branch, Action::ChooseEffectTarget(Target::Object(first)));
            assert!(
                matches!(settle(branch), Some(Decision::ChooseEffectTargets { legal_targets, .. }) if legal_targets == [Target::Object(second)])
            );
            assert_eq!(receipt_count(branch, "Completed"), 1);
            assert_eq!(receipt_count(branch, "Second"), 1);
        }
        assert_eq!(state.state_hash(), before_first.state_hash());
        let mut replay = restored(&state);
        for branch in [&mut state, &mut replay] {
            act(branch, Action::ChooseEffectTarget(Target::Object(second)));
            settled(branch);
            assert_eq!(branch.players[actor.index()].library.len(), 2);
            assert_eq!(branch.players[actor.index()].hand, vec![first, second]);
            assert_eq!(named(branch, actor.opponent(), "Treasure Token").len(), 1);
            assert_eq!(
                receipt_count(branch, "Completed"),
                3,
                "one then exactly two, even after the second dredge mills another Knowledge"
            );
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}
#[test]
fn paid_knowledge_riddler_expands_each_instruction_once_with_dredge_and_hull() {
    for actor in [P0, P1] {
        let mut state = game(&[
            "Accumulated Knowledge",
            "Plains",
            "Plains",
            "Plains",
            "Plains",
            "Plains",
        ]);
        state.active_player = actor;
        state.priority_player = actor;
        put(
            &mut state,
            actor.opponent(),
            "Hullbreacher",
            Zone::Battlefield,
        );
        put(&mut state, actor, "Quantum Riddler", Zone::Battlefield);
        put(
            &mut state,
            actor.opponent(),
            "Accumulated Knowledge",
            Zone::Graveyard,
        );
        let thug = put(&mut state, actor, "Golgari Thug", Zone::Graveyard);
        knowledge(&mut state, actor);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectTargets { .. })
        ));
        let mut replay = restored(&state);
        for branch in [&mut state, &mut replay] {
            act(branch, Action::ChooseEffectTarget(Target::Object(thug)));
            settled(branch);
            assert_eq!(branch.players[actor.index()].hand, vec![thug]);
            assert_eq!(named(branch, actor.opponent(), "Treasure Token").len(), 4);
            assert_eq!(receipt_count(branch, "StartedWithRiddlerV1"), 2);
            assert_eq!(receipt_count(branch, "Completed"), 5);
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}
#[test]
fn paid_knowledge_jace_hull_no_dredge_stops_in_either_instruction() {
    for actor in [P0, P1] {
        for win_at in [None, Some(0), Some(2)] {
            let mut state = game(&[]);
            let jace = cast_jace(&mut state, actor);
            put(
                &mut state,
                actor.opponent(),
                "Hullbreacher",
                Zone::Battlefield,
            );
            put(&mut state, actor, "Quantum Riddler", Zone::Battlefield);
            put(&mut state, actor, "Accumulated Knowledge", Zone::Graveyard);
            put(
                &mut state,
                actor.opponent(),
                "Accumulated Knowledge",
                Zone::Graveyard,
            );
            knowledge(&mut state, actor);
            assert!(matches!(
                settle(&mut state),
                Some(Decision::ChooseEffectTargets { .. })
            ));
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                for ordinal in 0..5 {
                    assert!(
                        matches!(next(branch), Decision::ChooseEffectTargets { player, legal_targets, .. } if player == actor && legal_targets == [Target::Object(jace)])
                    );
                    *branch = restored(branch);
                    if win_at == Some(ordinal) {
                        act(branch, Action::ChooseEffectTarget(Target::Object(jace)));
                        assert!(
                            matches!(next(branch), Decision::GameOver { winner: Some(winner) } if winner == actor)
                        );
                        assert_eq!(receipt_count(branch, "Completed"), ordinal + 1);
                        assert_eq!(receipt_count(branch, "Second"), usize::from(ordinal > 0));
                        break;
                    }
                    act(branch, Action::FinishEffectSelection);
                    if ordinal < 4 {
                        assert!(matches!(
                            settle(branch),
                            Some(Decision::ChooseEffectTargets { .. })
                        ));
                    }
                }
                if win_at.is_none() {
                    settled(branch);
                    assert_eq!(receipt_count(branch, "Completed"), 5);
                }
                assert_eq!(
                    named(branch, actor.opponent(), "Treasure Token").len(),
                    win_at.unwrap_or(5)
                );
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}
#[test]
fn paid_knowledge_restore_refuses_source_phase_count_and_raw_borrowing() {
    use mtg_kernel::effect;
    for actor in [P0, P1] {
        let mut state = plains();
        state.active_player = actor;
        state.priority_player = actor;
        let thug = put(&mut state, actor, "Golgari Thug", Zone::Graveyard);
        put(
            &mut state,
            actor.opponent(),
            "Accumulated Knowledge",
            Zone::Graveyard,
        );
        knowledge(&mut state, actor);
        let mut live = state.clone();
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectTargets { .. })
        ));
        let original = state.engine.pending_effect.clone().unwrap();
        assert!(mtg_kernel::rl::observe_policy_v5(
            &state,
            &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
            actor,
            0,
            0,
            0,
            1
        )
        .unwrap_err()
        .to_string()
        .contains("Knowledge"));
        let root = mtg_kernel::eternal_premodern_p1b_v1::accumulated_knowledge();
        assert!(effect::begin_resumable_resolution(
            &root,
            &original.ctx,
            original.resolving_item.clone(),
            &mut live
        )
        .is_err());
        assert_eq!(receipt_count(&live, "Started"), 0);
        let mut raw = state.clone();
        raw.engine.pending_effect = None;
        effect::execute(&root, &original.ctx, &mut raw);
        assert!(raw.engine.halted.is_some());
        for mutation in 0..5 {
            let mut forged = state.clone();
            let pending = forged.engine.pending_effect.as_mut().unwrap();
            match mutation {
                0 => pending.frames.clear(),
                1 => pending.ctx.controller = actor.opponent(),
                2 => pending.ctx.stack_item_id = None,
                3 => {
                    pending
                        .resolving_item
                        .v4
                        .source_contract
                        .as_mut()
                        .unwrap()
                        .zone_change_count += 1
                }
                _ => pending.resolving_item.is_copy = true,
            }
            assert!(
                effect::validate_pending_effect_choice(&forged).is_err(),
                "mutation {mutation}"
            );
        }
        // Decline dredge once, then restore at the second instruction's real menu.
        act(&mut state, Action::FinishEffectSelection);
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { legal_targets, .. }) if legal_targets == [Target::Object(thug)])
        );
        for (variant, field) in [
            ("Paid", "item"),
            ("Second", "first_at"),
            ("Second", "paid_at"),
            ("Second", "count"),
            ("Started", "total"),
            ("Completed", "ordinal"),
        ] {
            let mut json = serde_json::to_value(&state).unwrap();
            assert!(alter(
                &mut json["engine"]["event_history"],
                variant,
                field,
                999999.into()
            ));
            let forged: GameState = serde_json::from_value(json).unwrap();
            assert!(
                effect::validate_pending_effect_choice(&forged).is_err(),
                "{variant}.{field}"
            );
        }
        let mut removed = serde_json::to_value(&state).unwrap();
        removed["eternal_owned_draw_progress_v1"] = serde_json::Value::Null;
        let removed: GameState = serde_json::from_value(removed).unwrap();
        assert!(effect::validate_pending_effect_choice(&removed).is_err());
        let mut forged = serde_json::to_value(&state).unwrap();
        forged["eternal_owned_draw_progress_v1"]["completed"] = 1.into();
        let forged: GameState = serde_json::from_value(forged).unwrap();
        assert!(effect::validate_pending_effect_choice(&forged).is_err());
        let original = state.engine.pending_effect.clone().unwrap();
        let mut restart = state.clone();
        restart.engine.pending_effect = None;
        assert!(effect::begin_resumable_resolution(
            &root,
            &original.ctx,
            original.resolving_item.clone(),
            &mut restart
        )
        .is_err());
        act(&mut state, Action::FinishEffectSelection);
        settled(&mut state);
        assert_eq!(receipt_count(&state, "Completed"), 2);
    }
}
#[test]
fn native_chandra_knowledge_copy_keeps_ordinary_predecessor_route() {
    for actor in [P0, P1] {
        let mut state = plains();
        state.active_player = actor;
        state.priority_player = actor;
        cast(
            &mut state,
            actor,
            "Chandra, Hope's Beacon",
            None,
            &[ManaColor::R, ManaColor::R],
            4,
        );
        settled(&mut state);
        put(&mut state, actor, "Accumulated Knowledge", Zone::Graveyard);
        knowledge(&mut state, actor);
        let mut replay = restored(&state);
        for branch in [&mut state, &mut replay] {
            settled(branch);
            assert_eq!(branch.players[actor.index()].hand.len(), 4);
            assert_eq!(
                receipt_count(branch, "Paid"),
                1,
                "only the actual paid original acquires this owner"
            );
            assert_eq!(receipt_count(branch, "Completed"), 2);
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}
