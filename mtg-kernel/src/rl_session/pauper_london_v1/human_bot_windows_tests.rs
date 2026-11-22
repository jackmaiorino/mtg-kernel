//! The bot seat on its training window rule in human play
//! (`reset_pauper_london_human_seat_windows_v1`, the bridge reset field
//! `priority_windows`).
//!
//! Proven here:
//!
//! 1. The bridge and trainer start from the same explicit-deck deal. The bot
//!    uses `action-windows-v1` and the trainer's other seat uses `harness-v2`.
//!    The person passes any extra engine windows in the bridge. Every bot
//!    decision must have the trainer's menu, FlatV7 input and new-window marker.
//!    When all sessions use neutral choices, the full game states must match.
//!    Against the ordinary production trainer the bridge selects the trained
//!    bot's choice profile. The person replays extra revealed-card choices
//!    in the trainer's hand or graveyard order. The verifier records the first
//!    discrepancy in each game and requires every pair to reach matching terminals.
//! 2. Both rules `harness-v2` is the engine-windows reset exactly.
//! 3. The bridge takes the rules on its reset request, marks the bot's new
//!    windows in the private support block, and refuses the field anywhere
//!    but the human-play London server and on a seat with engine windows.

use super::*;
use crate::agent_bridge_flat_v4::FlatV4LockstepV1;
use crate::agent_bridge_flat_v4_encode::{
    encode_flat_v7_tensor_v1, flat_v7_extension_value_v1, FlatV4EncodeScratchV1,
};
use crate::agent_bridge_v1::{SpellbenchBridgeServerV1, SpellbenchEngineIdentityV1};
use crate::rl::{ActionSemanticV1, PlayerSeatV1, TargetRefV1};
use crate::state::SplitMix64;
use serde_json::{json, Value};
use std::collections::VecDeque;

const DECKS: [&str; 8] = [
    "Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "CawGates", "Faeries",
];
const LEARNED: [PauperMulliganModeV1; 2] = [PauperMulliganModeV1::Learned; 2];
const LIMITS: (u64, u64) = (200_000, 2_000_000);
const AW: SeatPriorityWindowsV1 = SeatPriorityWindowsV1::ActionWindowsV1;
const HV2: SeatPriorityWindowsV1 = SeatPriorityWindowsV1::HarnessV2;

fn seat_index(seat: PlayerSeatV1) -> usize {
    match seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}

fn mainboards(decks: &SessionDeckIdsV1) -> [Vec<u16>; 2] {
    let resolved = resolve_runtime_decks(decks).unwrap();
    [resolved[0].card_ids.to_vec(), resolved[1].card_ids.to_vec()]
}

fn tensor(fast: &FastActorSessionV1, rows: &[u32], scratch: &mut FlatV4EncodeScratchV1) -> Value {
    let encoded = encode_flat_v7_tensor_v1(fast, scratch).unwrap();
    let mut value = flat_v7_extension_value_v1(&encoded, rows).unwrap();
    value["tensor"].take()
}

#[derive(Clone, Copy, Debug)]
enum NeutralComparison {
    BothNeutral,
    BridgeNeutral,
    TrainerChoicesForBot,
}

struct Sessions {
    training: FastActorSessionV1,
    raw: RlEpisodeSessionV1,
    copy: FlatV4LockstepV1,
}

/// The trainer's London session and the bridge's two sessions on one deal.
/// The training runtime rule is set on all three (refused alike for a game
/// with a Delve card). Neutral choices are the bridge's (every human-play
/// game has them): set on the trainer's session too with
/// `neutral_training`, else the trainer's session is production's.
fn sessions(
    decks: [&str; 2],
    seed: u64,
    episode: u64,
    bot: usize,
    person_windows: bool,
    comparison: NeutralComparison,
) -> Sessions {
    let deck_ids: SessionDeckIdsV1 = [decks[0].to_string(), decks[1].to_string()];
    let boards = mainboards(&deck_ids);
    let mut rules = [HV2; 2];
    rules[bot] = AW;
    let mut training = FastActorSessionV1::reset_pauper_london_windows_v1(
        episode,
        seed,
        LIMITS.0,
        LIMITS.1,
        deck_ids.clone(),
        boards.clone(),
        PlayerId::P0,
        LEARNED,
        rules,
    )
    .unwrap();
    let mut engine_windows = [false; 2];
    engine_windows[1 - bot] = person_windows;
    let start = LondonHumanStartV1 {
        episode_id: episode,
        max_physical_decisions: LIMITS.0,
        max_policy_steps: LIMITS.1,
        modes: LEARNED,
        engine_window_seats: engine_windows,
        action_windows: rules,
        trainer_choice_seats: std::array::from_fn(|seat| {
            seat == bot && matches!(comparison, NeutralComparison::TrainerChoicesForBot)
        }),
    };
    let deal = || {
        build_session_deck_pair_state_from_explicit_decks(&boards, seed, Some(PlayerId::P0))
            .unwrap()
    };
    let (hashes, state) = deal();
    let mut raw =
        RlEpisodeSessionV1::london_human_from_state_v1(start, deck_ids.clone(), hashes, state)
            .unwrap();
    let (hashes, state) = deal();
    let fast =
        FastActorSessionV1::london_human_from_state_v1(start, deck_ids, hashes, state).unwrap();
    let mut copy = FlatV4LockstepV1::from_fast_session_for_test(episode, fast);
    if matches!(comparison, NeutralComparison::BothNeutral) {
        training.enable_spellbench_neutral_choices();
    }
    raw.enable_spellbench_neutral_choices();
    copy.enable_spellbench_neutral_choices();
    let rule = crate::engine::RuntimeRulesV1::RESOLUTION_BOUNDARY_V1;
    let allowed = [
        training.set_runtime_rules_v1(rule).is_ok(),
        raw.set_runtime_rules_v1(rule).is_ok(),
        copy.set_runtime_rules_v1(rule).is_ok(),
    ];
    assert!(allowed == [true; 3] || allowed == [false; 3], "{allowed:?}");
    Sessions {
        training,
        raw,
        copy,
    }
}

#[derive(Default, Debug)]
struct Played {
    bot_decisions: u64,
    bot_new_windows: u64,
    person_engine_windows: u64,
    /// Decisions only the bridge poses, and how many of them belong to the bot.
    neutral_only: u64,
    bot_neutral_only: u64,
    neutral_only_several: u64,
    person_revealed_order_picks: u64,
    person_canonicalized_hand_subsets: u64,
    terminal_person_fireblast_cost: bool,
    bot_after_person_revealed_order: u64,
    /// Against the production trainer: the first difference, if any.
    diverged: Option<String>,
    shared: u64,
    finished: bool,
}

/// Match the person's revealed-card order to the production trainer's committed
/// zone order. A graveyard-order decision that the trainer exposes stays shared.
fn person_revealed_order_pick(
    raw: &RlEpisodeSessionV1,
    training: &FastActorSessionV1,
    decision: &RlSessionDecisionV1,
    rows: &[u32],
) -> Option<u32> {
    use crate::effect::{
        EffectTargetSelectionPurpose::LookTopSelectByTypeToHandBottomRest,
        LibraryPartitionSelectionStage, PendingEffectChoice::SelectTargets,
    };
    use crate::rl::TargetRefV1;
    use crate::state::Target;

    let state = raw.game_state();
    let SelectTargets {
        player,
        selected,
        legal,
        min_targets,
        max_targets,
        purpose:
            LookTopSelectByTypeToHandBottomRest {
                stage,
                canonical_path,
                original_prefix,
                card_type,
                ..
            },
        ..
    } = state.engine.pending_effect.as_ref()?.choice.as_ref()?
    else {
        return None;
    };
    if state
        .engine
        .spellbench_neutral_v1
        .revealed_partition
        .as_ref()
        != Some(canonical_path)
    {
        return None;
    }
    let count = usize::from(*max_targets);
    assert_eq!(min_targets, max_targets);
    assert_eq!(selected.len() + legal.len(), count);
    let baseline = training.game_state();
    let matches_type = |binding: &crate::effect::EffectObjectBinding| {
        let card = state.objects.get(binding.object);
        crate::card_def::CARD_DEFS[card.card_def as usize].has_type(*card_type)
    };
    let player_state = &baseline.players[player.index()];
    let (destination, to_hand) = match stage {
        LibraryPartitionSelectionStage::ChooseMatchingSubset => (&player_state.hand, true),
        LibraryPartitionSelectionStage::OrderRest { .. }
            if !crate::engine::graveyard_order_choice_exposed(baseline, *player) =>
        {
            (&player_state.graveyard, false)
        }
        LibraryPartitionSelectionStage::OrderRest { .. } => return None,
    };
    let expected: Vec<_> = original_prefix
        .iter()
        .filter(|card| matches_type(card) == to_hand)
        .collect();
    assert_eq!(expected.len(), count, "the whole revealed group must move");
    // Other graveyard entries can follow the nonmatches, including the spell.
    let moved: Vec<_> = destination
        .iter()
        .filter_map(|id| {
            let binding = expected.iter().find(|card| card.object == *id)?;
            assert_eq!(
                baseline.objects.get(*id).zone_change_count,
                binding.expected_zone_change_count + 1
            );
            Some(*id)
        })
        .collect();
    assert_eq!(
        moved.len(),
        count,
        "trainer moved each revealed card to its destination"
    );
    for (picked, id) in selected.iter().zip(&moved) {
        assert_eq!(picked.target, Target::Object(*id));
    }
    for id in &moved[selected.len()..] {
        assert!(legal
            .iter()
            .any(|candidate| candidate.target == Target::Object(*id)));
    }
    let next = moved[selected.len()];
    Some(*rows.iter().find(|&&row| matches!(
        &decision.legal_actions[row as usize].semantic,
        ActionSemanticV1::ChooseEffectTarget { target: TargetRefV1::Object { object }, .. }
            if object.arena_id == next.0
    )).expect("the next card in the trainer's zone order has a bridge row"))
}

#[derive(Debug, PartialEq, Eq)]
struct HandSubset<'a> {
    source: crate::ids::ObjectId,
    path: &'a [u16],
    prefix: &'a [crate::effect::EffectObjectBinding],
    selected_count: usize,
}

enum HandSubsetAnswer {
    Pick(TargetRefV1),
    Finish,
}

fn optional_hand_subset(state: &crate::state::GameState) -> Option<HandSubset<'_>> {
    use crate::effect::{
        EffectTargetSelectionPurpose, LibraryPartitionSelectionStage, PendingEffectChoice,
    };

    let effect = state.engine.pending_effect.as_ref()?;
    let PendingEffectChoice::SelectTargets {
        selected,
        min_targets: 0,
        purpose:
            EffectTargetSelectionPurpose::LookTopSelectByTypeToHandBottomRest {
                stage: LibraryPartitionSelectionStage::ChooseMatchingSubset,
                original_prefix,
                canonical_path,
                ..
            },
        ..
    } = effect.choice.as_ref()?
    else {
        return None;
    };
    Some(HandSubset {
        source: effect.resolving_item.source,
        path: canonical_path,
        prefix: original_prefix,
        selected_count: selected.len(),
    })
}

/// Take the same seeded subset as the trainer script, then submit its cards
/// in the trainer's looked-prefix order. The neutral player's engine keeps
/// answer order for this group; the normal trainer canonicalizes it.
fn person_hand_subset_script(
    raw: &RlEpisodeSessionV1,
    training: &FastActorSessionV1,
    rng: &mut SplitMix64,
) -> Option<(VecDeque<HandSubsetAnswer>, bool)> {
    let root = optional_hand_subset(training.game_state())?;
    if root.selected_count != 0 {
        return None;
    }
    assert_eq!(Some(&root), optional_hand_subset(raw.game_state()).as_ref());

    let mut probe = training.clone();
    let mut answers = Vec::new();
    while optional_hand_subset(probe.game_state()).is_some_and(|current| {
        current.source == root.source && current.path == root.path && current.prefix == root.prefix
    }) {
        let FastActorResponseV1::Decision(decision) = probe.current_response() else {
            panic!("the subset is pending without a trainer decision");
        };
        let actions = probe.diagnostic_current_action_semantics().unwrap();
        let row = (rng.next_u64() % actions.len() as u64) as usize;
        let answer = match &actions[row] {
            ActionSemanticV1::ChooseEffectTarget {
                target: target @ TargetRefV1::Object { object },
                ..
            } => (
                root.prefix
                    .iter()
                    .position(|card| card.object.0 == object.arena_id)
                    .expect("the chosen card belongs to the current revealed prefix"),
                HandSubsetAnswer::Pick(target.clone()),
            ),
            ActionSemanticV1::FinishEffectSelection { .. } => {
                (root.prefix.len(), HandSubsetAnswer::Finish)
            }
            other => panic!("unexpected answer in the matching subset: {other:?}"),
        };
        answers.push(answer);
        assert!(answers.len() <= root.prefix.len() + 1);
        probe
            .step(decision.episode_id, decision.step, row as u32)
            .unwrap();
    }
    let reordered = answers.windows(2).any(|pair| pair[0].0 > pair[1].0);
    answers.sort_by_key(|(order, _)| *order);
    Some((
        answers.into_iter().map(|(_, action)| action).collect(),
        reordered,
    ))
}

/// After a shared two-Mountain Fireblast payment, the ordinary engine pays the
/// last Mountain in the same step. The neutral human explicitly confirms it.
fn person_fireblast_remainder(
    raw: &RlEpisodeSessionV1,
    decision: &RlSessionDecisionV1,
    rows: &[u32],
    pick: u32,
) -> Option<ActionSemanticV1> {
    use crate::engine::CostKind;
    let [left, right] = rows else { return None };
    let ActionSemanticV1::ChooseCostTarget {
        source,
        actor,
        candidate,
        cost_kind: CostKind::SacrificeLands,
        remaining: 2,
    } = &decision.legal_actions[pick as usize].semantic
    else {
        return None;
    };
    if raw
        .game_state()
        .objects
        .get(crate::ids::ObjectId(source.arena_id))
        .name
        != "Fireblast"
    {
        return None;
    }
    let other = if *left == pick { right } else { left };
    let ActionSemanticV1::ChooseCostTarget {
        source: other_source,
        actor: other_actor,
        candidate: last,
        cost_kind: CostKind::SacrificeLands,
        remaining: 2,
    } = &decision.legal_actions[*other as usize].semantic
    else {
        panic!("expected a second eligible Mountain")
    };
    assert_eq!((source, actor), (other_source, other_actor));
    assert_ne!(candidate, last);
    Some(ActionSemanticV1::ChooseCostTarget {
        actor: *actor,
        source: source.clone(),
        cost_kind: CostKind::SacrificeLands,
        remaining: 1,
        candidate: last.clone(),
    })
}

/// Plays the training session and the bridge sessions side by side with
/// seeded answers. The person's engine windows (absent from training) are
/// passed in the bridge sessions alone; so are the decisions only neutral
/// choices pose. For the person's revealed partition order, play the training order.
fn play(
    decks: [&str; 2],
    seed: u64,
    episode: u64,
    bot: usize,
    person_windows: bool,
    comparison: NeutralComparison,
    max_shared: u64,
) -> Played {
    let Sessions {
        mut training,
        mut raw,
        mut copy,
    } = sessions(decks, seed, episode, bot, person_windows, comparison);
    let neutral_training = matches!(comparison, NeutralComparison::BothNeutral);
    let mut rng = SplitMix64::seed(seed ^ 0xB07);
    let mut scratch = FlatV4EncodeScratchV1::default();
    let mut hand_subset_answers = VecDeque::new();
    let mut out = Played::default();
    let context = |what: &str, at: u64| {
        format!("{decks:?} seed {seed} bot p{bot} person windows {person_windows} choice profile {comparison:?}: {what} at shared decision {at}")
    };
    // Keep the first mismatch for each normal-training game so the matrix
    // can report all affected deals before enforcing equality.
    macro_rules! require_eq {
        ($left:expr, $right:expr, $what:expr) => {{
            let (left, right) = (&$left, &$right);
            if left != right {
                if neutral_training {
                    panic!("{}: {:?} vs {:?}", context($what, out.shared), left, right);
                }
                out.diverged = Some(format!("{} at shared decision {}", $what, out.shared));
                return out;
            }
        }};
    }
    loop {
        let response = raw.current_response();
        copy.verify(&raw, &response).unwrap_or_else(|error| {
            panic!("{}", context(&format!("lockstep {error:?}"), out.shared))
        });
        if let (Some(_), RlSessionResponseV1::Decision(decision)) =
            (raw.engine_window_v1(), &response)
        {
            if seat_index(decision.acting_player) != bot {
                // The person's engine window: Pass, as the trained surface
                // would have; the training game never sees it.
                let rows = copy.row_candidate_ids(decision).unwrap();
                if out.terminal_person_fireblast_cost {
                    assert_eq!(
                        rows.len(),
                        1,
                        "only a forced human Pass may follow the final payment"
                    );
                }
                let pass = *rows
                    .iter()
                    .find(|id| {
                        matches!(
                            decision.legal_actions[**id as usize].semantic,
                            ActionSemanticV1::Pass { .. }
                        )
                    })
                    .expect("a priority window always offers Pass");
                let chosen = &decision.legal_actions[pass as usize];
                raw.step(decision.episode_id, decision.step, pass, &chosen.stable_id)
                    .unwrap();
                copy.mirror_step(&rows, decision.step, pass).unwrap();
                out.person_engine_windows += 1;
                continue;
            }
        }
        if let RlSessionResponseV1::Decision(decision) = &response {
            // Against the production trainer the states are not compared
            // (neutral choices hold a pending activation in their own
            // fields and number stack items differently); what the seat is
            // asked is: the acting seat, the substep and the menu.
            let shared = match training.current_response() {
                FastActorResponseV1::Decision(theirs) => {
                    theirs.acting_player == decision.acting_player
                        && (theirs.substep_index, theirs.substep_count)
                            == (decision.substep_index, decision.substep_count)
                        && training.diagnostic_current_action_semantics()
                            == copy.fast_session().diagnostic_current_action_semantics()
                }
                FastActorResponseV1::Terminal(_) => {
                    // The training game is over and the bridge's is not.
                    if !neutral_training {
                        out.diverged = Some(format!(
                            "the training game ended first at shared decision {}",
                            out.shared
                        ));
                        return out;
                    }
                    false
                }
            };
            if !shared && !neutral_training {
                // A mismatched priority menu is a divergence for either seat.
                let priority = copy
                    .fast_session()
                    .diagnostic_current_action_semantics()
                    .is_some_and(|menu| {
                        menu.iter()
                            .any(|action| matches!(action, ActionSemanticV1::Pass { .. }))
                    });
                if priority {
                    out.diverged = Some(format!("menu at shared decision {}", out.shared));
                    return out;
                }
                let rows = copy.row_candidate_ids(decision).unwrap();
                if matches!(comparison, NeutralComparison::TrainerChoicesForBot)
                    && seat_index(decision.acting_player) == bot
                {
                    out.diverged = Some(format!(
                        "{} bridge {:?} training {:?}",
                        context("unmatched trained bot decision", out.shared),
                        copy.fast_session().diagnostic_current_action_semantics(),
                        training.diagnostic_current_action_semantics()
                    ));
                    return out;
                }
                out.neutral_only_several += u64::from(rows.len() > 1);
                let person_order = if seat_index(decision.acting_player) != bot && rows.len() > 1 {
                    person_revealed_order_pick(&raw, &training, decision, &rows)
                } else {
                    None
                };
                let pick = if let Some(pick) = person_order {
                    out.person_revealed_order_picks += 1;
                    pick
                } else {
                    assert_eq!(
                        rows.len(),
                        1,
                        "{} bridge {:?} training {:?}",
                        context(
                            "a bridge-only decision with more than one answer",
                            out.shared
                        ),
                        copy.fast_session().diagnostic_current_action_semantics(),
                        training.diagnostic_current_action_semantics()
                    );
                    rows[0]
                };
                let chosen = &decision.legal_actions[pick as usize];
                raw.step(decision.episode_id, decision.step, pick, &chosen.stable_id)
                    .unwrap();
                copy.mirror_step(&rows, decision.step, pick).unwrap();
                out.neutral_only += 1;
                out.bot_neutral_only += u64::from(seat_index(decision.acting_player) == bot);
                continue;
            }
        }
        if neutral_training {
            require_eq!(training.game_state(), raw.game_state(), "game state");
        }
        let ahead = out.person_engine_windows + out.neutral_only;
        let (theirs, ours) = match (training.current_response(), response) {
            (FastActorResponseV1::Decision(theirs), RlSessionResponseV1::Decision(ours)) => {
                (theirs, ours)
            }
            (FastActorResponseV1::Terminal(theirs), RlSessionResponseV1::Terminal(ours)) => {
                require_eq!(
                    (
                        theirs.terminal_outcome,
                        theirs.winner,
                        &theirs.terminal_reason
                    ),
                    (ours.terminal_outcome, ours.winner, &ours.terminal_reason),
                    "terminal"
                );
                if out.terminal_person_fireblast_cost {
                    let mut expected = training.game_state().clone();
                    let profile = &raw.game_state().engine.spellbench_neutral_v1;
                    expected.engine.spellbench_neutral_v1.choices = profile.choices;
                    expected.engine.spellbench_neutral_v1.trainer_choice_seats =
                        profile.trainer_choice_seats;
                    require_eq!(&expected, raw.game_state(), "terminal Fireblast game state");
                    require_eq!(
                        (
                            theirs.terminal_reward,
                            theirs.terminal_classification,
                            theirs.terminal_code,
                            theirs.policy_step_count + ahead,
                            theirs.physical_decision_count + ahead
                        ),
                        (
                            ours.terminal_reward,
                            ours.terminal_classification,
                            ours.terminal_code,
                            ours.policy_step_count,
                            ours.physical_decision_count
                        ),
                        "terminal Fireblast result and counters"
                    );
                }
                out.finished = true;
                return out;
            }
            _ => {
                if neutral_training {
                    panic!(
                        "{}",
                        context("one game ended and the other did not", out.shared)
                    );
                }
                out.diverged = Some(format!("one game ended at shared decision {}", out.shared));
                return out;
            }
        };
        if out.shared >= max_shared {
            return out;
        }
        require_eq!(theirs.acting_player, ours.acting_player, "acting seat");
        require_eq!(
            (theirs.substep_index, theirs.substep_count),
            (ours.substep_index, ours.substep_count),
            "substep"
        );
        require_eq!(
            training.diagnostic_current_action_semantics(),
            copy.fast_session().diagnostic_current_action_semantics(),
            "menu"
        );
        // The session counters differ by exactly the bridge-only decisions.
        require_eq!(
            (training.policy_step_count() + ahead, theirs.step + ahead),
            (raw.policy_step_count(), ours.step),
            "counters"
        );
        let rows = copy.row_candidate_ids(&ours).unwrap();
        if seat_index(ours.acting_player) == bot {
            out.bot_decisions += 1;
            out.bot_after_person_revealed_order += u64::from(out.person_revealed_order_picks > 0);
            let marker = training.priority_pass_window_v1().unwrap();
            require_eq!(
                marker,
                copy.fast_session().priority_pass_window_v1().unwrap(),
                "new-window marker"
            );
            // The bridge marks exactly the trainer's new windows.
            require_eq!(
                marker.is_some(),
                raw.engine_window_v1().is_some(),
                "support mark"
            );
            out.bot_new_windows += u64::from(marker.is_some());
            require_eq!(
                tensor(&training, &rows, &mut scratch),
                tensor(copy.fast_session(), &rows, &mut scratch),
                "model input"
            );
        }
        if hand_subset_answers.is_empty()
            && !neutral_training
            && seat_index(ours.acting_player) != bot
        {
            if let Some((answers, reordered)) = person_hand_subset_script(&raw, &training, &mut rng)
            {
                hand_subset_answers = answers;
                out.person_canonicalized_hand_subsets += u64::from(reordered);
            }
        }
        let row = if let Some(answer) = hand_subset_answers.pop_front() {
            assert_ne!(seat_index(ours.acting_player), bot);
            assert!(optional_hand_subset(raw.game_state()).is_some());
            rows.iter()
                .position(
                    |row| match (&answer, &ours.legal_actions[*row as usize].semantic) {
                        (
                            HandSubsetAnswer::Pick(expected),
                            ActionSemanticV1::ChooseEffectTarget { target, .. },
                        ) => expected == target,
                        (
                            HandSubsetAnswer::Finish,
                            ActionSemanticV1::FinishEffectSelection { .. },
                        ) => true,
                        _ => false,
                    },
                )
                .expect("the planned answer is still legal in the shared menu")
        } else {
            (rng.next_u64() % rows.len() as u64) as usize
        };
        let pick = rows[row];
        let chosen = &ours.legal_actions[pick as usize];
        let final_cost = if matches!(comparison, NeutralComparison::TrainerChoicesForBot)
            && seat_index(ours.acting_player) != bot
        {
            person_fireblast_remainder(&raw, &ours, &rows, pick)
        } else {
            None
        };
        raw.step(ours.episode_id, ours.step, pick, &chosen.stable_id)
            .unwrap();
        copy.mirror_step(&rows, ours.step, pick).unwrap();
        if let Some(expected) = &final_cost {
            let response = raw.current_response();
            copy.verify(&raw, &response).unwrap();
            let RlSessionResponseV1::Decision(cost) = response else {
                panic!("the human's Fireblast payment is pending")
            };
            assert_eq!(cost.acting_player, ours.acting_player);
            let rows = copy.row_candidate_ids(&cost).unwrap();
            let [only] = rows.as_slice() else {
                panic!("the two-card payment leaves exactly one Mountain")
            };
            let confirmed = &cost.legal_actions[*only as usize];
            assert_eq!(&confirmed.semantic, expected);
            raw.step(cost.episode_id, cost.step, *only, &confirmed.stable_id)
                .unwrap();
            copy.mirror_step(&rows, cost.step, *only).unwrap();
            out.neutral_only += 1;
        }
        training
            .step(theirs.episode_id, theirs.step, row as u32)
            .unwrap();
        out.terminal_person_fireblast_cost = final_cost.is_some()
            && matches!(
                training.current_response(),
                FastActorResponseV1::Terminal(_)
            );
        out.shared += 1;
    }
}

#[test]
fn shared_bot_input_after_persons_winding_way_hand_order() {
    let played = play(
        ["Wildfire", "Elves"],
        7,
        0xB0_0000 + 114,
        0,
        true,
        NeutralComparison::BridgeNeutral,
        76,
    );
    assert!(played.person_revealed_order_picks > 0, "{played:?}");
    assert!(played.bot_after_person_revealed_order > 0, "{played:?}");
    println!("Winding Way: {played:?}");
}

#[test]
fn shared_bot_input_after_persons_winding_way_graveyard_order() {
    let played = play(
        ["Spy", "Spy"],
        7,
        0xB0_0000 + 4014,
        0,
        true,
        NeutralComparison::BridgeNeutral,
        94,
    );
    assert!(played.person_revealed_order_picks >= 3, "{played:?}");
    assert!(played.bot_after_person_revealed_order > 0, "{played:?}");
    println!("Spy Winding Way: {played:?}");
}

#[test]
fn trainer_partition_preserves_shared_spy_bot_decisions_in_both_seats() {
    let mut extra_bot_decisions = [0; 2];
    for (bot, last_shared) in [(0, 277), (1, 65)] {
        let played = play(
            ["Spy", "Spy"],
            7,
            0xB0_0000 + 4014 + bot as u64,
            bot,
            true,
            NeutralComparison::TrainerChoicesForBot,
            last_shared,
        );
        assert_eq!(played.shared, last_shared, "{played:?}");
        if bot == 0 {
            assert!(played.person_revealed_order_picks >= 3, "{played:?}");
        }
        extra_bot_decisions[bot] = played.bot_neutral_only;
        println!("Spy seat {bot}: {played:?}");
    }
    assert_eq!(
        extra_bot_decisions,
        [0, 0],
        "the production trainer has no corresponding decisions"
    );
}

#[test]
fn rally_against_spy_matches_the_normal_trainer_through_terminal() {
    let played = play(
        ["Rally", "Spy"],
        7,
        0xB0_0000 + 1114,
        0,
        true,
        NeutralComparison::TrainerChoicesForBot,
        8_000,
    );
    assert!(played.finished && played.diverged.is_none(), "{played:?}");
    assert!(played.bot_decisions > 10, "{played:?}");
    assert!(played.person_canonicalized_hand_subsets > 0, "{played:?}");
}

#[test]
fn human_fireblast_final_mountain_matches_the_terminal_board_in_both_window_settings() {
    for windows in [true, false] {
        let played = play(
            ["Burn", "Wildfire"],
            31,
            0xB0_0000 + 5163,
            1,
            windows,
            NeutralComparison::TrainerChoicesForBot,
            8000,
        );
        assert!(
            played.finished && played.diverged.is_none() && played.terminal_person_fireblast_cost,
            "{played:?}"
        );
        assert_eq!(
            (played.shared, played.bot_decisions, played.bot_neutral_only),
            (166, 97, 0)
        );
    }
}

#[test]
fn the_bot_on_action_windows_plays_the_training_session() {
    let mut total = Played::default();
    let mut games = 0;
    let mut diverged = Vec::new();
    for (index, deck) in DECKS.iter().enumerate() {
        let other = DECKS[(index + 3) % DECKS.len()];
        for (pairing, decks) in [[*deck, *deck], [*deck, other]].into_iter().enumerate() {
            for seed in [7_u64, 31] {
                for bot in [0, 1] {
                    for (person_windows, neutral_training) in
                        [(true, true), (false, true), (true, false), (false, false)]
                    {
                        let episode = 0xB0_0000
                            + index as u64 * 1_000
                            + pairing as u64 * 100
                            + seed * 2
                            + bot as u64;
                        let played = play(
                            decks,
                            seed,
                            episode,
                            bot,
                            person_windows,
                            if neutral_training {
                                NeutralComparison::BothNeutral
                            } else {
                                NeutralComparison::TrainerChoicesForBot
                            },
                            8_000,
                        );
                        if neutral_training {
                            assert_eq!(played.neutral_only, 0);
                        }
                        if neutral_training || played.diverged.is_none() {
                            assert!(
                                played.finished,
                                "{decks:?} seed {seed} bot p{bot}: not finished ({played:?})"
                            );
                        } else {
                            diverged.push(format!("{decks:?} seed {seed} bot p{bot}: {played:?}"));
                        }
                        if played.diverged.is_none() {
                            assert!(
                                played.bot_decisions > 10,
                                "{decks:?} seed {seed}: {played:?}"
                            );
                        }
                        if !person_windows {
                            assert_eq!(played.person_engine_windows, 0);
                        }
                        total.bot_decisions += played.bot_decisions;
                        total.bot_new_windows += played.bot_new_windows;
                        total.person_engine_windows += played.person_engine_windows;
                        total.neutral_only += played.neutral_only;
                        total.person_revealed_order_picks += played.person_revealed_order_picks;
                        total.person_canonicalized_hand_subsets +=
                            played.person_canonicalized_hand_subsets;
                        total.bot_after_person_revealed_order +=
                            played.bot_after_person_revealed_order;
                        total.bot_neutral_only += played.bot_neutral_only;
                        total.neutral_only_several += played.neutral_only_several;
                        total.shared += played.shared;
                        games += 1;
                    }
                }
            }
        }
    }
    println!("human bot windows proof: {games} games, {total:?}");
    println!(
        "against the production trainer, {} of {} games diverged:",
        diverged.len(),
        games / 2
    );
    for line in &diverged {
        println!("  {line}");
    }
    assert!(
        diverged.is_empty(),
        "production comparison failed: {diverged:#?}"
    );
    // The proof is not vacuous: the bot meets new windows and the person
    // meets engine windows the training game does not have.
    assert!(total.bot_new_windows > 100, "{total:?}");
    assert!(total.person_engine_windows > 100, "{total:?}");
}

#[test]
fn both_rules_harness_v2_is_the_engine_windows_reset() {
    for seats in [[false; 2], [true, false], [false, true]] {
        let decks: SessionDeckIdsV1 = ["Spy".to_string(), "Faeries".to_string()];
        let old = RlEpisodeSessionV1::reset_pauper_london_human_windows_v1(
            7,
            19,
            LIMITS.0,
            LIMITS.1,
            decks.clone(),
            LEARNED,
            seats,
        )
        .unwrap();
        let new = RlEpisodeSessionV1::reset_pauper_london_human_seat_windows_v1(
            7,
            19,
            LIMITS.0,
            LIMITS.1,
            decks.clone(),
            LEARNED,
            seats,
            [HV2; 2],
        )
        .unwrap();
        assert_eq!(old.game_state(), new.game_state());
        assert_eq!(old.current_response(), new.current_response());
        // A seat cannot have both.
        for seat in 0..2 {
            let mut rules = [HV2; 2];
            rules[seat] = AW;
            let both = RlEpisodeSessionV1::reset_pauper_london_human_seat_windows_v1(
                7,
                19,
                LIMITS.0,
                LIMITS.1,
                decks.clone(),
                LEARNED,
                seats,
                rules,
            );
            assert_eq!(both.is_err(), seats[seat], "{seats:?} {rules:?}");
            let fast = FastActorSessionV1::reset_pauper_london_human_seat_windows_v1(
                7,
                19,
                LIMITS.0,
                LIMITS.1,
                decks.clone(),
                LEARNED,
                seats,
                rules,
            );
            assert_eq!(fast.is_err(), seats[seat], "{seats:?} {rules:?}");
        }
    }
}

fn request(kind: &str, id: &str, extra: Value) -> String {
    let mut value = json!({"protocol": "spellbench/v1", "request_type": kind, "request_id": id});
    for (key, item) in extra.as_object().unwrap() {
        value[key] = item.clone();
    }
    serde_json::to_string(&value).unwrap()
}

fn reset_request(game: &str, seed: u64, extra: Value) -> String {
    let mut fields = json!({"game_id": game, "format": "pauper-bo1", "game_seed": seed,
        "max_decisions": 100_000, "max_steps": 1_000_000,
        "seats": [{"seat": "p0", "deck": {"catalog_id": "Burn"}},
                  {"seat": "p1", "deck": {"catalog_id": "Faeries"}}]});
    for (key, item) in extra.as_object().unwrap() {
        fields[key] = item.clone();
    }
    request("reset", &format!("reset-{game}"), fields)
}

fn london_server() -> SpellbenchBridgeServerV1 {
    SpellbenchBridgeServerV1::with_pauper_london_human_v1(
        SpellbenchEngineIdentityV1::with_source_revision(None),
    )
}

/// One bridge game with seeded answers (a marked decision of `pass_seat` is
/// answered Pass). Returns the response lines and the marked decisions per
/// seat.
fn bridge_game(extra: Value, seed: u64, pass_seat: Option<&str>) -> (Vec<String>, [u64; 2]) {
    let mut server = london_server();
    let mut lines = vec![server.handle_line(&reset_request("gA", seed, extra))];
    let mut rng = SplitMix64::seed(seed ^ 0xA11);
    let mut marked = [0; 2];
    for step in 0..4_000 {
        let response: Value = serde_json::from_str(lines.last().unwrap()).unwrap();
        if response["response_type"] != "decision" {
            break;
        }
        let candidates = response["candidates"].as_array().unwrap();
        let rows: Vec<usize> = response["extensions"]["x_kernel_flat_v7"]["row_candidate_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_u64().unwrap() as usize)
            .collect();
        let seat = response["acting_seat"].as_str().unwrap().to_string();
        let window = response["extensions"]["x_kernel_v2_support"]["engine_window"].is_string();
        if window {
            marked[usize::from(seat == "p1")] += 1;
        }
        let pick = if window && pass_seat == Some(seat.as_str()) {
            *rows
                .iter()
                .find(|row| candidates[**row]["semantic"]["kind"] == "pass")
                .unwrap()
        } else {
            rows[(rng.next_u64() % rows.len() as u64) as usize]
        };
        lines.push(server.handle_line(&request(
            "step",
            &format!("s{step}"),
            json!({"game_id": "gA", "expected_step": response["step"],
                   "selection": {"candidate_id": candidates[pick]["candidate_id"],
                                 "semantic_echo": candidates[pick]["semantic"]}}),
        )));
    }
    (lines, marked)
}

#[test]
fn the_bridge_takes_the_rules_on_reset_and_marks_the_bots_new_windows() {
    // Absent and both harness-v2: the bytes of the bridge without the field.
    let (plain, none) = bridge_game(
        json!({"engine_window_seats": ["p0"], "auto_graveyard_order": true}),
        23,
        Some("p0"),
    );
    let (spelled, _) = bridge_game(
        json!({"engine_window_seats": ["p0"], "auto_graveyard_order": true,
               "priority_windows": ["harness-v2", "harness-v2"]}),
        23,
        Some("p0"),
    );
    assert_eq!(plain, spelled);
    assert_eq!(none[1], 0);
    // The bot in p1 on action windows: its new windows are marked; the
    // person in p0 keeps engine windows; the game plays to its end.
    let (bot, marked) = bridge_game(
        json!({"engine_window_seats": ["p0"], "auto_graveyard_order": true,
               "priority_windows": ["harness-v2", "action-windows-v1"]}),
        23,
        Some("p0"),
    );
    assert!(marked[1] > 0, "{marked:?}");
    assert!(marked[0] > 0, "{marked:?}");
    let last: Value = serde_json::from_str(bot.last().unwrap()).unwrap();
    assert_eq!(last["response_type"], "terminal", "{last}");
}

#[test]
fn the_bridge_refuses_priority_windows_it_cannot_honour() {
    let mut server = london_server();
    let response: Value = serde_json::from_str(&server.handle_line(&reset_request(
        "gC",
        5,
        json!({"engine_window_seats": ["p1"], "priority_windows": ["harness-v2", "action-windows-v1"]}),
    )))
    .unwrap();
    assert_eq!(response["error"]["code"], "malformed_request", "{response}");
    let response: Value = serde_json::from_str(&server.handle_line(&reset_request(
        "gD",
        5,
        json!({"priority_windows": ["harness-v2", "every-window"]}),
    )))
    .unwrap();
    assert_eq!(response["response_type"], "error", "{response}");
    for options in [
        crate::agent_bridge_v1::SpellbenchBridgeOptionsV1::default(),
        crate::agent_bridge_v1::SpellbenchBridgeOptionsV1 {
            x_kernel_flat_v4: true,
            x_kernel_infoset_v1: false,
            private_v2_support: true,
        },
    ] {
        let mut server = SpellbenchBridgeServerV1::with_options(
            SpellbenchEngineIdentityV1::with_source_revision(None),
            options,
        );
        let response: Value = serde_json::from_str(&server.handle_line(&reset_request(
            "gR",
            5,
            json!({"priority_windows": ["action-windows-v1", "harness-v2"]}),
        )))
        .unwrap();
        assert_eq!(response["error"]["code"], "malformed_request", "{response}");
        // Both harness-v2 is today's surface, taken anywhere.
        let response: Value = serde_json::from_str(&server.handle_line(&reset_request(
            "gR2",
            5,
            json!({"priority_windows": ["harness-v2", "harness-v2"]}),
        )))
        .unwrap();
        assert_eq!(response["response_type"], "decision", "{response}");
    }
}
