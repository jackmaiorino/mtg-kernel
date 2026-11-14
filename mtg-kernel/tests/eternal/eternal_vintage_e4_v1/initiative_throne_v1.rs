//! Initiative and Undercity Throne regressions:
//! - Throne of the Dead Three when Containment Priest replaces the revealed
//!   creature's entry with exile, or Grafdigger's Cage prevents its entry from
//!   the library;
//! - Phyrexian Metamorph copying an initiative granter and later leaving the
//!   battlefield before VentureAfterTaking, VentureAtUpkeep or CombatTransfer
//!   resolves;
//! - Vintage Jewel Shops vs Mono-White Initiative Best-of-Three match seed
//!   72907925487712 (game seed 583263403901698).

use super::{act, cast, islands, next, pool, put, settle, P0, P1};
use crate::eternal_catalog_v1::{snapshot_decks, splitmix64};
use mtg_kernel::bo3_match::{GameOutcomeV1, MatchPhaseV1, PlayDrawChoiceV1};
use mtg_kernel::card_def::{card_id_by_name, Keywords, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision};
use mtg_kernel::eternal_bo3_v1::{
    EternalBestOfThreeMatchV1, EternalFormatV1, EternalRegisteredDeckV1, RandomSideboardPolicyV1,
};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::limited_session_v1::CustomCardCountV1;
use mtg_kernel::mana::ManaColor;
use mtg_kernel::policy_surface_v5::{PolicyActionV5, PolicyDecisionV5, PolicySurfaceV5};
use mtg_kernel::rl::{policy_legal_action_candidates_v5, ActionSemanticV1};
use mtg_kernel::state::{GameState, Step, Target, UndercityRoomV1, Zone};
use mtg_kernel::surface::{SurfaceAction, SurfaceDecision};

fn id(name: &str) -> u16 {
    card_id_by_name(name).unwrap_or_else(|| panic!("{name}"))
}

fn zone(state: &GameState, object: ObjectId) -> Zone {
    state.objects.get(object).zone
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
    assert_eq!(
        state.players[player.index()].undercity_room,
        Some(UndercityRoomV1::Archives)
    );
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
        assert_eq!(state.players[actor.index()].undercity_room, None);
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
        assert_eq!(state.players[actor.index()].undercity_room, None);
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
        assert_eq!(
            state.players[actor.index()].undercity_room,
            Some(UndercityRoomV1::SecretEntrance)
        );

        // 2. Advance to opponent's combat: combat damage transfers the
        // initiative (`CombatTransfer`) even though `initiative_source` was the
        // departed Metamorph, and then on opponent's next upkeep `VentureAtUpkeep`
        // also resolves cleanly without `InvalidEffectContinuation`.
        while state.active_player != opponent || state.step != Step::DeclareAttackers {
            match next(&mut state) {
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
        let _ = next(&mut state);
        act(&mut state, Action::DeclareAttackers(vec![attacker]));
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
        let room_before = state.players[opponent.index()].undercity_room;
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
        assert_ne!(state.players[opponent.index()].undercity_room, room_before);
        assert!(state.engine.halted.is_none());
    }
}

fn native_registered_deck(archetype: &str) -> EternalRegisteredDeckV1 {
    let found = snapshot_decks()
        .into_iter()
        .find(|deck| deck.format == "vintage" && deck.archetype == archetype)
        .unwrap_or_else(|| panic!("missing vintage snapshot {archetype}"));
    let expand = |rows: &[CustomCardCountV1]| -> Vec<u16> {
        rows.iter()
            .flat_map(|row| std::iter::repeat_n(id(&row.name), row.count as usize))
            .collect()
    };
    EternalRegisteredDeckV1::new_with_native_opening_v1(
        EternalFormatV1::Vintage,
        found.archetype,
        expand(&found.deck.cards),
        expand(&found.sideboard),
        found.deck.companion.as_deref().map(id),
    )
    .unwrap()
}

fn play_native_game(mut state: GameState, seed: u64) -> Option<PlayerId> {
    const MAX_LISTED_TRIGGERS: usize = 7;
    let mut surface = PolicySurfaceV5::new();
    let mut random = seed;
    for step in 0..131_072 {
        let decision = surface
            .next_decision(&mut state)
            .unwrap_or_else(|error| panic!("seed={seed} step={step}: {error}"));
        if let PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::GameOver { winner })) =
            &decision
        {
            return *winner;
        }
        assert!(
            !decision.is_terminal(),
            "seed={seed} step={step}: {decision:?}"
        );
        let action = match &decision {
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(
                Decision::ChooseEternalOpeningV1 { legal_cards, .. },
            )) => {
                let pick = splitmix64(&mut random) % (legal_cards.len() as u64 + 1);
                let opened = legal_cards.get(pick as usize).copied();
                PolicyActionV5::Surface(SurfaceAction::Action(
                    opened.map_or(Action::FinishEffectSelection, |card| {
                        Action::ChooseEffectTarget(Target::Object(card))
                    }),
                ))
            }
            PolicyDecisionV5::Surface(SurfaceDecision::Decision(Decision::OrderTriggers {
                pending,
                ..
            })) if pending.len() > MAX_LISTED_TRIGGERS => PolicyActionV5::Surface(
                SurfaceAction::Action(Action::OrderTriggers((0..pending.len()).collect())),
            ),
            _ => {
                let candidates = policy_legal_action_candidates_v5(&decision, &surface, &state)
                    .unwrap_or_else(|error| panic!("seed={seed} step={step}: {error}"));
                let actor = decision.actor(&state).unwrap();
                let busy = state
                    .stack
                    .iter()
                    .filter(|item| item.controller == actor)
                    .count()
                    >= 2;
                let open: Vec<_> = candidates
                    .iter()
                    .filter(|candidate| {
                        !busy
                            || !matches!(
                                candidate.record.semantic,
                                ActionSemanticV1::CastSpell { .. }
                                    | ActionSemanticV1::ActivateAbility { .. }
                            )
                    })
                    .collect();
                let menu = if open.is_empty() {
                    candidates.iter().collect::<Vec<_>>()
                } else {
                    open
                };
                let pick = (splitmix64(&mut random) % menu.len() as u64) as usize;
                menu[pick].policy_action.clone()
            }
        };
        surface
            .apply_action(&mut state, action)
            .unwrap_or_else(|error| panic!("seed={seed} step={step}: {error}"));
    }
    panic!("seed={seed}: native game exceeded step limit");
}

#[test]
fn vintage_jewel_shops_vs_mono_white_initiative_seed_72907925487712_finishes_cleanly() {
    let seed = 72_907_925_487_712_u64;
    let policy = RandomSideboardPolicyV1 {
        seed,
        max_exchanges: 5,
    };
    let decks = [
        native_registered_deck("vintage-jewel-shops"),
        native_registered_deck("vintage-mono-white-initiative"),
    ];
    let mut bo3 = EternalBestOfThreeMatchV1::new_v1(decks, PlayerId((seed % 2) as u8)).unwrap();
    loop {
        let (game_index, chooser) = match bo3.match_state().phase() {
            MatchPhaseV1::AwaitingPlayDrawChoice {
                game_index,
                chooser,
            } => (game_index, chooser),
            MatchPhaseV1::Complete { .. } => break,
            MatchPhaseV1::AwaitingGameResult { .. } => unreachable!(),
        };
        let configurations =
            [P0, P1].map(|player| policy.configure_v1(bo3.deck(player), player, game_index));
        let game_seed = seed.wrapping_mul(8).wrapping_add(u64::from(game_index));
        let (_game, state) = bo3
            .start_game_with_native_opening_v1(
                chooser,
                PlayDrawChoiceV1::Play,
                configurations,
                game_seed,
            )
            .unwrap();
        let winner = play_native_game(state, game_seed);
        let outcome = winner.map_or(GameOutcomeV1::Draw, |player| GameOutcomeV1::Winner {
            winner: player,
        });
        bo3.record_game_result_v1(outcome).unwrap();
    }
}
