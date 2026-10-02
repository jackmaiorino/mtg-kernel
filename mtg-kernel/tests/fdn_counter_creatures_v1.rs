//! Entry counters, kicker, continuous trample and incarnation-bound landfall.
#![cfg(feature = "limited-fdn-fixtures")]

use mtg_kernel::card_def::{card_id_by_name, preflight_fully_supported_deck, Keywords, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, CommittedEvent, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::rl;
use mtg_kernel::state::{Counters, GameObject, GameState, ObjectStateV4, Step, Zone};
use mtg_kernel::surface_v2::{
    HarnessSurfaceV2, PriorityModeV1, SuppressionAuditMode, SurfaceAction, SurfaceDecision,
};
use mtg_kernel::trigger;
use std::hash::{Hash, Hasher};

fn ready() -> GameState {
    let forest = card_id_by_name("Forest").unwrap();
    let mut state =
        GameState::new_from_libraries(&[forest; 40], &[forest; 40], |_| "Forest".into(), 123);
    state.step = Step::Main1;
    state
}

fn put(state: &mut GameState, player: PlayerId, name: &str, zone: Zone) -> ObjectId {
    let card_def = card_id_by_name(name).unwrap();
    let id = state.objects.push(GameObject {
        card_def,
        name: name.into(),
        owner: player,
        controller: player,
        zone,
        tapped: false,
        summoning_sick: true,
        damage: 0,
        counters: Default::default(),
        attachments: Vec::new(),
        v4: ObjectStateV4::from_card_def(card_def),
        spell_copy_origin: None,
        plotted_turn: None,
        zone_change_count: 0,
    });
    match zone {
        Zone::Hand => state.players[player.index()].hand.push(id),
        Zone::Battlefield => state.players[player.index()].battlefield.push(id),
        _ => panic!("helper zone"),
    }
    id
}

fn move_to(state: &mut GameState, object: ObjectId, zone: Zone) {
    event::propose_and_commit(state, ProposedEvent::zone_change(object, zone));
}

fn enter(state: &mut GameState, player: PlayerId, name: &str) -> ObjectId {
    let id = put(state, player, name, Zone::Hand);
    move_to(state, id, Zone::Battlefield);
    id
}

fn surface() -> HarnessSurfaceV2 {
    HarnessSurfaceV2::new_with_priority_mode_v1(
        PriorityModeV1::EngineWindowsV1,
        SuppressionAuditMode::Full,
    )
}

fn next(surface: &mut HarnessSurfaceV2, state: &mut GameState) -> Decision {
    match surface.next_decision(state) {
        SurfaceDecision::Decision(decision) => decision,
        other => panic!("expected decision: {other:?}"),
    }
}

fn apply(surface: &mut HarnessSurfaceV2, state: &mut GameState, action: Action) {
    surface.apply(state, SurfaceAction::Action(action)).unwrap();
}

fn drain(surface: &mut HarnessSurfaceV2, state: &mut GameState) {
    for _ in 0..100 {
        match next(surface, state) {
            Decision::OrderTriggers { pending, .. } => apply(
                surface,
                state,
                Action::OrderTriggers((0..pending.len()).collect()),
            ),
            Decision::CastSpellOrPass { .. } if state.stack.is_empty() => {
                assert!(state.engine.pending_triggers.is_empty());
                assert!(surface.suppressions().is_empty());
                return;
            }
            Decision::CastSpellOrPass { .. } => apply(surface, state, Action::Pass),
            other => panic!("unexpected counter-creature decision: {other:?}"),
        }
    }
    panic!("did not finish resolving");
}

fn queue(state: &mut GameState) {
    let pending = trigger::collect_and_process(state);
    state.engine.pending_triggers.extend(pending);
}

fn cast_colony(
    state: &mut GameState,
    surface: &mut HarnessSurfaceV2,
    mana: u8,
    kicked: Option<bool>,
) -> ObjectId {
    let colony = put(state, PlayerId::P0, "Gnarlid Colony", Zone::Hand);
    state.players[0].mana_pool[ManaColor::G.pool_index()] = mana;
    assert!(
        matches!(next(surface, state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&colony))
    );
    apply(surface, state, Action::CastSpell(colony));
    if let Some(kicked) = kicked {
        assert!(matches!(
            next(surface, state),
            Decision::ChooseKicker { .. }
        ));
        apply(surface, state, Action::ChooseKicker(kicked));
    }
    assert!(matches!(
        next(surface, state),
        Decision::CastSpellOrPass { .. }
    ));
    assert_eq!(state.stack.last().unwrap().kicked, kicked == Some(true));
    drain(surface, state);
    colony
}

#[test]
fn definitions_append_and_bind_entry_kicker_and_static_recipes() {
    for (id, name, stats) in [
        (186, "Gnarlid Colony", (2, 2)),
        (187, "Mossborn Hydra", (0, 0)),
    ] {
        assert_eq!(card_id_by_name(name), Some(id));
        let def = &CARD_DEFS[id as usize];
        preflight_fully_supported_deck(&[id]).unwrap();
        assert_eq!((def.power, def.toughness), (Some(stats.0), Some(stats.1)));
        assert!(def.enters_with_plus_one_counters.is_some());
    }
}

#[test]
fn colony_uncast_kicker_is_not_inherited_by_an_ordinary_entry() {
    let mut state = ready();
    let colony = enter(&mut state, PlayerId::P0, "Gnarlid Colony");
    assert_eq!(state.objects.get(colony).counters.plus1_plus1, 0);
    assert!(!engine::has_effective_keyword(
        &state,
        colony,
        Keywords::TRAMPLE
    ));
}

#[test]
fn colony_base_kicked_and_declined_casts_pay_the_exact_costs() {
    for (mana, kick, remaining, count) in [
        (2, None, 0, 0),
        (5, Some(true), 0, 2),
        (5, Some(false), 3, 0),
    ] {
        let mut state = ready();
        let mut surface = surface();
        let colony = cast_colony(&mut state, &mut surface, mana, kick);
        assert_eq!(
            state.players[0].mana_pool[ManaColor::G.pool_index()],
            remaining
        );
        assert_eq!(state.objects.get(colony).zone, Zone::Battlefield);
        assert_eq!(state.objects.get(colony).counters.plus1_plus1, count);
        assert_eq!(engine::effective_power(&state, colony), 2 + count);
        assert_eq!(
            engine::has_effective_keyword(&state, colony, Keywords::TRAMPLE),
            count > 0
        );
    }
}

#[test]
fn kicked_colony_loses_counters_and_cast_provenance_after_bounce() {
    let mut state = ready();
    let mut surface = surface();
    let colony = cast_colony(&mut state, &mut surface, 5, Some(true));
    move_to(&mut state, colony, Zone::Hand);
    move_to(&mut state, colony, Zone::Battlefield);
    assert_eq!(state.objects.get(colony).counters.plus1_plus1, 0);
    assert!(!engine::has_effective_keyword(
        &state,
        colony,
        Keywords::TRAMPLE
    ));
}

#[test]
fn continuous_trample_tracks_counters_control_types_and_source_departure() {
    let mut state = ready();
    let colony = enter(&mut state, PlayerId::P0, "Gnarlid Colony");
    let friend = enter(&mut state, PlayerId::P0, "Llanowar Elves");
    let enemy = enter(&mut state, PlayerId::P1, "Llanowar Elves");
    let artifact = enter(&mut state, PlayerId::P0, "Ichor Wellspring");
    for object in [friend, enemy, artifact] {
        state.objects.get_mut(object).counters.plus1_plus1 = 1;
    }
    assert!(engine::has_effective_keyword(
        &state,
        friend,
        Keywords::TRAMPLE
    ));
    assert!(!engine::has_effective_keyword(
        &state,
        enemy,
        Keywords::TRAMPLE
    ));
    assert!(!engine::has_effective_keyword(
        &state,
        artifact,
        Keywords::TRAMPLE
    ));
    state.objects.get_mut(friend).counters.plus1_plus1 = 0;
    assert!(!engine::has_effective_keyword(
        &state,
        friend,
        Keywords::TRAMPLE
    ));
    state.objects.get_mut(friend).counters.plus1_plus1 = 1;
    state.objects.get_mut(colony).controller = PlayerId::P1;
    assert!(!engine::has_effective_keyword(
        &state,
        friend,
        Keywords::TRAMPLE
    ));
    assert!(engine::has_effective_keyword(
        &state,
        enemy,
        Keywords::TRAMPLE
    ));
    move_to(&mut state, colony, Zone::Graveyard);
    assert!(!engine::has_effective_keyword(
        &state,
        enemy,
        Keywords::TRAMPLE
    ));
}

#[test]
fn hydra_enters_alive_with_one_counter_and_has_trample() {
    let mut state = ready();
    let hydra = enter(&mut state, PlayerId::P0, "Mossborn Hydra");
    queue(&mut state);
    assert_eq!(state.objects.get(hydra).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(hydra).counters.plus1_plus1, 1);
    assert_eq!(engine::effective_toughness(&state, hydra), 1);
    assert!(engine::has_effective_keyword(
        &state,
        hydra,
        Keywords::TRAMPLE
    ));
    assert!(state.stack.is_empty());
    assert!(state.engine.pending_triggers.is_empty());
}

#[test]
fn hydra_cast_resolves_with_entry_counters_before_state_based_actions() {
    let mut state = ready();
    let hydra = put(&mut state, PlayerId::P0, "Mossborn Hydra", Zone::Hand);
    state.players[0].mana_pool[ManaColor::G.pool_index()] = 3;
    let mut surface = surface();
    assert!(
        matches!(next(&mut surface, &mut state), Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&hydra))
    );
    apply(&mut surface, &mut state, Action::CastSpell(hydra));
    drain(&mut surface, &mut state);
    assert_eq!(state.objects.get(hydra).zone, Zone::Battlefield);
    assert_eq!(state.objects.get(hydra).counters.plus1_plus1, 1);
    assert_eq!(state.players[0].mana_pool[ManaColor::G.pool_index()], 0);
}

#[test]
fn landfall_ignores_opposing_lands_and_doubles_at_resolution() {
    let mut state = ready();
    let hydra = enter(&mut state, PlayerId::P0, "Mossborn Hydra");
    queue(&mut state);
    enter(&mut state, PlayerId::P1, "Forest");
    queue(&mut state);
    assert!(state.engine.pending_triggers.is_empty());
    enter(&mut state, PlayerId::P0, "Forest");
    queue(&mut state);
    assert_eq!(state.engine.pending_triggers.len(), 1);
    state.objects.get_mut(hydra).counters.plus1_plus1 = 3;
    drain(&mut surface(), &mut state);
    assert_eq!(state.objects.get(hydra).counters.plus1_plus1, 6);
}

#[test]
fn each_land_in_one_event_batch_creates_a_separate_doubling() {
    let mut state = ready();
    let hydra = enter(&mut state, PlayerId::P0, "Mossborn Hydra");
    queue(&mut state);
    let first = put(&mut state, PlayerId::P0, "Forest", Zone::Hand);
    let second = put(&mut state, PlayerId::P0, "Forest", Zone::Hand);
    event::propose_and_commit_batch(
        &mut state,
        vec![
            ProposedEvent::zone_change(first, Zone::Battlefield),
            ProposedEvent::zone_change(second, Zone::Battlefield),
        ],
    );
    queue(&mut state);
    assert_eq!(state.engine.pending_triggers.len(), 2);
    drain(&mut surface(), &mut state);
    assert_eq!(state.objects.get(hydra).counters.plus1_plus1, 4);
}

#[test]
fn pending_landfall_does_not_modify_a_returned_hydra_incarnation() {
    let mut state = ready();
    let hydra = enter(&mut state, PlayerId::P0, "Mossborn Hydra");
    queue(&mut state);
    enter(&mut state, PlayerId::P0, "Forest");
    queue(&mut state);
    move_to(&mut state, hydra, Zone::Hand);
    move_to(&mut state, hydra, Zone::Battlefield);
    drain(&mut surface(), &mut state);
    assert_eq!(state.objects.get(hydra).counters.plus1_plus1, 1);
}

#[test]
fn landfall_crosses_the_old_counter_limit_without_truncating_public_state() {
    let mut state = ready();
    let hydra = enter(&mut state, PlayerId::P0, "Mossborn Hydra");
    queue(&mut state);
    for _ in 0..16 {
        enter(&mut state, PlayerId::P0, "Forest");
        queue(&mut state);
        drain(&mut surface(), &mut state);
    }
    assert_eq!(state.objects.get(hydra).counters.plus1_plus1, 65_536);
    assert_eq!(engine::effective_power(&state, hydra), 65_536);
    let serialized = serde_json::to_string(&state).unwrap();
    let restored: GameState = serde_json::from_str(&serialized).unwrap();
    assert_eq!(state.state_hash(), restored.state_hash());
    assert_eq!(serialized, serde_json::to_string(&restored).unwrap());
    let projected = rl::observe_v2(&state, &surface(), PlayerId::P0, 0).unwrap();
    let public = projected
        .projection
        .engine_context
        .wide_plus_one_counters
        .as_ref()
        .unwrap();
    assert_eq!(public.len(), 1);
    assert_eq!(public[0].permanent.arena_id, hydra.0);
    assert_eq!(public[0].count, 65_536);
}

#[test]
fn counter_hashing_preserves_the_original_width_in_the_legacy_range() {
    for count in [-10i16, 0, 1, i16::MAX] {
        let counters = Counters {
            plus1_plus1: i32::from(count),
            ..Default::default()
        };
        let mut current = std::collections::hash_map::DefaultHasher::new();
        counters.hash(&mut current);
        let mut legacy = std::collections::hash_map::DefaultHasher::new();
        count.hash(&mut legacy);
        for _ in 0..4 {
            0i16.hash(&mut legacy);
        }
        assert_eq!(current.finish(), legacy.finish());
    }
}

#[test]
fn hydra_entry_and_each_doubling_emit_one_exact_counter_event() {
    let mut state = ready();
    let hydra = enter(&mut state, PlayerId::P0, "Mossborn Hydra");
    queue(&mut state);
    enter(&mut state, PlayerId::P0, "Forest");
    queue(&mut state);
    drain(&mut surface(), &mut state);
    let counters: Vec<_> = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::PlusOneCountersAdded {
                object,
                zone_change_count,
                player,
                count,
            } if *object == hydra => Some((*zone_change_count, *player, *count)),
            _ => None,
        })
        .collect();
    assert_eq!(counters, vec![(1, PlayerId::P0, 1), (1, PlayerId::P0, 1)]);
}

#[test]
fn pending_landfall_restores_the_same_bound_source_and_next_transition() {
    let mut state = ready();
    let hydra = enter(&mut state, PlayerId::P0, "Mossborn Hydra");
    queue(&mut state);
    enter(&mut state, PlayerId::P0, "Forest");
    queue(&mut state);
    let mut restored: GameState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    assert_eq!(state.state_hash(), restored.state_hash());
    drain(&mut surface(), &mut state);
    drain(&mut surface(), &mut restored);
    assert_eq!(state.objects.get(hydra).counters.plus1_plus1, 2);
    assert_eq!(
        serde_json::to_vec(&state).unwrap(),
        serde_json::to_vec(&restored).unwrap()
    );
}

#[test]
fn colony_grant_causes_real_trample_damage_through_foundations_choices() {
    let mut state = ready();
    mtg_kernel::combat_damage_v1::enable_foundations_combat_v1(&mut state).unwrap();
    let elf = put(
        &mut state,
        PlayerId::P0,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let blocker = put(
        &mut state,
        PlayerId::P1,
        "Llanowar Elves",
        Zone::Battlefield,
    );
    state.objects.get_mut(elf).summoning_sick = false;
    state.objects.get_mut(blocker).summoning_sick = false;
    state.objects.get_mut(elf).counters.plus1_plus1 = 2;
    enter(&mut state, PlayerId::P0, "Gnarlid Colony");
    let mut surface = surface();
    let mut declared = false;
    for _ in 0..100 {
        let decision = match surface.next_decision(&mut state) {
            SurfaceDecision::DeclareBlockersForAttacker {
                attacker,
                legal_blockers,
            } => {
                assert_eq!(attacker, elf);
                assert!(legal_blockers.contains(&blocker));
                surface
                    .apply(
                        &mut state,
                        SurfaceAction::DeclareBlockersForAttacker(vec![blocker]),
                    )
                    .unwrap();
                continue;
            }
            SurfaceDecision::Decision(decision) => decision,
        };
        match decision {
            Decision::DeclareAttackers { .. } => {
                assert!(!declared);
                apply(
                    &mut surface,
                    &mut state,
                    Action::DeclareAttackers(vec![elf]),
                );
                declared = true;
            }
            Decision::DeclareBlockers { .. } => apply(
                &mut surface,
                &mut state,
                Action::DeclareBlockers(vec![(blocker, elf)]),
            ),
            Decision::ChooseCombatDamageRange { .. } => apply(
                &mut surface,
                &mut state,
                Action::ChooseCombatDamageRange { upper_half: false },
            ),
            Decision::CastSpellOrPass { .. }
                if state.step == Step::CombatDamage
                    && state.objects.get(blocker).zone == Zone::Graveyard =>
            {
                assert_eq!(state.players[1].life, 18);
                assert_eq!(state.objects.get(elf).damage, 1);
                return;
            }
            Decision::CastSpellOrPass { .. } => apply(&mut surface, &mut state, Action::Pass),
            other => panic!("unexpected trample decision: {other:?}"),
        }
    }
    panic!("combat did not finish");
}
