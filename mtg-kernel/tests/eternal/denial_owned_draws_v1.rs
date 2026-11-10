//! Actual paid Arcane Denial owns both next-upkeep delayed draw instructions.
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
    target: Option<Target>,
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
            Decision::ChooseCastMode { .. } => {
                act(state, Action::ChooseCastMode(engine::CastMode::Normal))
            }
            Decision::ChooseTargets { legal_targets, .. } => {
                let target = target.expect("printed target");
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

fn priority(state: &mut GameState, actor: PlayerId) {
    match next(state) {
        Decision::CastSpellOrPass { player, .. } if player == actor => return,
        Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
        other => panic!("priority: {other:?}"),
    }
    assert!(matches!(next(state), Decision::CastSpellOrPass { player, .. } if player == actor));
}
fn denial(state: &mut GameState, actor: PlayerId, target: ObjectId) -> ObjectId {
    priority(state, actor);
    cast(
        state,
        actor,
        "Arcane Denial",
        Some(Target::Object(target)),
        &[ManaColor::U],
        1,
    )
}
fn bolt(state: &mut GameState, actor: PlayerId) -> ObjectId {
    priority(state, actor);
    cast(
        state,
        actor,
        "Lightning Bolt",
        Some(Target::Player(actor.opponent())),
        &[ManaColor::R],
        0,
    )
}
fn next_upkeep(state: &mut GameState, previous_active: PlayerId) {
    for _ in 0..160 {
        let decision = next(state);
        if state.active_player == previous_active.opponent() && state.step == Step::Upkeep {
            return;
        }
        match decision {
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::DeclareAttackers { .. } => act(state, Action::DeclareAttackers(vec![])),
            Decision::DeclareBlockers { .. } => act(state, Action::DeclareBlockers(vec![])),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            other => panic!("next upkeep: {other:?}"),
        }
    }
    panic!("next upkeep absent");
}
fn order_pair(state: &mut GameState, controller: PlayerId, optional_first: bool) {
    let Decision::OrderTriggers { player, pending } = next(state) else {
        panic!("two owned delayed triggers");
    };
    assert_eq!(player, controller);
    assert_eq!(pending.len(), 2);
    assert!(pending.iter().all(|row| row.controller == controller));
    let optional = pending
        .iter()
        .position(|row| matches!(row.effect, mtg_kernel::effect::EffectOp::Choice { .. }))
        .unwrap();
    let mandatory = 1 - optional;
    // Native order is bottom to top; the last published row resolves first.
    act(
        state,
        Action::OrderTriggers(if optional_first {
            vec![mandatory, optional]
        } else {
            vec![optional, mandatory]
        }),
    );
}
fn optional(state: &mut GameState, actor: PlayerId, count: u16) {
    assert!(
        matches!(settle(state), Some(Decision::ChooseEffectOption { player, option_count: 3, .. }) if player == actor)
    );
    act(state, Action::ChooseEffectOption(count));
}
fn v5(state: &GameState, actor: PlayerId) {
    mtg_kernel::rl::observe_policy_v5(
        state,
        &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
        actor,
        0,
        0,
        0,
        1,
    )
    .unwrap();
}
#[test]
fn paid_denial_both_seats_same_controller_all_counts_orders_restore_and_v5() {
    for actor in [P0, P1] {
        for same_controller in [false, true] {
            for count in 0..=2 {
                for optional_first in [false, true] {
                    let target_controller = if same_controller {
                        actor
                    } else {
                        actor.opponent()
                    };
                    let mut state = plains();
                    state.active_player = target_controller;
                    state.priority_player = target_controller;
                    let target = bolt(&mut state, target_controller);
                    let source = denial(&mut state, actor, target);
                    assert_eq!(receipt_count(&state, "Paid"), 1);
                    settled(&mut state);
                    assert_eq!(zone(&state, source), Zone::Graveyard);
                    assert_eq!(zone(&state, target), Zone::Graveyard);
                    assert!(state
                        .players
                        .iter()
                        .all(|seat| seat.life == 20 && seat.hand.is_empty()));
                    assert_eq!(receipt_count(&state, "Scheduled"), 1);
                    v5(&state, state.priority_player);
                    let mut replay = restored(&state);
                    for branch in [&mut state, &mut replay] {
                        next_upkeep(branch, target_controller);
                        v5(branch, actor);
                        order_pair(branch, actor, optional_first);
                        assert_eq!(receipt_count(branch, "Queued"), 2);
                        assert_eq!(receipt_count(branch, "Published"), 2);
                        assert!(
                            matches!(settle(branch), Some(Decision::ChooseEffectOption { player, option_count: 3, .. }) if player == target_controller)
                        );
                        v5(branch, target_controller);
                        *branch = restored(branch);
                        act(branch, Action::ChooseEffectOption(count));
                        settled(branch);
                        assert_eq!(
                            branch.players[actor.index()].hand.len(),
                            1 + if same_controller { count as usize } else { 0 }
                        );
                        if !same_controller {
                            assert_eq!(
                                branch.players[target_controller.index()].hand.len(),
                                count as usize
                            );
                        }
                        assert_eq!(receipt_count(branch, "Completed"), 1 + count as usize);
                        assert_eq!(receipt_count(branch, "DrawStarted"), 2);
                        assert_eq!(branch.step, Step::Upkeep, "no normal turn draw yet");
                        v5(branch, branch.priority_player);
                    }
                    assert_eq!(state.state_hash(), replay.state_hash());
                }
            }
        }
    }
}
#[test]
fn paid_denial_legal_uncounterable_schedules_but_departed_illegal_target_does_not() {
    for actor in [P0, P1] {
        for uncounterable in [false, true] {
            let other = actor.opponent();
            let mut state = plains();
            state.active_player = other;
            state.priority_player = other;
            let target = if uncounterable {
                cast(
                    &mut state,
                    other,
                    "Supreme Verdict",
                    None,
                    &[ManaColor::W, ManaColor::W, ManaColor::U],
                    1,
                )
            } else {
                bolt(&mut state, other)
            };
            denial(&mut state, actor, target);
            if !uncounterable {
                priority(&mut state, actor);
                cast(
                    &mut state,
                    actor,
                    "Counterspell",
                    Some(Target::Object(target)),
                    &[ManaColor::U, ManaColor::U],
                    0,
                );
            }
            settled(&mut state);
            assert_eq!(
                receipt_count(&state, "Scheduled"),
                usize::from(uncounterable)
            );
            assert_eq!(zone(&state, target), Zone::Graveyard);
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                next_upkeep(branch, other);
                if uncounterable {
                    order_pair(branch, actor, true);
                    optional(branch, other, 2);
                    settled(branch);
                }
                assert_eq!(
                    branch.players[actor.index()].hand.len(),
                    usize::from(uncounterable)
                );
                assert_eq!(
                    branch.players[other.index()].hand.len(),
                    if uncounterable { 2 } else { 0 }
                );
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}
#[test]
fn paid_denial_departed_reentered_source_retains_original_pending_and_published_identity() {
    for actor in [P0, P1] {
        let other = actor.opponent();
        let mut state = plains();
        state.active_player = other;
        state.priority_player = other;
        let target = bolt(&mut state, other);
        let source = denial(&mut state, actor, target);
        let original = state.stack.last().unwrap().v4.source_contract.unwrap();
        settled(&mut state);
        // Native zone events after actual spell resolution test a new source
        // incarnation. This does not claim a shipped blink/reanimation spell.
        for destination in [Zone::Hand, Zone::Library] {
            mtg_kernel::event::propose_and_commit(
                &mut state,
                mtg_kernel::event::ProposedEvent::zone_change(source, destination),
            );
        }
        assert_ne!(
            state.objects.get(source).zone_change_count,
            original.zone_change_count
        );
        next_upkeep(&mut state, other);
        let mut replay = restored(&state);
        for branch in [&mut state, &mut replay] {
            let observation = mtg_kernel::rl::observe_policy_v5(
                branch,
                &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
                actor,
                0,
                0,
                0,
                1,
            )
            .unwrap();
            let pending = &observation
                .projection
                .surface
                .engine_context
                .pending_triggers;
            assert_eq!(pending.len(), 2);
            assert!(pending.iter().all(|row| row
                .source
                .as_ref()
                .is_some_and(|source| source.zone == Zone::Stack
                    && source.zone_change_count == original.zone_change_count)));
            order_pair(branch, actor, true);
            assert!(branch
                .stack
                .iter()
                .all(|row| row.v4.source_contract == Some(original)));
            optional(branch, other, 2);
            settled(branch);
            assert_eq!(receipt_count(branch, "Completed"), 3);
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}
#[test]
fn paid_denial_riddler_adjusts_each_instruction_once_with_dredge_and_hull() {
    for actor in [P0, P1] {
        let other = actor.opponent();
        let mut state = plains();
        state.active_player = actor;
        state.priority_player = actor;
        put(&mut state, actor, "Quantum Riddler", Zone::Battlefield);
        put(&mut state, other, "Hullbreacher", Zone::Battlefield);
        let thug = put(&mut state, actor, "Golgari Thug", Zone::Graveyard);
        let target = bolt(&mut state, actor);
        denial(&mut state, actor, target);
        settled(&mut state);
        next_upkeep(&mut state, actor);
        order_pair(&mut state, actor, true);
        optional(&mut state, actor, 2);
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { player, legal_targets, .. }) if player == actor && legal_targets == [Target::Object(thug)])
        );
        let mut replay = restored(&state);
        for branch in [&mut state, &mut replay] {
            act(branch, Action::ChooseEffectTarget(Target::Object(thug)));
            settled(branch);
            assert_eq!(branch.players[actor.index()].hand, vec![thug]);
            assert_eq!(named(branch, other, "Treasure Token").len(), 4);
            assert_eq!(receipt_count(branch, "StartedWithRiddlerV1"), 2);
            assert_eq!(receipt_count(branch, "Completed"), 5);
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}
#[test]
fn paid_denial_jace_hull_no_dredge_terminal_stops_both_instruction_orders() {
    for actor in [P0, P1] {
        for optional_first in [false, true] {
            let mut state = game(&[]);
            // The other player will begin the next turn, before their draw step.
            for _ in 0..4 {
                put(&mut state, actor.opponent(), "Plains", Zone::Library);
            }
            let jace = cast_jace(&mut state, actor);
            put(
                &mut state,
                actor.opponent(),
                "Hullbreacher",
                Zone::Battlefield,
            );
            put(&mut state, actor, "Quantum Riddler", Zone::Battlefield);
            let target = bolt(&mut state, actor);
            denial(&mut state, actor, target);
            settled(&mut state);
            next_upkeep(&mut state, actor);
            order_pair(&mut state, actor, optional_first);
            if optional_first {
                optional(&mut state, actor, 2);
            }
            assert!(
                matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { player, legal_targets, .. }) if player == actor && legal_targets == [Target::Object(jace)])
            );
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                assert!(mtg_kernel::rl::observe_policy_v5(
                    branch,
                    &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
                    actor,
                    0,
                    0,
                    0,
                    1
                )
                .unwrap_err()
                .to_string()
                .contains("Denial"));
                act(branch, Action::ChooseEffectTarget(Target::Object(jace)));
                assert!(
                    matches!(next(branch), Decision::GameOver { winner: Some(winner) } if winner == actor)
                );
                assert_eq!(receipt_count(branch, "Completed"), 1);
                assert_eq!(receipt_count(branch, "DrawStarted"), 1);
                assert_eq!(
                    branch.stack.len(),
                    1,
                    "the second delayed trigger never resolves"
                );
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}
#[test]
fn paid_denial_restore_refuses_source_controller_schedule_phase_count_and_raw_borrowing() {
    use mtg_kernel::effect;
    for actor in [P0, P1] {
        let mut state = plains();
        state.active_player = actor;
        state.priority_player = actor;
        put(&mut state, actor, "Golgari Thug", Zone::Graveyard);
        let target = bolt(&mut state, actor);
        denial(&mut state, actor, target);
        settled(&mut state);
        next_upkeep(&mut state, actor);
        order_pair(&mut state, actor, true);
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectOption { .. })
        ));
        restored(&state);
        let original = state.engine.pending_effect.clone().unwrap();
        let root = original.resolving_item.inline_effect.clone().unwrap();
        let mut raw = state.clone();
        raw.engine.pending_effect = None;
        effect::execute(&root, &original.ctx, &mut raw);
        assert!(raw.engine.halted.is_some());
        let mut borrowed = state.clone();
        borrowed.engine.pending_effect = None;
        borrowed.stack.push(original.resolving_item.clone());
        assert!(effect::begin_resumable_resolution(
            &root,
            &original.ctx,
            original.resolving_item.clone(),
            &mut borrowed
        )
        .is_err());
        for mutation in 0..5 {
            let mut forged = state.clone();
            let pending = forged.engine.pending_effect.as_mut().unwrap();
            match mutation {
                0 => pending.ctx.controller = actor.opponent(),
                1 => pending.ctx.stack_item_id = None,
                2 => {
                    pending
                        .resolving_item
                        .v4
                        .source_contract
                        .as_mut()
                        .unwrap()
                        .zone_change_count += 1
                }
                3 => pending.resolving_item.is_copy = true,
                _ => pending.choice = None,
            }
            assert!(
                effect::validate_pending_effect_choice(&forged).is_err(),
                "mutation {mutation}"
            );
        }
        for (variant, field) in [
            ("Scheduled", "paid_at"),
            ("Scheduled", "target_at"),
            ("Queued", "upkeep_at"),
            ("Published", "item"),
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
        for variant in ["Scheduled", "Queued"] {
            let mut json = serde_json::to_value(&state).unwrap();
            assert!(alter(
                &mut json["engine"]["event_history"],
                variant,
                "turn",
                serde_json::json!({ "turn": 999999, "active": actor })
            ));
            let forged: GameState = serde_json::from_value(json).unwrap();
            assert!(
                effect::validate_pending_effect_choice(&forged).is_err(),
                "{variant}.turn"
            );
        }
        let mut json = serde_json::to_value(&state).unwrap();
        assert!(alter(
            &mut json["engine"]["event_history"],
            "Scheduled",
            "drawer",
            serde_json::to_value(actor.opponent()).unwrap()
        ));
        let forged: GameState = serde_json::from_value(json).unwrap();
        assert!(effect::validate_pending_effect_choice(&forged).is_err());
        act(&mut state, Action::ChooseEffectOption(2));
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectTargets { .. })
        ));
        for (variant, field, value) in [
            ("Answered", "count", 1),
            ("DrawStarted", "count", 1),
            ("Started", "total", 1),
        ] {
            let mut json = serde_json::to_value(&state).unwrap();
            assert!(alter(
                &mut json["engine"]["event_history"],
                variant,
                field,
                value.into()
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
        let mut replay = restored(&state);
        for branch in [&mut state, &mut replay] {
            for _ in 0..3 {
                act(branch, Action::FinishEffectSelection);
                if receipt_count(branch, "Completed") < 3 {
                    settle(branch);
                }
            }
            settled(branch);
            assert_eq!(receipt_count(branch, "Completed"), 3);
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}
#[test]
fn native_chandra_denial_copy_keeps_ordinary_predecessor_route() {
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
        let target = bolt(&mut state, actor.opponent());
        denial(&mut state, actor, target);
        // Copy may choose the original target again. It resolves first and
        // counters it, so only the copy's predecessor schedule is produced.
        for _ in 0..12 {
            match settle(&mut state) {
                None => break,
                Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
                    assert!(legal_targets.contains(&Target::Object(target)));
                    act(
                        &mut state,
                        Action::ChooseEffectTarget(Target::Object(target)),
                    );
                }
                other => panic!("copy: {other:?}"),
            }
        }
        assert_eq!(receipt_count(&state, "Paid"), 1);
        assert_eq!(receipt_count(&state, "Scheduled"), 0);
        let mut replay = restored(&state);
        for branch in [&mut state, &mut replay] {
            next_upkeep(branch, actor);
            optional(branch, actor.opponent(), 2);
            settled(branch);
            assert_eq!(branch.players[actor.index()].hand.len(), 1);
            assert_eq!(branch.players[actor.opponent().index()].hand.len(), 2);
            assert_eq!(receipt_count(branch, "DrawStarted"), 0);
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}
#[test]
fn paid_denial_commandeer_captures_both_controllers_at_resolution_and_native_retarget() {
    for actor in [P0, P1] {
        for steal_denial in [false, true] {
            let other = actor.opponent();
            let mut state = plains();
            state.active_player = actor;
            state.priority_player = actor;
            let target = bolt(&mut state, actor);
            let source = denial(&mut state, actor, target);
            priority(&mut state, other);
            cast(
                &mut state,
                other,
                "Commandeer",
                Some(Target::Object(if steal_denial { source } else { target })),
                &[ManaColor::U, ManaColor::U],
                5,
            );
            let retarget = if steal_denial {
                Target::Object(target)
            } else {
                Target::Player(actor)
            };
            assert!(
                matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { legal_targets, .. }) if legal_targets.contains(&retarget))
            );
            act(&mut state, Action::ChooseEffectTarget(retarget));
            settled(&mut state);
            assert_eq!(receipt_count(&state, "Scheduled"), 1);
            assert_eq!(receipt_count(&state, "Rebound"), usize::from(steal_denial));
            let controller = if steal_denial { other } else { actor };
            let drawer = if steal_denial { actor } else { other };
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                next_upkeep(branch, actor);
                order_pair(branch, controller, true);
                optional(branch, drawer, 2);
                settled(branch);
                assert_eq!(branch.players[controller.index()].hand.len(), 1);
                assert_eq!(branch.players[drawer.index()].hand.len(), 2);
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}
#[test]
fn paid_denial_two_controllers_queue_apnap_at_the_same_next_upkeep() {
    for active in [P0, P1] {
        let other = active.opponent();
        let mut state = plains();
        state.active_player = active;
        state.priority_player = active;
        let first = bolt(&mut state, active);
        denial(&mut state, other, first);
        settled(&mut state);
        let second = bolt(&mut state, other);
        denial(&mut state, active, second);
        settled(&mut state);
        assert_eq!(receipt_count(&state, "Scheduled"), 2);
        next_upkeep(&mut state, active);
        let mut replay = restored(&state);
        for branch in [&mut state, &mut replay] {
            order_pair(branch, other, true); // Active player's pair is placed first.
            order_pair(branch, active, true); // Nonactive player's pair resolves first.
            optional(branch, other, 2);
            optional(branch, active, 2);
            settled(branch);
            assert!(branch.players.iter().all(|seat| seat.hand.len() == 3));
            assert_eq!(receipt_count(branch, "Queued"), 4);
            assert_eq!(receipt_count(branch, "Completed"), 6);
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}
#[test]
fn paid_denial_commandeer_retargets_without_replacing_original_payment() {
    for actor in [P0, P1] {
        let other = actor.opponent();
        let mut state = plains();
        state.active_player = actor;
        state.priority_player = actor;
        let first = bolt(&mut state, actor);
        let source = denial(&mut state, actor, first);
        let second = bolt(&mut state, other);
        cast(
            &mut state,
            other,
            "Commandeer",
            Some(Target::Object(source)),
            &[ManaColor::U, ManaColor::U],
            5,
        );
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { legal_targets, .. }) if legal_targets.contains(&Target::Object(second)))
        );
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(second)),
        );
        settled(&mut state);
        assert_eq!(receipt_count(&state, "Paid"), 1);
        assert_eq!(receipt_count(&state, "Rebound"), 2);
        assert_eq!(receipt_count(&state, "Scheduled"), 1);
        let mut replay = restored(&state);
        for branch in [&mut state, &mut replay] {
            next_upkeep(branch, actor);
            order_pair(branch, other, true);
            optional(branch, other, 2);
            settled(branch);
            assert_eq!(branch.players[other.index()].hand.len(), 3);
            assert!(branch.players[actor.index()].hand.is_empty());
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}
#[test]
fn paid_denial_parent_refuses_raw_and_live_borrowed_native_stack_identity() {
    use mtg_kernel::effect::{self, ExecCtx};
    for actor in [P0, P1] {
        let mut state = plains();
        state.active_player = actor;
        state.priority_player = actor;
        let target = bolt(&mut state, actor);
        denial(&mut state, actor, target);
        let row = state.stack.last().unwrap().clone();
        let mut ctx = ExecCtx::no_targets(row.source, row.controller);
        ctx.stack_item_id = Some(row.v4.stack_item_id);
        ctx.targets.clone_from(&row.targets);
        ctx.target_contracts.clone_from(&row.v4.target_contracts);
        ctx.paid_cost_refs.clone_from(&row.v4.paid_cost_refs);
        ctx.optional_additional_cost_paid = row.v4.optional_additional_cost_paid;
        let root = mtg_kernel::eternal_e9b_v1::arcane_denial();
        let mut borrowed = state.clone();
        assert!(
            effect::begin_resumable_resolution(&root, &ctx, row.clone(), &mut borrowed).is_err()
        );
        let mut raw = state.clone();
        effect::execute(&root, &ctx, &mut raw);
        assert!(raw.engine.halted.is_some());
        assert_eq!(receipt_count(&raw, "Scheduled"), 0);
        settled(&mut state);
        assert_eq!(receipt_count(&state, "Scheduled"), 1);
        let mut replay = state.clone();
        assert!(effect::begin_resumable_resolution(&root, &ctx, row, &mut replay).is_err());
        assert_eq!(receipt_count(&replay, "Scheduled"), 1);
    }
}
