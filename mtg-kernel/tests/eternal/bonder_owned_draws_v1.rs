//! Paid Bonder APNAP draws and authenticated replacement continuations.
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
    pool(state, player, &[ManaColor::U, ManaColor::U, ManaColor::U], 1);
    assert!(matches!(next(state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&jace)));
    act(state, Action::CastSpell(jace));
    settled(state);
    jace
}

fn activate(state: &mut GameState, source: ObjectId, actor: PlayerId, active: PlayerId) {
    state.active_player = active;
    state.priority_player = actor;
    pool(state, actor, &[], 4);
    assert!(matches!(next(state), Decision::CastSpellOrPass { activatable_abilities, .. } if activatable_abilities.contains(&(source, 0))));
    act(state, Action::ActivateAbility(source, 0));
    assert!(state.objects.get(source).tapped);
    assert_eq!(state.players[actor.index()].mana_pool, [0; 6]);
}

fn restored(state: &GameState) -> GameState {
    let restored: GameState = serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap();
    assert_eq!(state.state_hash(), restored.state_hash());
    mtg_kernel::effect::validate_pending_effect_choice(&restored).unwrap();
    restored
}

fn observation(state: &GameState, player: PlayerId) -> Result<mtg_kernel::rl::ObservationV2, mtg_kernel::rl::RlContractError> {
    mtg_kernel::rl::observe_v2(state, &mtg_kernel::surface_v2::HarnessSurfaceV2::default(), player, 0)
}

#[test]
fn paid_bonder_ordinary_draws_preserve_current_observation() {
    for actor in [P0, P1] {
        for active in [actor, actor.opponent()] {
            for both in [false, true] {
                let mut state = plains();
                let source = put(&mut state, actor, "Bonder's Ornament", Zone::Battlefield);
                if both { put(&mut state, actor.opponent(), "Bonder's Ornament", Zone::Battlefield); }
                activate(&mut state, source, actor, active);
                settled(&mut state);
                assert_eq!(state.players[actor.index()].hand.len(), 1);
                assert_eq!(state.players[actor.opponent().index()].hand.len(), usize::from(both));
                assert!(observation(&state, actor).is_ok());
                assert!(observation(&restored(&state), actor).is_ok());
            }
        }
    }
}

#[test]
fn paid_bonder_empty_library_jace_hull_both_seats_and_apnap_orders() {
    for actor in [P0, P1] {
        for active in [actor, actor.opponent()] {
            let other = actor.opponent();
            let mut state = game(&[]);
            for _ in 0..4 { put(&mut state, other, "Plains", Zone::Library); }
            let jace = cast_jace(&mut state, actor);
            put(&mut state, other, "Hullbreacher", Zone::Battlefield);
            let source = put(&mut state, actor, "Bonder's Ornament", Zone::Battlefield);
            put(&mut state, other, "Bonder's Ornament", Zone::Battlefield);
            activate(&mut state, source, actor, active);
            let prompt = settle(&mut state).expect("Jace/Hull choice without Dredge");
            assert!(matches!(&prompt, Decision::ChooseEffectTargets { player, source: found, legal_targets, .. } if *player == actor && *found == source && legal_targets == &[Target::Object(jace)]));
            assert_eq!(state.players[other.index()].hand.len(), usize::from(active == other));
            assert!(observation(&state, actor).unwrap_err().to_string().contains("Bonder"));
            let mut replay = restored(&state);
            assert_eq!(prompt, next(&mut replay));
            for branch in [&mut state, &mut replay] {
                act(branch, Action::ChooseEffectTarget(Target::Object(jace)));
                assert!(matches!(next(branch), Decision::GameOver { winner: Some(winner) } if winner == actor));
                assert_eq!(branch.players[other.index()].hand.len(), usize::from(active == other), "a first-player terminal win stops the second draw");
                assert!(named(branch, other, "Treasure Token").is_empty());
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}

#[test]
fn paid_bonder_second_player_restore_cannot_repeat_skip_or_rebind_first_draw() {
    for actor in [P0, P1] {
        let other = actor.opponent();
        let mut state = game(&[]);
        cast_jace(&mut state, actor);
        let other_jace = cast_jace(&mut state, other);
        put(&mut state, actor, "Hullbreacher", Zone::Battlefield);
        put(&mut state, other, "Hullbreacher", Zone::Battlefield);
        let source = put(&mut state, actor, "Bonder's Ornament", Zone::Battlefield);
        put(&mut state, other, "Bonder's Ornament", Zone::Battlefield);
        activate(&mut state, source, actor, actor);
        assert!(matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { player, .. }) if player == actor));
        act(&mut state, Action::FinishEffectSelection);
        let prompt = settle(&mut state).expect("second player's own choice");
        assert!(matches!(&prompt, Decision::ChooseEffectTargets { player, legal_targets, .. } if *player == other && legal_targets == &[Target::Object(other_jace)]));
        assert_eq!(named(&state, other, "Treasure Token").len(), 1);
        let mut replay = restored(&state);
        assert_eq!(prompt, next(&mut replay));
        for mutation in 0..7 {
            let mut forged = state.clone();
            let pending = forged.engine.pending_effect.as_mut().unwrap();
            match mutation {
                0 => { pending.frames.clear(); }
                1 => { pending.resolving_item.v4.activated_ability_index = Some(1); }
                2 => { pending.resolving_item.inline_effect = Some(mtg_kernel::effect::EffectOp::DrawCards { player: mtg_kernel::effect::PlayerRef::Controller, count: 1 }); }
                3 => { pending.resolving_item.v4.ability_source_contract.as_mut().unwrap().zone_change_count += 1; pending.ctx.ability_source_contract = pending.resolving_item.v4.ability_source_contract; }
                4 => { forged.active_player = other; }
                5 => { let ornament = named(&forged, other, "Bonder's Ornament")[0]; forged.objects.get_mut(ornament).controller = actor; }
                _ => { pending.ctx.controller = other; }
            }
            assert!(mtg_kernel::effect::validate_pending_effect_choice(&forged).is_err(), "mutation {mutation}");
        }
        let mut json = serde_json::to_value(&state).unwrap();
        json["eternal_owned_draw_progress_v1"]["completed"] = 1.into();
        let forged: GameState = serde_json::from_value(json).unwrap();
        assert!(mtg_kernel::effect::validate_pending_effect_choice(&forged).is_err());
        // Completion and count are native journal facts, not resumable frame inputs.
        for key in ["Completed", "Started"] {
            let mut json = serde_json::to_value(&state).unwrap();
            fn alter(value: &mut serde_json::Value, key: &str) -> bool {
                match value {
                    serde_json::Value::Object(map) => {
                        if let Some(row) = map.get_mut(key) {
                            let field = if key == "Completed" { "ordinal" } else { "total" };
                            row[field] = 9.into();
                            return true;
                        }
                        map.values_mut().any(|v| alter(v, key))
                    }
                    serde_json::Value::Array(rows) => rows.iter_mut().any(|v| alter(v, key)),
                    _ => false,
                }
            }
            assert!(alter(&mut json["engine"]["event_history"], key));
            let forged: GameState = serde_json::from_value(json).unwrap();
            assert!(mtg_kernel::effect::validate_pending_effect_choice(&forged).is_err(), "changed {key}");
        }
        for branch in [&mut state, &mut replay] {
            act(branch, Action::FinishEffectSelection);
            settled(branch);
            assert_eq!(named(branch, actor, "Treasure Token").len(), 1);
            assert_eq!(named(branch, other, "Treasure Token").len(), 1);
            assert!(branch.players.iter().all(|p| !p.has_lost && !p.drew_from_empty));
            assert!(observation(branch, actor).is_ok());
        }
        assert_eq!(state.state_hash(), replay.state_hash());
    }
}

#[test]
fn paid_bonder_dredge_hull_choices_and_riddler_expand_once_per_player() {
    for actor in [P0, P1] {
        for active in [actor, actor.opponent()] {
            let other = actor.opponent();
            let mut state = plains();
            let source = put(&mut state, actor, "Bonder's Ornament", Zone::Battlefield);
            put(&mut state, other, "Bonder's Ornament", Zone::Battlefield);
            put(&mut state, other, "Hullbreacher", Zone::Battlefield);
            put(&mut state, actor, "Quantum Riddler", Zone::Battlefield);
            put(&mut state, other, "Quantum Riddler", Zone::Battlefield);
            let thug = put(&mut state, actor, "Golgari Thug", Zone::Graveyard);
            activate(&mut state, source, actor, active);
            assert!(matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { player, legal_targets, .. }) if player == actor && legal_targets == [Target::Object(thug)]));
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                act(branch, Action::ChooseEffectTarget(Target::Object(thug)));
                settled(branch);
                assert_eq!(zone(branch, thug), Zone::Hand);
                assert_eq!(branch.players[actor.index()].library.len(), 16);
                assert_eq!(branch.players[other.index()].hand.len(), 2);
                assert_eq!(named(branch, other, "Treasure Token").len(), 1, "exactly one remaining adjusted attempt, despite the small hand");
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}

#[test]
fn paid_bonder_unopposed_jace_stops_apnap_without_dredge_or_hull() {
    for active in [P0, P1] {
        let other = active.opponent();
        let mut state = game(&[]);
        for _ in 0..3 { put(&mut state, other, "Plains", Zone::Library); }
        cast_jace(&mut state, active);
        let source = put(&mut state, active, "Bonder's Ornament", Zone::Battlefield);
        put(&mut state, other, "Bonder's Ornament", Zone::Battlefield);
        activate(&mut state, source, active, active);
        assert!(matches!(settle(&mut state), Some(Decision::GameOver { winner: Some(winner) }) if winner == active));
        assert!(state.players[other.index()].hand.is_empty());
        assert_eq!(state.players[other.index()].library.len(), 3);
    }
}

fn paid_metamorph(state: &mut GameState, actor: PlayerId, donor: ObjectId) -> ObjectId {
    state.active_player = actor;
    state.priority_player = actor;
    let metamorph = put(state, actor, "Phyrexian Metamorph", Zone::Hand);
    pool(state, actor, &[ManaColor::U], 3);
    act(state, Action::CastSpell(metamorph));
    assert!(matches!(settle(state), Some(Decision::ChooseEffectTargets { legal_targets, .. }) if legal_targets.contains(&Target::Object(donor))));
    act(state, Action::ChooseEffectTarget(Target::Object(donor)));
    settled(state);
    assert_eq!(state.objects.get(metamorph).card_def, id("Bonder's Ornament"));
    assert!(state.eternal_v1.as_ref().unwrap().vintage.copies.iter().any(|copy| copy.host.object == metamorph));
    metamorph
}

#[test]
fn paid_metamorph_bonder_keeps_ordinary_native_draw_contract() {
    for actor in [P0, P1] {
        let mut state = plains();
        let donor = put(&mut state, actor.opponent(), "Bonder's Ornament", Zone::Battlefield);
        let source = paid_metamorph(&mut state, actor, donor);
        activate(&mut state, source, actor, actor.opponent());
        settled(&mut state);
        assert_eq!(state.players[actor.index()].hand.len(), 1);
        assert_eq!(state.players[actor.opponent().index()].hand.len(), 1);
        assert!(observation(&state, actor).is_ok());
    }
}

#[test]
fn paid_bonder_retains_original_source_after_departure_and_new_incarnation() {
    for actor in [P0, P1] {
        for copied in [false, true] {
            let mut state = game(&[]);
            let other = actor.opponent();
            let jace = cast_jace(&mut state, actor);
            put(&mut state, other, "Hullbreacher", Zone::Battlefield);
            let donor = put(&mut state, actor, "Bonder's Ornament", Zone::Battlefield);
            put(&mut state, other, "Bonder's Ornament", Zone::Battlefield);
            let source = if copied { paid_metamorph(&mut state, actor, donor) }
                else { put(&mut state, actor, "Bonder's Ornament", Zone::Battlefield) };
            activate(&mut state, source, actor, actor);
            let original_generation = state.objects.get(source).zone_change_count;
            let removal = put(&mut state, actor, "Disenchant", Zone::Hand);
            pool(&mut state, actor, &[ManaColor::W], 1);
            act(&mut state, Action::CastSpell(removal));
            assert!(matches!(next(&mut state), Decision::ChooseTargets { legal_targets, .. } if legal_targets.contains(&Target::Object(source))));
            act(&mut state, Action::ChooseTarget(Target::Object(source)));
            for _ in 0..12 {
                let decision = next(&mut state);
                if zone(&state, source) == Zone::Graveyard { break; }
                assert!(matches!(decision, Decision::CastSpellOrPass { .. }));
                act(&mut state, Action::Pass);
            }
            assert_eq!(zone(&state, source), Zone::Graveyard);
            assert!(state.engine.pending_effect.is_none());
            if copied {
                assert_eq!(state.objects.get(source).card_def, id("Phyrexian Metamorph"));
                assert!(!state.eternal_v1.as_ref().unwrap().vintage.copies.iter().any(|copy| copy.host.object == source));
            } else {
                mtg_kernel::event::propose_and_commit(&mut state, mtg_kernel::event::ProposedEvent::zone_change(source, Zone::Battlefield));
            }
            assert!(state.objects.get(source).zone_change_count > original_generation);
            assert!(matches!(settle(&mut state), Some(Decision::ChooseEffectTargets { player, legal_targets, .. }) if player == actor && legal_targets == [Target::Object(jace)]));
            let mut replay = restored(&state);
            for branch in [&mut state, &mut replay] {
                act(branch, Action::ChooseEffectTarget(Target::Object(jace)));
                assert!(matches!(next(branch), Decision::GameOver { winner: Some(winner) } if winner == actor));
                assert!(named(branch, other, "Treasure Token").is_empty());
            }
            assert_eq!(state.state_hash(), replay.state_hash());
        }
    }
}
