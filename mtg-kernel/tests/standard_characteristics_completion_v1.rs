//! Effective Room and transforming-face identity, including public projection.
#![cfg(feature = "standard-magezero-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, CardType, Supertype, TargetSpec, CARD_DEFS};
use mtg_kernel::effect::{self, ExecCtx};
use mtg_kernel::engine::{self, Action, CastMode, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::policy_surface_v5::PolicySurfaceV5;
use mtg_kernel::rl::{self, CardCharacteristicsV2};
use mtg_kernel::standard_cards_v1::StandardTargetV1;
use mtg_kernel::state::{GameObject, GameState, ObjectStateV4, Step, Target, Zone};
use std::hash::{Hash, Hasher};

const P0: PlayerId = PlayerId::P0;
const P1: PlayerId = PlayerId::P1;
const ROOM: &str = "Unholy Annex // Ritual Chamber";
const OJER: &str = "Ojer Axonil, Deepest Might";

fn game() -> GameState {
    let island = card_id_by_name("Island").unwrap();
    let mut state = GameState::new_from_libraries(
        &[island; 20],
        &[island; 20],
        |id| CARD_DEFS[id as usize].object_name.into(),
        0x524f_4f4d,
    );
    state.active_player = P0;
    state.priority_player = P0;
    state.step = Step::Main1;
    state
}
fn put(state: &mut GameState, owner: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let object = state.objects.push(GameObject {
        card_def,
        name: CARD_DEFS[card_def as usize].object_name.into(),
        owner,
        controller: owner,
        zone: Zone::Hand,
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
    state.players[owner.index()].hand.push(object);
    if zone != Zone::Hand {
        event::propose_and_commit(state, ProposedEvent::zone_change(object, zone));
        state.objects.get_mut(object).summoning_sick = false;
    }
    object
}
fn next(state: &mut GameState) -> Decision {
    let decision = engine::advance_until_decision(state);
    assert!(!matches!(decision, Decision::Halted { .. }), "{decision:?}");
    decision
}
fn act(state: &mut GameState, action: Action) {
    engine::step(state, action.clone()).unwrap_or_else(|error| panic!("{action:?}: {error}"));
}
fn settle(state: &mut GameState) {
    for _ in 0..100 {
        match next(state) {
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => return,
            Decision::CastSpellOrPass { .. } => act(state, Action::Pass),
            Decision::OrderTriggers { pending, .. } => {
                act(state, Action::OrderTriggers((0..pending.len()).collect()))
            }
            decision => panic!("unexpected {decision:?}"),
        }
    }
    panic!("stack failed to settle");
}
fn unlock(state: &mut GameState, object: ObjectId, door: u8) {
    let controller = state.objects.get(object).controller;
    let ctx = ExecCtx::no_targets(object, controller);
    let effect = if door == 0 {
        mtg_kernel::standard_cards_v1::unlock_left_door()
    } else {
        mtg_kernel::standard_cards_v1::unlock_right_door()
    };
    effect::execute(&effect, &ctx, state);
}
fn characteristics(state: &GameState, object: ObjectId) -> CardCharacteristicsV2 {
    rl::observe_policy_v6(state, &PolicySurfaceV5::new(), P0, 0, 0, 0, 1)
        .unwrap()
        .projection
        .battlefield
        .iter()
        .flatten()
        .find(|card| card.stable.arena_id == object.0)
        .unwrap()
        .characteristics
        .clone()
}

#[test]
fn rooms_use_unlocked_names_values_and_colors_and_reset_on_zone_change() {
    let mut state = game();
    let room = put(&mut state, P0, ROOM, Zone::Hand);
    assert_eq!(
        engine::effective_names(&state, room),
        ["Unholy Annex", "Ritual Chamber"]
    );
    assert_eq!(engine::object_mana_value(&state, room), 8);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(room, Zone::Battlefield),
    );
    assert!(engine::effective_names(&state, room).is_empty());
    assert_eq!(engine::object_mana_value(&state, room), 0);
    assert_eq!(engine::object_color_mask(&state, room), 0);
    assert!(engine::object_has_type(&state, room, CardType::Enchantment));
    let identity = characteristics(&state, room).effective_identity.unwrap();
    assert!(identity.names.is_empty());
    assert_eq!(identity.mana_value, 0);
    assert!(identity.supertypes.is_empty());
    state.players[0].mana_pool[ManaColor::B.pool_index()] = 3;
    next(&mut state);
    act(&mut state, Action::ActivateAbility(room, 0));
    settle(&mut state);
    assert_eq!(engine::effective_names(&state, room), ["Unholy Annex"]);
    assert_eq!(engine::object_mana_value(&state, room), 3);
    assert_eq!(
        engine::object_color_mask(&state, room),
        1 << ManaColor::B.pool_index()
    );
    state.players[0].mana_pool[ManaColor::B.pool_index()] = 5;
    act(&mut state, Action::ActivateAbility(room, 1));
    settle(&mut state);
    assert_eq!(
        engine::effective_names(&state, room),
        ["Unholy Annex", "Ritual Chamber"]
    );
    assert_eq!(engine::object_mana_value(&state, room), 8);
    let identity = characteristics(&state, room).effective_identity.unwrap();
    assert_eq!(identity.names, ["Unholy Annex", "Ritual Chamber"]);
    assert_eq!(identity.mana_value, 8);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(room, Zone::Graveyard),
    );
    assert_eq!(engine::object_mana_value(&state, room), 8);
    event::propose_and_commit(
        &mut state,
        ProposedEvent::zone_change(room, Zone::Battlefield),
    );
    assert_eq!(engine::object_mana_value(&state, room), 0);
    assert!(engine::effective_names(&state, room).is_empty());
}

#[test]
fn room_casts_have_only_the_selected_door_identity_on_stack_and_battlefield() {
    for (mode, name, mana_value) in [
        (CastMode::Normal, "Unholy Annex", 3),
        (CastMode::Alternative, "Ritual Chamber", 5),
    ] {
        let mut state = game();
        let room = put(&mut state, P0, ROOM, Zone::Hand);
        state.players[0].mana_pool[ManaColor::B.pool_index()] = 5;
        next(&mut state);
        act(&mut state, Action::CastSpell(room));
        assert!(matches!(next(&mut state), Decision::ChooseCastMode { .. }));
        act(&mut state, Action::ChooseCastMode(mode));
        assert!(matches!(next(&mut state), Decision::CastSpellOrPass { .. }));
        assert_eq!(state.objects.get(room).zone, Zone::Stack);
        assert_eq!(engine::effective_names(&state, room), [name]);
        assert_eq!(engine::object_mana_value(&state, room), mana_value);
        settle(&mut state);
        assert_eq!(state.objects.get(room).zone, Zone::Battlefield);
        assert_eq!(engine::effective_names(&state, room), [name]);
        assert_eq!(engine::object_mana_value(&state, room), mana_value);
        let observation =
            rl::observe_policy_v6(&state, &PolicySurfaceV5::new(), P0, 0, 0, 0, 1).unwrap();
        let card = observation.projection.battlefield[0]
            .iter()
            .find(|card| card.stable.arena_id == room.0)
            .unwrap();
        assert_eq!(card.card_name, name);
        assert_eq!(
            card.characteristics
                .effective_identity
                .as_ref()
                .unwrap()
                .mana_value,
            mana_value
        );
    }
}

#[test]
fn etali_can_cast_either_room_door_free_during_combat_and_resume_after_restore() {
    for (mode, name, mana_value) in [
        (CastMode::Normal, "Unholy Annex", 3),
        (CastMode::Alternative, "Ritual Chamber", 5),
    ] {
        let mut state = game();
        state.step = Step::DeclareBlockers;
        let room = put(&mut state, P1, ROOM, Zone::Library);
        put(&mut state, P0, "Etali, Primal Conqueror", Zone::Battlefield);
        let mut chose_card = false;
        let mut chose_door = false;
        for _ in 0..30 {
            match next(&mut state) {
                Decision::ChooseEffectTargets { legal_targets, .. } => {
                    assert!(!chose_card);
                    assert!(legal_targets.contains(&Target::Object(room)));
                    act(&mut state, Action::ChooseEffectTarget(Target::Object(room)));
                    chose_card = true;
                }
                Decision::ChooseCastMode { spell, options, .. } => {
                    assert_eq!(spell, room);
                    assert_eq!(options, [CastMode::Normal, CastMode::Alternative]);
                    let mut restored: GameState =
                        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
                    act(&mut state, Action::ChooseCastMode(mode));
                    act(&mut restored, Action::ChooseCastMode(mode));
                    assert_eq!(next(&mut state), next(&mut restored));
                    assert_eq!(state, restored);
                    assert_eq!(state.objects.get(room).zone, Zone::Stack);
                    assert_eq!(engine::effective_names(&state, room), [name]);
                    assert_eq!(engine::object_mana_value(&state, room), mana_value);
                    assert!(state.engine.pending_effect.is_none());
                    chose_door = true;
                    break;
                }
                Decision::CastSpellOrPass { .. } => act(&mut state, Action::Pass),
                Decision::OrderTriggers { pending, .. } => act(
                    &mut state,
                    Action::OrderTriggers((0..pending.len()).collect()),
                ),
                other => panic!("Etali Room cast: {other:?}"),
            }
        }
        assert!(chose_card && chose_door);
        settle(&mut state);
        assert_eq!(state.objects.get(room).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(room).owner, P1);
        assert_eq!(state.objects.get(room).controller, P0);
        assert_eq!(engine::effective_names(&state, room), [name]);
        assert_eq!(engine::object_mana_value(&state, room), mana_value);
        assert_eq!(state.players[0].mana_pool, [0; 6]);
    }
}

#[test]
fn portable_hole_uses_room_current_mana_value() {
    let mut state = game();
    let locked = put(&mut state, P1, ROOM, Zone::Battlefield);
    let left = put(&mut state, P1, ROOM, Zone::Battlefield);
    let right = put(&mut state, P1, ROOM, Zone::Battlefield);
    unlock(&mut state, left, 0);
    unlock(&mut state, right, 1);
    let targets = engine::legal_targets_for(
        TargetSpec::StandardV1(StandardTargetV1::OpponentNonlandPermanentManaValueAtMost(2)),
        &[],
        &state,
    );
    assert!(targets.contains(&Target::Object(locked)));
    assert!(!targets.contains(&Target::Object(left)));
    assert!(!targets.contains(&Target::Object(right)));
}

#[test]
fn maelstrom_pulse_matches_any_shared_name_but_not_two_nameless_rooms() {
    for target_locked in [false, true] {
        let mut state = game();
        let left = put(&mut state, P1, ROOM, Zone::Battlefield);
        let both = put(&mut state, P1, ROOM, Zone::Battlefield);
        let right = put(&mut state, P1, ROOM, Zone::Battlefield);
        let locked = put(&mut state, P1, ROOM, Zone::Battlefield);
        let other_locked = put(&mut state, P1, ROOM, Zone::Battlefield);
        unlock(&mut state, left, 0);
        unlock(&mut state, both, 0);
        unlock(&mut state, both, 1);
        unlock(&mut state, right, 1);
        settle(&mut state);
        let spell = put(&mut state, P0, "Maelstrom Pulse", Zone::Hand);
        state.players[0].mana_pool[ManaColor::B.pool_index()] = 2;
        state.players[0].mana_pool[ManaColor::G.pool_index()] = 1;
        next(&mut state);
        act(&mut state, Action::CastSpell(spell));
        assert!(matches!(next(&mut state), Decision::ChooseTargets { .. }));
        act(
            &mut state,
            Action::ChooseTarget(Target::Object(if target_locked { locked } else { left })),
        );
        settle(&mut state);
        for object in [left, both, right, locked, other_locked] {
            let destroyed = if target_locked {
                object == locked
            } else {
                object == left || object == both
            };
            assert_eq!(
                state.objects.get(object).zone,
                if destroyed {
                    Zone::Graveyard
                } else {
                    Zone::Battlefield
                }
            );
        }
    }
}

#[test]
fn multiple_temples_are_nonlegendary_but_keep_front_face_mana_value() {
    let mut state = game();
    let first = put(&mut state, P0, OJER, Zone::Battlefield);
    event::propose_and_commit(&mut state, ProposedEvent::transform_in_place(first, 1));
    let second = put(&mut state, P0, OJER, Zone::Battlefield);
    event::propose_and_commit(&mut state, ProposedEvent::transform_in_place(second, 1));
    let front = put(&mut state, P0, OJER, Zone::Battlefield);
    settle(&mut state);
    for temple in [first, second] {
        assert_eq!(state.objects.get(temple).zone, Zone::Battlefield);
        assert_eq!(engine::effective_name(&state, temple), "Temple of Power");
        assert!(engine::object_has_type(&state, temple, CardType::Land));
        assert!(engine::effective_supertypes(&state, temple).is_empty());
        assert_eq!(engine::object_mana_value(&state, temple), 4);
        let identity = characteristics(&state, temple).effective_identity.unwrap();
        assert!(identity.supertypes.is_empty());
        assert_eq!(identity.names, ["Temple of Power"]);
        assert_eq!(identity.mana_value, 4);
    }
    assert!(engine::effective_supertypes(&state, front).contains(&Supertype::Legendary));
    assert!(characteristics(&state, front).effective_identity.is_none());
}

#[test]
fn absent_effective_identity_preserves_characteristic_hash_and_json() {
    let mut state = game();
    let island = put(&mut state, P0, "Island", Zone::Battlefield);
    let card = characteristics(&state, island);
    assert!(card.effective_identity.is_none());
    assert!(serde_json::to_value(&card)
        .unwrap()
        .get("effective_identity")
        .is_none());
    let mut actual = std::collections::hash_map::DefaultHasher::new();
    card.hash(&mut actual);
    let mut legacy = std::collections::hash_map::DefaultHasher::new();
    card.legend_return_sources.hash(&mut legacy);
    card.legend_rules.hash(&mut legacy);
    card.base_pt_until_end_of_turn.hash(&mut legacy);
    card.type_flags.hash(&mut legacy);
    card.base_power.hash(&mut legacy);
    card.base_toughness.hash(&mut legacy);
    card.effective_power.hash(&mut legacy);
    card.effective_toughness.hash(&mut legacy);
    card.effective_color_mask.hash(&mut legacy);
    card.effective_subtype_ids.hash(&mut legacy);
    card.effective_keywords.hash(&mut legacy);
    assert_eq!(actual.finish(), legacy.finish());
}

#[test]
fn packleader_entry_threshold_counts_only_unlocked_room_doors() {
    for (door, expected_counter) in [(None, 0), (Some(0), 0), (Some(1), 1)] {
        let mut state = game();
        let room = put(&mut state, P0, ROOM, Zone::Battlefield);
        if let Some(door) = door {
            unlock(&mut state, room, door);
        }
        let packleader = put(&mut state, P0, "Ascendant Packleader", Zone::Battlefield);
        assert_eq!(
            state.objects.get(packleader).counters.plus1_plus1,
            expected_counter
        );
    }
}
