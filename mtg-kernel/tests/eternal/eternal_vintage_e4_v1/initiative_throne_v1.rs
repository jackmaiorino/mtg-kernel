//! Initiative and Undercity Throne regressions:
//! - Throne of the Dead Three when Containment Priest replaces the revealed
//!   creature's entry with exile, or Grafdigger's Cage prevents its entry from
//!   the library;
//! - Phyrexian Metamorph copying an initiative granter and later leaving the
//!   battlefield before VentureAfterTaking, VentureAtUpkeep or CombatTransfer
//!   resolves.
//!
//! The Vintage Jewel Shops vs Mono-White Initiative Best-of-Three match seed
//! 72907925487712 (game seed 583263403901698) replays in `eternal_bo3_v1`.

use super::{act, cast, islands, next, pool, put, settle, P0, P1};
use mtg_kernel::card_def::{Keywords, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::mana::ManaColor;
use mtg_kernel::state::{GameState, Step, Target, UndercityRoomV1, Zone};

fn zone(state: &GameState, object: ObjectId) -> Zone {
    state.objects.get(object).zone
}

fn room(state: &GameState, player: PlayerId) -> Option<UndercityRoomV1> {
    state.players[player.index()]
        .dungeon
        .room_id
        .and_then(UndercityRoomV1::from_stable_id)
}

/// Advances `player` into Undercity room 3 (`Archives` / `Catacombs`) so the
/// next initiative venture enters `Throne of the Dead Three`.
fn advance_to_fourth_undercity_room(state: &mut GameState, player: PlayerId) {
    for _ in 0..4 {
        let plume = put(state, player, "White Plume Adventurer", Zone::Hand);
        pool(state, player, &[ManaColor::W], 2);
        cast(state, plume, &[]);
        while let Some(decision) = settle(state) {
            match decision {
                Decision::ChooseEffectOption { .. } => act(state, Action::ChooseEffectOption(0)),
                Decision::ChooseEffectTargets {
                    can_finish: true, ..
                } => act(state, Action::FinishEffectSelection),
                Decision::ChooseEffectTargets { legal_targets, .. } => {
                    act(state, Action::ChooseEffectTarget(legal_targets[0]))
                }
                Decision::ChooseTargets { legal_targets, .. } => {
                    act(state, Action::ChooseTarget(legal_targets[0]))
                }
                Decision::ChooseEffectBoolean { .. } => {
                    act(state, Action::ChooseEffectBoolean(false))
                }
                other => panic!("unexpected room choice: {other:?}"),
            }
        }
    }
    assert_eq!(room(state, player), Some(UndercityRoomV1::Archives));
}

#[test]
fn throne_of_the_dead_three_exiles_under_containment_priest_and_shuffles_without_halting() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        advance_to_fourth_undercity_room(&mut state, actor);

        // Put a creature card on top of `actor`'s library and a Containment
        // Priest under `opponent`'s control, then cast a fifth White Plume
        // Adventurer to trigger Throne of the Dead Three.
        let _priest = put(
            &mut state,
            opponent,
            "Containment Priest",
            Zone::Battlefield,
        );
        let top_creature = put(&mut state, actor, "Seasoned Dungeoneer", Zone::Hand);
        state.players[actor.index()]
            .hand
            .retain(|&c| c != top_creature);
        state.objects.get_mut(top_creature).zone = Zone::Library;
        state.players[actor.index()].library.insert(0, top_creature);

        let fifth_plume = put(&mut state, actor, "White Plume Adventurer", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::W], 2);
        cast(&mut state, fifth_plume, &[]);
        while let Some(decision) = settle(&mut state) {
            match decision {
                Decision::ChooseEffectTargets { legal_targets, .. } => {
                    assert!(legal_targets.contains(&Target::Object(top_creature)));
                    act(
                        &mut state,
                        Action::ChooseEffectTarget(Target::Object(top_creature)),
                    );
                }
                Decision::ChooseEffectOption { .. } => {
                    act(&mut state, Action::ChooseEffectOption(0))
                }
                other => panic!("unexpected Throne choice: {other:?}"),
            }
        }

        assert!(state.engine.halted.is_none());
        assert_eq!(zone(&state, top_creature), Zone::Exile);
        assert_eq!(state.objects.get(top_creature).counters.plus1_plus1, 0);
        assert!(!engine::has_effective_keyword(
            &state,
            top_creature,
            Keywords::HEXPROOF
        ));
        assert_eq!(room(&state, actor), None);
    }
}

#[test]
fn throne_of_the_dead_three_keeps_card_in_library_under_grafdiggers_cage_and_shuffles() {
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;
        advance_to_fourth_undercity_room(&mut state, actor);

        let _cage = put(&mut state, opponent, "Grafdigger's Cage", Zone::Battlefield);
        let top_creature = put(&mut state, actor, "Seasoned Dungeoneer", Zone::Hand);
        state.players[actor.index()]
            .hand
            .retain(|&c| c != top_creature);
        state.objects.get_mut(top_creature).zone = Zone::Library;
        state.players[actor.index()].library.insert(0, top_creature);

        let fifth_plume = put(&mut state, actor, "White Plume Adventurer", Zone::Hand);
        pool(&mut state, actor, &[ManaColor::W], 2);
        cast(&mut state, fifth_plume, &[]);
        while let Some(decision) = settle(&mut state) {
            match decision {
                Decision::ChooseEffectTargets { legal_targets, .. } => {
                    assert!(legal_targets.contains(&Target::Object(top_creature)));
                    act(
                        &mut state,
                        Action::ChooseEffectTarget(Target::Object(top_creature)),
                    );
                }
                Decision::ChooseEffectOption { .. } => {
                    act(&mut state, Action::ChooseEffectOption(0))
                }
                other => panic!("unexpected Throne choice: {other:?}"),
            }
        }

        assert!(state.engine.halted.is_none());
        assert_eq!(zone(&state, top_creature), Zone::Library);
        assert!(state.players[actor.index()].library.contains(&top_creature));
        assert_eq!(state.objects.get(top_creature).counters.plus1_plus1, 0);
        assert_eq!(room(&state, actor), None);
    }
}

#[test]
fn departed_phyrexian_metamorph_copy_of_initiative_granter_keeps_venture_and_combat_transfer_valid()
{
    for actor in [P0, P1] {
        let opponent = actor.opponent();
        let mut state = islands();
        state.active_player = actor;
        state.priority_player = actor;

        let plume = put(
            &mut state,
            opponent,
            "White Plume Adventurer",
            Zone::Battlefield,
        );
        let metamorph = put(&mut state, actor, "Phyrexian Metamorph", Zone::Hand);
        let attacker = put(&mut state, opponent, "Thraben Inspector", Zone::Battlefield);

        // Actor casts Phyrexian Metamorph copying White Plume Adventurer.
        pool(&mut state, actor, &[ManaColor::U], 3);
        cast(&mut state, metamorph, &[]);
        match settle(&mut state) {
            Some(Decision::ChooseEffectTargets { legal_targets, .. }) => {
                assert!(legal_targets.contains(&Target::Object(plume)));
                act(
                    &mut state,
                    Action::ChooseEffectTarget(Target::Object(plume)),
                );
            }
            other => panic!("expected Metamorph copy choice, got {other:?}"),
        }

        // Before the ETB initiative trigger resolves, Metamorph leaves the
        // battlefield so its live `card_def` reverts to Phyrexian Metamorph!
        let _ = next(&mut state);
        event::propose_and_commit(
            &mut state,
            ProposedEvent::zone_change(metamorph, Zone::Graveyard),
        );
        assert_eq!(
            CARD_DEFS[state.objects.get(metamorph).card_def as usize].name,
            "Phyrexian Metamorph"
        );

        // 1. ETB TakeInitiative + VentureAfterTaking resolves cleanly.
        while let Some(decision) = settle(&mut state) {
            match decision {
                Decision::ChooseEffectTargets {
                    can_finish: true, ..
                } => act(&mut state, Action::FinishEffectSelection),
                Decision::ChooseEffectOption { .. } => {
                    act(&mut state, Action::ChooseEffectOption(0))
                }
                other => panic!("unexpected venture choice: {other:?}"),
            }
        }
        assert_eq!(state.initiative, Some(actor));
        assert_eq!(room(&state, actor), Some(UndercityRoomV1::SecretEntrance));

        // 2. Advance to opponent's combat: combat damage transfers the
        // initiative (`CombatTransfer`) even though `initiative_source` was the
        // departed Metamorph, and then on opponent's next upkeep `VentureAtUpkeep`
        // also resolves cleanly without `InvalidEffectContinuation`.
        loop {
            match next(&mut state) {
                Decision::DeclareAttackers { player, eligible } if player == opponent => {
                    assert!(eligible.contains(&attacker), "{eligible:?}");
                    act(&mut state, Action::DeclareAttackers(vec![attacker]));
                    break;
                }
                Decision::DeclareAttackers { .. } => {
                    act(&mut state, Action::DeclareAttackers(vec![]))
                }
                Decision::Discard { count, choices, .. } => act(
                    &mut state,
                    Action::Discard(choices[..count as usize].to_vec()),
                ),
                _ => act(&mut state, Action::Pass),
            }
        }
        while state.initiative != Some(opponent) || !state.stack.is_empty() {
            match next(&mut state) {
                Decision::DeclareBlockers { .. } => {
                    act(&mut state, Action::DeclareBlockers(vec![]))
                }
                Decision::OrderTriggers { pending, .. } => act(
                    &mut state,
                    Action::OrderTriggers((0..pending.len()).collect()),
                ),
                Decision::ChooseEffectOption { .. } => {
                    act(&mut state, Action::ChooseEffectOption(0))
                }
                Decision::ChooseEffectTargets {
                    can_finish: true, ..
                } => act(&mut state, Action::FinishEffectSelection),
                Decision::ChooseEffectTargets { legal_targets, .. } => {
                    act(&mut state, Action::ChooseEffectTarget(legal_targets[0]))
                }
                Decision::ChooseTargets { legal_targets, .. } => {
                    act(&mut state, Action::ChooseTarget(legal_targets[0]))
                }
                _ => act(&mut state, Action::Pass),
            }
        }
        assert_eq!(state.initiative, Some(opponent));

        // Advance through actor's turn to opponent's upkeep to exercise VentureAtUpkeep!
        let room_before = room(&state, opponent);
        while state.active_player != opponent || state.step != Step::Main1 {
            match next(&mut state) {
                Decision::DeclareAttackers { .. } => {
                    act(&mut state, Action::DeclareAttackers(vec![]))
                }
                Decision::OrderTriggers { pending, .. } => act(
                    &mut state,
                    Action::OrderTriggers((0..pending.len()).collect()),
                ),
                Decision::ChooseEffectOption { .. } => {
                    act(&mut state, Action::ChooseEffectOption(0))
                }
                Decision::ChooseEffectTargets {
                    can_finish: true, ..
                } => act(&mut state, Action::FinishEffectSelection),
                Decision::ChooseEffectTargets { legal_targets, .. } => {
                    act(&mut state, Action::ChooseEffectTarget(legal_targets[0]))
                }
                Decision::ChooseTargets { legal_targets, .. } => {
                    act(&mut state, Action::ChooseTarget(legal_targets[0]))
                }
                Decision::ChooseEffectBoolean { .. } => {
                    act(&mut state, Action::ChooseEffectBoolean(false))
                }
                Decision::Discard { count, choices, .. } => act(
                    &mut state,
                    Action::Discard(choices[..count as usize].to_vec()),
                ),
                _ => act(&mut state, Action::Pass),
            }
        }
        assert_ne!(room(&state, opponent), room_before);
        assert!(state.engine.halted.is_none());
    }
}
