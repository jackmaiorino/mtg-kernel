//! Paid Standstill cast-trigger ancestry and successful-sacrifice draw ownership.
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

/// A first main phase with P0 active and holding priority. Each library
/// holds `library` in order, top first.
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

/// Passes priority and orders triggers until the stack and pending triggers
/// are empty, returning the first other decision if one interrupts.
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

fn priority(state: &mut GameState, actor: PlayerId) {
    for _ in 0..4 {
        match next(state) {
            Decision::CastSpellOrPass { player, .. } if player == actor => return,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            other => panic!("priority: {other:?}"),
        }
    }
    panic!("priority absent");
}
fn paid_standstill(state: &mut GameState, owner: PlayerId) -> ObjectId {
    let source = cast(state, owner, "Standstill", None, &[ManaColor::U], 1);
    settled(state);
    assert_eq!(zone(state, source), Zone::Battlefield);
    source
}
fn bolt(state: &mut GameState, actor: PlayerId) -> ObjectId {
    priority(state, actor);
    cast(
        state,
        actor,
        "Lightning Bolt",
        Some(actor.opponent()),
        &[ManaColor::R],
        0,
    )
}
fn source_triggers(state: &GameState, source: ObjectId) -> usize {
    state
        .stack
        .iter()
        .filter(|item| item.source == source)
        .count()
}
fn sacrifices(state: &GameState, source: ObjectId) -> usize {
    state.engine.event_history.iter().filter(|event| matches!(event, mtg_kernel::event::CommittedEvent::Sacrificed { object, .. } if *object == source)).count()
}
#[test]
fn paid_standstill_ordinary_caster_opponent_draws_both_seats_and_controllers() {
    for owner in [P0, P1] {
        for caster in [owner, owner.opponent()] {
            let mut state = plains();
            state.active_player = owner;
            state.priority_player = owner;
            let source = paid_standstill(&mut state, owner);
            assert_eq!(
                receipt_count(&state, "StandstillV1"),
                0,
                "no draw journal before a triggering cast"
            );
            bolt(&mut state, caster);
            assert_eq!(source_triggers(&state, source), 1);
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                mtg_kernel::rl::observe_policy_v5(
                    branch,
                    &mtg_kernel::policy_surface_v5::PolicySurfaceV5::new(),
                    caster,
                    0,
                    0,
                    0,
                    1,
                )
                .unwrap();
                settled(branch);
                assert_eq!(zone(branch, source), Zone::Graveyard);
                assert_eq!(sacrifices(branch, source), 1);
                assert_eq!(branch.players[caster.opponent().index()].hand.len(), 3);
                assert!(branch.players[caster.index()].hand.is_empty());
                assert_eq!(receipt_count(branch, "Started"), 1);
                assert_eq!(receipt_count(branch, "Completed"), 3);
                assert!(mtg_kernel::rl::observe_v2(
                    branch,
                    &mtg_kernel::surface_v2::HarnessSurfaceV2::default(),
                    owner,
                    0
                )
                .is_ok());
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}
#[test]
fn paid_standstill_jace_hull_terminal_stop_and_hull_all_attempts() {
    for drawer in [P0, P1] {
        for owner in [drawer, drawer.opponent()] {
            let mut state = game(&[]);
            for _ in 0..4 {
                put(&mut state, drawer.opponent(), "Plains", Zone::Library);
            }
            let jace = cast_jace(&mut state, drawer);
            put(
                &mut state,
                drawer.opponent(),
                "Hullbreacher",
                Zone::Battlefield,
            );
            put(&mut state, drawer, "Quantum Riddler", Zone::Battlefield);
            state.active_player = owner;
            state.priority_player = owner;
            let source = paid_standstill(&mut state, owner);
            bolt(&mut state, drawer.opponent());
            let prompt = settle(&mut state).expect("Jace/Hull choice without Dredge");
            assert!(
                matches!(&prompt, Decision::ChooseEffectTargets { player, legal_targets, .. } if *player == drawer && legal_targets == &[Target::Object(jace)])
            );
            assert_eq!(sacrifices(&state, source), 1);
            assert_eq!(zone(&state, source), Zone::Graveyard);
            let mut winner = restored(&state);
            assert_eq!(next(&mut winner), prompt);
            act(
                &mut winner,
                Action::ChooseEffectTarget(Target::Object(jace)),
            );
            assert!(
                matches!(next(&mut winner), Decision::GameOver { winner: Some(player) } if player == drawer)
            );
            assert_eq!(
                receipt_count(&winner, "Completed"),
                1,
                "terminal first attempt stops the remaining three"
            );
            assert_eq!(
                winner.players[drawer.index()].life,
                20,
                "underlying Bolt never resolves"
            );
            assert!(named(&winner, drawer.opponent(), "Treasure Token").is_empty());
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                for ordinal in 0..4 {
                    act(branch, Action::FinishEffectSelection);
                    if ordinal < 3 {
                        assert!(matches!(
                            settle(branch),
                            Some(Decision::ChooseEffectTargets { .. })
                        ));
                        *branch = restored(branch);
                        assert_eq!(sacrifices(branch, source), 1);
                    }
                }
                settled(branch);
                assert_eq!(named(branch, drawer.opponent(), "Treasure Token").len(), 4);
                assert_eq!(sacrifices(branch, source), 1);
                assert_eq!(receipt_count(branch, "StartedWithRiddlerV1"), 1);
                assert_eq!(receipt_count(branch, "Completed"), 4);
                assert!(branch
                    .players
                    .iter()
                    .all(|seat| !seat.has_lost && !seat.drew_from_empty));
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}
#[test]
fn paid_standstill_dredge_hull_riddler_freezes_count_after_first_attempt() {
    for drawer in [P0, P1] {
        for owner in [drawer, drawer.opponent()] {
            let mut state = plains();
            state.active_player = owner;
            state.priority_player = owner;
            put(
                &mut state,
                drawer.opponent(),
                "Hullbreacher",
                Zone::Battlefield,
            );
            put(&mut state, drawer, "Quantum Riddler", Zone::Battlefield);
            let thug = put(&mut state, drawer, "Golgari Thug", Zone::Graveyard);
            let source = paid_standstill(&mut state, owner);
            bolt(&mut state, drawer.opponent());
            assert!(
                matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { player, legal_targets, .. }) if player == drawer && legal_targets == [Target::Object(thug)])
            );
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                act(branch, Action::ChooseEffectTarget(Target::Object(thug)));
                settled(branch);
                assert_eq!(zone(branch, thug), Zone::Hand);
                assert_eq!(branch.players[drawer.index()].library.len(), 16);
                assert_eq!(named(branch, drawer.opponent(), "Treasure Token").len(), 3);
                assert_eq!(sacrifices(branch, source), 1);
                assert_eq!(receipt_count(branch, "StartedWithRiddlerV1"), 1);
                assert_eq!(receipt_count(branch, "Completed"), 4);
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}
#[test]
fn paid_standstill_successful_sacrifice_to_exile_still_draws() {
    for owner in [P0, P1] {
        let mut state = plains();
        state.active_player = owner;
        state.priority_player = owner;
        cast(&mut state, owner, "Rest in Peace", None, &[ManaColor::W], 1);
        settled(&mut state);
        let source = paid_standstill(&mut state, owner);
        bolt(&mut state, owner.opponent());
        let mut replay = restored(&state);
        for branch in [&mut state, &mut replay] {
            settled(branch);
            assert_eq!(zone(branch, source), Zone::Exile);
            assert_eq!(sacrifices(branch, source), 1);
            assert_eq!(branch.players[owner.index()].hand.len(), 3);
            assert_eq!(receipt_count(branch, "Completed"), 3);
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}
#[test]
fn paid_standstill_stacked_cast_triggers_sacrifice_once_and_stale_sources_do_nothing() {
    for owner in [P0, P1] {
        let mut state = plains();
        state.active_player = owner;
        state.priority_player = owner;
        let source = paid_standstill(&mut state, owner);
        bolt(&mut state, owner.opponent());
        bolt(&mut state, owner);
        assert_eq!(source_triggers(&state, source), 2);
        let mut replay = restored(&state);
        for branch in [&mut state, &mut replay] {
            settled(branch);
            assert_eq!(sacrifices(branch, source), 1);
            assert_eq!(branch.players[owner.opponent().index()].hand.len(), 3);
            assert!(branch.players[owner.index()].hand.is_empty());
            assert_eq!(receipt_count(branch, "Started"), 1);
            assert_eq!(receipt_count(branch, "Skipped"), 1);
        }
        assert_eq!(state.state_hash(), replay.state_hash());
        for change in 0..3 {
            let mut state = plains();
            state.active_player = owner;
            state.priority_player = owner;
            let source = paid_standstill(&mut state, owner);
            bolt(&mut state, owner.opponent());
            // Explicit native zone/control test primitives after paid publication,
            // not a claim that a shipped blink or control spell resolved here.
            if change == 2 {
                state.objects.get_mut(source).controller = owner.opponent();
            } else {
                mtg_kernel::event::propose_and_commit(
                    &mut state,
                    mtg_kernel::event::ProposedEvent::zone_change(source, Zone::Exile),
                );
                if change == 1 {
                    mtg_kernel::event::propose_and_commit(
                        &mut state,
                        mtg_kernel::event::ProposedEvent::zone_change(source, Zone::Battlefield),
                    );
                }
            }
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                settled(branch);
                assert!(branch.players.iter().all(|seat| seat.hand.is_empty()));
                assert_eq!(sacrifices(branch, source), 0);
                assert_eq!(receipt_count(branch, "Started"), 0);
                assert_eq!(receipt_count(branch, "Skipped"), 1);
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}
#[test]
fn paid_standstill_restore_rejects_cast_source_sacrifice_and_progress_tampering() {
    for drawer in [P0, P1] {
        let mut state = game(&[]);
        for _ in 0..4 {
            put(&mut state, drawer.opponent(), "Plains", Zone::Library);
        }
        cast_jace(&mut state, drawer);
        put(
            &mut state,
            drawer.opponent(),
            "Hullbreacher",
            Zone::Battlefield,
        );
        let source = paid_standstill(&mut state, drawer);
        bolt(&mut state, drawer.opponent());
        let live_state = state.clone();
        let live_row = state
            .stack
            .iter()
            .find(|item| item.source == source)
            .unwrap()
            .clone();
        assert!(matches!(
            settle(&mut state),
            Some(Decision::ChooseEffectTargets { .. })
        ));
        restored(&state);
        let original = state.engine.pending_effect.as_ref().unwrap();
        let mut borrowed = live_state;
        assert!(mtg_kernel::effect::begin_resumable_resolution(
            live_row.inline_effect.as_ref().unwrap(),
            &original.ctx,
            live_row.clone(),
            &mut borrowed
        )
        .is_err());
        assert_eq!(sacrifices(&borrowed, source), 0);
        for mutation in 0..6 {
            let mut forged = state.clone();
            let pending = forged.engine.pending_effect.as_mut().unwrap();
            match mutation {
                0 => pending.frames.clear(),
                1 => pending.ctx.controller = drawer.opponent(),
                2 => {
                    pending
                        .resolving_item
                        .v4
                        .ability_source_contract
                        .as_mut()
                        .unwrap()
                        .zone_change_count += 1
                }
                3 => pending.resolving_item.is_copy = true,
                4 => {
                    pending.resolving_item.inline_effect =
                        Some(mtg_kernel::effect::EffectOp::DrawCards {
                            player: mtg_kernel::effect::PlayerRef::Controller,
                            count: 3,
                        })
                }
                _ => pending.ctx.stack_item_id = None,
            }
            assert!(
                mtg_kernel::effect::validate_pending_effect_choice(&forged).is_err(),
                "mutation {mutation}"
            );
        }
        for (variant, field) in [
            ("Started", "total"),
            ("Triggered", "cast_at"),
            ("Published", "triggered_at"),
            ("SacrificeCompleted", "sacrifice_at"),
            ("SacrificeCompleted", "departure_at"),
            ("SacrificeCompleted", "departed_generation"),
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
                mtg_kernel::effect::validate_pending_effect_choice(&forged).is_err(),
                "{variant}.{field}"
            );
        }
        let mut json = serde_json::to_value(&state).unwrap();
        json["eternal_owned_draw_progress_v1"]["completed"] = 1.into();
        let forged: GameState = serde_json::from_value(json).unwrap();
        assert!(mtg_kernel::effect::validate_pending_effect_choice(&forged).is_err());
        let mut raw = state.clone();
        raw.engine.pending_effect = None;
        mtg_kernel::effect::execute(
            original.resolving_item.inline_effect.as_ref().unwrap(),
            &original.ctx,
            &mut raw,
        );
        assert!(raw.engine.halted.is_some());
        assert_eq!(sacrifices(&raw, source), 1);
    }
}

#[test]
fn native_vial_metamorph_of_animated_standstill_keeps_ordinary_copied_source_route() {
    for owner in [P0, P1] {
        let mut state = plains();
        state.active_player = owner;
        state.priority_player = owner;
        let vial = cast(&mut state, owner, "Aether Vial", None, &[], 1);
        settled(&mut state);
        cast(
            &mut state,
            owner,
            "Opalescence",
            None,
            &[ManaColor::W, ManaColor::W],
            2,
        );
        settled(&mut state);
        let source = paid_standstill(&mut state, owner);
        // Charge-counter setup control. The Vial activation still pays its
        // actual tap cost and follows the native hand/Metamorph entry choices.
        state.objects.get_mut(vial).counters.charge = 4;
        let copy = put(&mut state, owner, "Phyrexian Metamorph", Zone::Hand);
        assert!(
            matches!(next(&mut state), Decision::CastSpellOrPass { activatable_abilities, .. } if activatable_abilities.contains(&(vial, 0)))
        );
        act(&mut state, Action::ActivateAbility(vial, 0));
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert!(state.objects.get(vial).tapped);
        assert!(
            matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { legal_targets, .. }) if legal_targets.contains(&Target::Object(copy)))
        );
        act(&mut state, Action::ChooseEffectTarget(Target::Object(copy)));
        assert!(
            matches!(next(&mut state), Decision::ChooseEffectTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(source)))
        );
        act(
            &mut state,
            Action::ChooseEffectTarget(Target::Object(source)),
        );
        settled(&mut state);
        assert_eq!(state.objects.get(copy).card_def, id("Standstill"));
        assert!(state
            .eternal_v1
            .as_ref()
            .unwrap()
            .vintage
            .copies
            .iter()
            .any(|entry| entry.host.object == copy));
        bolt(&mut state, owner.opponent());
        assert_eq!(source_triggers(&state, source), 1);
        assert_eq!(source_triggers(&state, copy), 1);
        let mut replay = restored(&state);
        for branch in [&mut state, &mut replay] {
            settled(branch);
            assert_eq!(sacrifices(branch, source), 1);
            assert_eq!(sacrifices(branch, copy), 1);
            assert_eq!(branch.players[owner.index()].hand.len(), 6);
            assert_eq!(
                receipt_count(branch, "Started"),
                1,
                "only the physical source acquires this successor owner"
            );
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}
