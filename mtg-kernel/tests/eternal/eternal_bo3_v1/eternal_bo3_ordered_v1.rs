//! In-process driver for games played through the registered Premodern
//! ordered-graveyard owner (`FebSessionV1::reset_registered_v1`, step B1 of
//! `docs/design/premodern_shapeshifter_alter_reality_v1.md`).
//!
//! Submissions use numeric ordinals (`FebSessionV1::current` + `FebSessionV1::step`),
//! reading the typed registered view (`registered_decision_view_v1`) to select
//! ranked or seeded-random actions and record casts.

use crate::eternal_catalog_v1::{splitmix64, Policy};
use mtg_kernel::premodern_ordered_public_v1::{
    feb::{FebCurrentV1, FebSessionV1, REGISTERED_PROFILE_V1},
    NumericMenuV1, NumericNativeActionV1, NUMERIC_REGISTERED_SCHEMA_V1,
};
use mtg_kernel::rl::{ActionSemanticV1, PlayerSeatV1};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const MAX_STEPS: usize = 131_072;

/// One registered ordered-graveyard game played to its natural terminal.
pub(crate) struct OrderedGame {
    /// Every numeric observation, chosen numeric action, ordinal, and final
    /// state hash.
    pub(crate) transcript_sha256: String,
    /// Final `GameState::state_hash` at natural `GameOver`.
    pub(crate) state_hash: u64,
    /// The winning seat (`0` or `1`), or `None` for a drawn game.
    pub(crate) winner: Option<usize>,
    /// Spells cast per seat, by card definition id.
    pub(crate) cast_cards: [BTreeMap<u16, usize>; 2],
    /// Chosen action counts keyed by `ActionSemanticV1` action kind (or
    /// `"graveyard_order"` for ordered-graveyard batch choices).
    pub(crate) action_counts: BTreeMap<String, usize>,
}

fn seat_index(seat: PlayerSeatV1) -> usize {
    match seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}

fn ranked_key(row: &NumericNativeActionV1, ordinal: usize) -> (u8, usize) {
    let semantic = &row.action.semantic;
    let attacks = matches!(
        semantic,
        ActionSemanticV1::DeclareAttackers { attackers, .. } if !attackers.is_empty()
    );
    let rank = match semantic {
        ActionSemanticV1::ChooseLondonMulligan {
            mulligan: false, ..
        }
        | ActionSemanticV1::PlayLand { .. } => 0,
        ActionSemanticV1::DeclareAttackers { attackers, .. } if !attackers.is_empty() => 0,
        ActionSemanticV1::DeclareBlockersForAttacker { blockers, .. } if !blockers.is_empty() => 0,
        ActionSemanticV1::CastSpell { .. } => 1,
        ActionSemanticV1::ChooseCombatDamageRange {
            upper_half: false, ..
        } => 2,
        ActionSemanticV1::Pass { .. } => 10,
        ActionSemanticV1::ChooseLondonMulligan { mulligan: true, .. }
        | ActionSemanticV1::ActivateManaAbility { .. }
        | ActionSemanticV1::ActivateAbility { .. } => 11,
        _ => 5,
    };
    let tie = if attacks {
        usize::MAX - ordinal
    } else {
        ordinal
    };
    (rank, tie)
}

/// Plays `owner`, a registered ordered game started with
/// `EternalBestOfThreeMatchV1::start_game_with_ordered_graveyard_v1`, to its
/// natural terminal using numeric ordinals.
pub(crate) fn play(mut owner: FebSessionV1, seed: u64, policy: Policy) -> OrderedGame {
    assert_eq!(owner.profile(), REGISTERED_PROFILE_V1);
    let mut random = match policy {
        Policy::Random(state) => state,
        Policy::Ranked => seed,
    };
    let mut transcript = Sha256::new();
    let mut cast_cards: [BTreeMap<u16, usize>; 2] = Default::default();
    let mut action_counts: BTreeMap<String, usize> = BTreeMap::new();
    let started = std::time::Instant::now();

    for step in 0..=MAX_STEPS {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(600),
            "seed={seed} step={step}: registered ordered game still running after 600 s"
        );
        let (numeric, decision) = match owner.current(PlayerSeatV1::P0) {
            Ok(FebCurrentV1::Decision { numeric, decision }) => (numeric, decision),
            Ok(FebCurrentV1::GameOver { winner }) => {
                assert!(matches!(
                    owner.current(PlayerSeatV1::P1),
                    Ok(FebCurrentV1::GameOver { winner: other }) if other == winner
                ));
                let state_hash = owner
                    .terminal_state_hash_v1()
                    .expect("natural GameOver exposes terminal state hash");
                transcript.update(state_hash.to_le_bytes());
                return OrderedGame {
                    transcript_sha256: format!("{:x}", transcript.finalize()),
                    state_hash,
                    winner: winner.map(seat_index),
                    cast_cards,
                    action_counts,
                };
            }
            Ok(FebCurrentV1::RegisteredHaltV1 { mechanic, source }) => {
                panic!("seed={seed} step={step}: registered halt {mechanic:?} source={source:?}");
            }
            Ok(FebCurrentV1::NativeHalt) => {
                panic!("seed={seed} step={step}: unexpected native halt");
            }
            Ok(FebCurrentV1::UnsupportedDecision) => {
                panic!("seed={seed} step={step}: unsupported decision");
            }
            Err(error) => panic!("seed={seed} step={step}: numeric view failed: {error}"),
        };
        assert!(
            step < MAX_STEPS,
            "seed={seed}: no natural terminal in {MAX_STEPS} steps"
        );
        assert_eq!(numeric.profile, REGISTERED_PROFILE_V1);

        let actor = numeric.actor;
        let (actor_numeric, token) = if let Some(token) = decision {
            assert_eq!(actor, PlayerSeatV1::P0);
            (numeric, token)
        } else {
            assert_eq!(actor, PlayerSeatV1::P1);
            assert!(numeric.actions.is_empty());
            let Ok(FebCurrentV1::Decision {
                numeric: actor_numeric,
                decision: Some(token),
            }) = owner.current(actor)
            else {
                panic!("seed={seed} step={step}: actor {actor:?} lacks a sealed decision");
            };
            (actor_numeric, token)
        };
        assert_eq!(actor_numeric.profile, REGISTERED_PROFILE_V1);
        assert_eq!(actor_numeric.step, step as u64);
        assert!(
            !actor_numeric.actions.is_empty(),
            "seed={seed} step={step}: empty numeric action menu"
        );

        let view = owner
            .registered_decision_view_v1(actor)
            .unwrap_or_else(|error| panic!("seed={seed} step={step}: {error}"))
            .expect("actor has a typed registered view");
        assert_eq!(view.observation.schema, NUMERIC_REGISTERED_SCHEMA_V1);

        let ordinal = match view.menu.as_ref().expect("actor has menu") {
            NumericMenuV1::GraveyardOrder => {
                let candidates = &view
                    .observation
                    .ordered
                    .order_choice
                    .as_ref()
                    .expect("graveyard order menu carries order_choice")
                    .native_candidates;
                assert_eq!(candidates.len(), actor_numeric.actions.len());
                *action_counts
                    .entry("graveyard_order".to_owned())
                    .or_default() += 1;
                match policy {
                    Policy::Ranked => 0,
                    Policy::Random(_) => {
                        (splitmix64(&mut random) % candidates.len() as u64) as usize
                    }
                }
            }
            NumericMenuV1::Native { actions } => {
                assert_eq!(actions.len(), actor_numeric.actions.len());
                let controlled_stack = view.observation.stable.as_ref().map_or(0, |stable| {
                    stable
                        .projection
                        .surface
                        .stack
                        .iter()
                        .filter(|item| item.controller == actor)
                        .count()
                });
                let chosen = match policy {
                    Policy::Ranked => (controlled_stack > 0)
                        .then(|| {
                            actions.iter().position(|row| {
                                matches!(row.action.semantic, ActionSemanticV1::Pass { .. })
                            })
                        })
                        .flatten()
                        .unwrap_or_else(|| {
                            actions
                                .iter()
                                .enumerate()
                                .min_by_key(|&(ordinal, row)| ranked_key(row, ordinal))
                                .map(|(ordinal, _)| ordinal)
                                .unwrap()
                        }),
                    Policy::Random(_) => {
                        let busy = controlled_stack >= 2;
                        let open: Vec<usize> = actions
                            .iter()
                            .enumerate()
                            .filter_map(|(ordinal, row)| {
                                (!busy
                                    || !matches!(
                                        row.action.semantic,
                                        ActionSemanticV1::CastSpell { .. }
                                            | ActionSemanticV1::ActivateAbility { .. }
                                    ))
                                .then_some(ordinal)
                            })
                            .collect();
                        let menu = if open.is_empty() {
                            (0..actions.len()).collect()
                        } else {
                            open
                        };
                        menu[(splitmix64(&mut random) % menu.len() as u64) as usize]
                    }
                };
                if let ActionSemanticV1::CastSpell { actor, source } =
                    &actions[chosen].action.semantic
                {
                    *cast_cards[seat_index(*actor)]
                        .entry(source.card_db_id)
                        .or_default() += 1;
                }
                if let Some(kind) = serde_json::to_value(&actions[chosen].action.semantic).unwrap()
                    ["action_kind"]
                    .as_str()
                {
                    *action_counts.entry(kind.to_owned()).or_default() += 1;
                }
                chosen
            }
        };

        transcript.update((step as u64).to_le_bytes());
        transcript.update([seat_index(actor) as u8]);
        transcript.update((ordinal as u64).to_le_bytes());
        let chosen_tree = &actor_numeric.actions[ordinal];
        let payload = serde_json::to_vec(&(
            actor_numeric.observation.rows(),
            actor_numeric.observation.topology(),
            chosen_tree.rows(),
            chosen_tree.topology(),
        ))
        .unwrap();
        transcript.update(payload);

        owner
            .step(&token, ordinal)
            .unwrap_or_else(|error| panic!("seed={seed} step={step} ordinal={ordinal}: {error}"));
    }
    unreachable!("step cap checked on every iteration")
}
