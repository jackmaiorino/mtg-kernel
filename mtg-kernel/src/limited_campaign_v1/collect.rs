//! Game collection (design Sections 2.1, 3.1 and 5.2): one game per
//! `(pair, leg)` on the sealed FullPool owner, with full retention of the
//! learner seats' multi-action decisions, the explicit terminal classes and
//! a worker pool whose results come back in canonical order.
use super::checkpoint::CheckpointPolicyV1;
use super::diagnostics::{measure_stage, DiagnosticsV1, GameStagesV1};
use super::inventory::DeckV1;
use super::sample_logits_v1;
use super::schedule::{actor_seed_v1, leg_layout_v1, DealV1, SlotV1};
use crate::card_def::KERNEL_CARDDB_HASH;
use crate::current_tree_learner_v1::AuthenticatedInputV1;
use crate::ids::PlayerId;
use crate::limited_full_pool_consumer_v1::{reset_numeric_reveal_facts_v3, ResetV1};
use crate::rl::{PlayerSeatV1, TerminalClassificationV1, TerminalOutcomeV1};
use crate::rl_session::FastActorResponseV1;
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

type Result<T> = std::result::Result<T, String>;

const ACTION_TRACE_TAG_V1: &[u8] = b"mtg-kernel/limited-campaign-action-trace/v1\0";

/// Safety ceilings of one game (design Section 3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameLimitsV1 {
    pub max_turns: u32,
    pub max_decisions: u64,
    pub max_physical_decisions: u64,
    pub max_policy_steps: u64,
}

/// `max_turns 100`, `max_decisions 4_000`, ResetV1 `16_000` and `2_048_000`.
pub const CAMPAIGN_LIMITS_V1: GameLimitsV1 = GameLimitsV1 {
    max_turns: 100,
    max_decisions: 4_000,
    max_physical_decisions: 16_000,
    max_policy_steps: 2_048_000,
};

/// Everything one game depends on.
pub struct GameSpecV1<'a> {
    pub pair: u64,
    pub leg: u8,
    pub deal: DealV1,
    /// The schedule seed of the seat streams
    /// (`actor_seed_v1(stream_seed, pair, leg, seat)`).
    pub stream_seed: u64,
    pub starting: PlayerId,
    pub decks: [&'a DeckV1; 2],
    pub policies: [&'a CheckpointPolicyV1; 2],
    pub policy_labels: [String; 2],
    pub learner_seats: [bool; 2],
    pub limits: GameLimitsV1,
}

/// A labelled policy for [`pair_specs_v1`].
pub struct SeatPolicyV1<'a> {
    pub policy: &'a CheckpointPolicyV1,
    pub label: String,
}

/// How one pair is played.
#[derive(Clone, Copy)]
pub struct PairPlanV1 {
    pub deal: DealV1,
    /// Both seats play policy `a`.
    pub symmetric: bool,
    pub stream_seed: u64,
    /// Retain the decisions of every seat policy `a` plays.
    pub train: bool,
    pub limits: GameLimitsV1,
}

/// The two legs of one pair. Policy `a` is the trained (or candidate) policy
/// and `b` its opponent.
pub fn pair_specs_v1<'a>(
    plan: PairPlanV1,
    decks: &'a [DeckV1],
    a: &SeatPolicyV1<'a>,
    b: &SeatPolicyV1<'a>,
) -> [GameSpecV1<'a>; 2] {
    let PairPlanV1 {
        deal,
        symmetric,
        stream_seed,
        train,
        limits,
    } = plan;
    [0u8, 1].map(|leg| {
        let (slots, starting) = leg_layout_v1(symmetric, &deal, leg);
        let of = |slot| match slot {
            SlotV1::A => a,
            SlotV1::B => b,
        };
        GameSpecV1 {
            pair: deal.pair,
            leg,
            deal,
            stream_seed,
            starting,
            decks: deal.decks.map(|index| &decks[index]),
            policies: slots.map(|slot| of(slot).policy),
            policy_labels: slots.map(|slot| of(slot).label.clone()),
            learner_seats: slots.map(|slot| train && slot == SlotV1::A),
            limits,
        }
    })
}

/// A retained learner decision: `legal > 1`, in decision order.
pub struct RetainedV1 {
    pub seat: u8,
    pub input: AuthenticatedInputV1,
    pub selected: usize,
}

/// How a game ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalV1 {
    /// Natural W/D/L: the returns of seat 0 and seat 1.
    Natural([i8; 2]),
    /// A safety ceiling was reached; the reason names it.
    Truncated(String),
    /// Halted, an inconsistent terminal, or any error: fail closed.
    Failed(String),
}

/// The persisted record of one game (design Section 3.1): the deal, the
/// policies, the counts, the terminal class and a digest of the action
/// trace (SHA-256 over a tag and, per decision, the acting seat as one byte,
/// the sealed menu width and the selected ordinal as little-endian `u32`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameReceiptV1 {
    pub pair: u64,
    pub leg: u8,
    pub env_seed: u64,
    pub starting_player: u8,
    pub decks: [String; 2],
    pub policies: [String; 2],
    pub learner_seats: [bool; 2],
    pub terminal: String,
    pub outcome: Option<String>,
    pub returns: Option<[i8; 2]>,
    pub truncation_reason: Option<String>,
    pub failure: Option<String>,
    pub turns: u32,
    pub decisions: u64,
    pub forced_decisions: u64,
    pub retained_decisions: [u64; 2],
    pub action_trace_sha256: String,
}

pub struct GameRecordV1 {
    pub receipt: GameReceiptV1,
    pub terminal: TerminalV1,
    pub retained: Vec<RetainedV1>,
    pub stages: Option<GameStagesV1>,
}

fn seat_of(seat: PlayerSeatV1) -> usize {
    match seat {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}

/// Plays one game. Forced decisions (one legal action) take ordinal 0
/// without a model call or a draw from the seat stream; every other decision
/// runs the seat's policy and takes exactly one draw.
pub fn play_game_v1(spec: &GameSpecV1<'_>, diagnostics: Option<&DiagnosticsV1>) -> GameRecordV1 {
    let mut stages = diagnostics.map(|_| GameStagesV1::default());
    let mut receipt = GameReceiptV1 {
        pair: spec.pair,
        leg: spec.leg,
        env_seed: spec.deal.env_seed,
        starting_player: spec.starting.index() as u8,
        decks: spec.decks.map(|deck| deck.name.clone()),
        policies: spec.policy_labels.clone(),
        learner_seats: spec.learner_seats,
        terminal: String::new(),
        outcome: None,
        returns: None,
        truncation_reason: None,
        failure: None,
        turns: 0,
        decisions: 0,
        forced_decisions: 0,
        retained_decisions: [0; 2],
        action_trace_sha256: String::new(),
    };
    let mut retained = Vec::new();
    let mut trace = Sha256::new();
    trace.update(ACTION_TRACE_TAG_V1);
    if let Some(diagnostics) = diagnostics {
        diagnostics.record(json!({"game": [spec.pair, spec.leg], "checkpoint": "started"}));
    }
    let played = (|| -> Result<TerminalV1> {
        let reset = ResetV1 {
            expected_card_db_hash: KERNEL_CARDDB_HASH,
            episode_id: spec.deal.env_seed,
            env_seed: spec.deal.env_seed,
            starting_player: spec.starting.into(),
            deck_ids: spec.decks.map(|deck| deck.name.clone()),
            mainboards: spec.decks.map(|deck| deck.cards.clone()),
            max_physical_decisions: spec.limits.max_physical_decisions,
            max_policy_steps: spec.limits.max_policy_steps,
        };
        let mut session = measure_stage(stages.as_mut().map(|s| &mut s.reset), || {
            reset_numeric_reveal_facts_v3(&reset)
        })
        .map_err(|error| format!("reset: {error}"))?;
        let mut streams = [0u8, 1].map(|seat| {
            SplitMix64::seed(actor_seed_v1(spec.stream_seed, spec.pair, spec.leg, seat))
        });
        loop {
            let response = measure_stage(stages.as_mut().map(|s| &mut s.current_response), || {
                session.current_response()
            });
            let expected = match response {
                FastActorResponseV1::Decision(expected) => expected,
                FastActorResponseV1::Terminal(terminal) => {
                    return Ok(
                        match (terminal.terminal_classification, terminal.terminal_outcome) {
                            (TerminalClassificationV1::Natural, TerminalOutcomeV1::P0Win) => {
                                TerminalV1::Natural([1, -1])
                            }
                            (TerminalClassificationV1::Natural, TerminalOutcomeV1::P1Win) => {
                                TerminalV1::Natural([-1, 1])
                            }
                            (TerminalClassificationV1::Natural, TerminalOutcomeV1::Draw) => {
                                TerminalV1::Natural([0, 0])
                            }
                            (TerminalClassificationV1::Natural, outcome) => TerminalV1::Failed(
                                format!("natural terminal has no W/D/L outcome: {outcome:?}"),
                            ),
                            (TerminalClassificationV1::Truncated, _) => TerminalV1::Truncated(
                                format!("session ceiling: {}", terminal.terminal_reason),
                            ),
                            (TerminalClassificationV1::Halted, _) => {
                                TerminalV1::Failed(format!("halted: {}", terminal.terminal_reason))
                            }
                        },
                    );
                }
            };
            let actor = seat_of(expected.acting_player);
            let context = format!(
                "seat {actor} step {} {:?}",
                expected.step, expected.decision_kind
            );
            let decision = measure_stage(stages.as_mut().map(|s| &mut s.encode), || {
                session.encode(expected)
            })
            .map_err(|error| format!("encode {context}: {error}"))?;
            receipt.turns = decision.public_turn();
            if receipt.turns > spec.limits.max_turns {
                return Ok(TerminalV1::Truncated("max_turns".into()));
            }
            if receipt.decisions >= spec.limits.max_decisions {
                return Ok(TerminalV1::Truncated("max_decisions".into()));
            }
            let legal = decision.packet().actions().len();
            let legal_u32 =
                u32::try_from(legal).map_err(|_| format!("menu width exceeds u32: {context}"))?;
            let selected = match legal {
                0 => return Err(format!("decision has no legal action: {context}")),
                1 => {
                    receipt.forced_decisions += 1;
                    0
                }
                _ => {
                    let input =
                        measure_stage(stages.as_mut().map(|s| &mut s.authenticated_input), || {
                            session.training_input_v1(&decision)
                        })
                        .map_err(|error| format!("learner input {context}: {error}"))?;
                    let logits =
                        measure_stage(stages.as_mut().map(|s| &mut s.model_forward), || {
                            spec.policies[actor].logits(&input)
                        })
                        .map_err(|error| format!("forward {context}: {error}"))?;
                    if logits.len() != legal {
                        return Err(format!("forward {context}: logits do not cover the menu"));
                    }
                    let stream = &mut streams[actor];
                    let selected = measure_stage(stages.as_mut().map(|s| &mut s.sample), || {
                        sample_logits_v1(&logits, stream.next_u64())
                    })
                    .map_err(|error| format!("sample {context}: {error}"))?;
                    if spec.learner_seats[actor] {
                        receipt.retained_decisions[actor] += 1;
                        retained.push(RetainedV1 {
                            seat: actor as u8,
                            input,
                            selected,
                        });
                    }
                    selected
                }
            };
            trace.update([actor as u8]);
            trace.update(legal_u32.to_le_bytes());
            trace.update((selected as u32).to_le_bytes());
            measure_stage(stages.as_mut().map(|s| &mut s.apply), || {
                session.apply(&decision, selected as u32)
            })
            .map_err(|error| format!("apply {context}: {error}"))?;
            receipt.decisions += 1;
        }
    })();
    let terminal = played.unwrap_or_else(TerminalV1::Failed);
    match &terminal {
        TerminalV1::Natural(returns) => {
            receipt.terminal = "natural".into();
            receipt.returns = Some(*returns);
            receipt.outcome = Some(
                match returns {
                    [1, _] => "p0_win",
                    [_, 1] => "p1_win",
                    _ => "draw",
                }
                .into(),
            );
        }
        TerminalV1::Truncated(reason) => {
            receipt.terminal = "truncated".into();
            receipt.truncation_reason = Some(reason.clone());
        }
        TerminalV1::Failed(reason) => {
            receipt.terminal = "failed".into();
            receipt.failure = Some(reason.clone());
        }
    }
    receipt.action_trace_sha256 = format!("{:x}", trace.finalize());
    if let Some(diagnostics) = diagnostics {
        diagnostics.record(json!({
            "game": [spec.pair, spec.leg],
            "checkpoint": "returned",
            "terminal": receipt.terminal,
            "decisions": receipt.decisions,
            "stages": stages,
        }));
    }
    GameRecordV1 {
        receipt,
        terminal,
        retained,
        stages,
    }
}

/// Plays every game on `workers` threads (each takes the next unplayed game)
/// and returns the records in the order of `specs`. Each game is a function
/// of its spec alone, so the records are the same for any worker count. A
/// panic in a game becomes a failed record.
pub fn play_games_v1(
    specs: &[GameSpecV1<'_>],
    workers: usize,
    diagnostics: Option<&DiagnosticsV1>,
) -> Vec<GameRecordV1> {
    let next = AtomicUsize::new(0);
    let slots = specs
        .iter()
        .map(|_| Mutex::new(None))
        .collect::<Vec<Mutex<Option<GameRecordV1>>>>();
    std::thread::scope(|scope| {
        for _ in 0..workers.max(1).min(specs.len()) {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(spec) = specs.get(index) else {
                    break;
                };
                let record = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    play_game_v1(spec, diagnostics)
                }))
                .unwrap_or_else(|_| failed_record(spec, "game panicked"));
                *slots[index].lock().expect("result slot poisoned") = Some(record);
            });
        }
    });
    slots
        .into_iter()
        .zip(specs)
        .map(|(slot, spec)| {
            slot.into_inner()
                .expect("result slot poisoned")
                .unwrap_or_else(|| failed_record(spec, "game did not run"))
        })
        .collect()
}

fn failed_record(spec: &GameSpecV1<'_>, reason: &str) -> GameRecordV1 {
    GameRecordV1 {
        receipt: GameReceiptV1 {
            pair: spec.pair,
            leg: spec.leg,
            env_seed: spec.deal.env_seed,
            starting_player: spec.starting.index() as u8,
            decks: spec.decks.map(|deck| deck.name.clone()),
            policies: spec.policy_labels.clone(),
            learner_seats: spec.learner_seats,
            terminal: "failed".into(),
            outcome: None,
            returns: None,
            truncation_reason: None,
            failure: Some(reason.into()),
            turns: 0,
            decisions: 0,
            forced_decisions: 0,
            retained_decisions: [0; 2],
            action_trace_sha256: String::new(),
        },
        terminal: TerminalV1::Failed(reason.into()),
        retained: Vec::new(),
        stages: None,
    }
}
