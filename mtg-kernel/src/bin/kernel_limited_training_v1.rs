//! Bounded original-deck training and fixed-opponent engineering replay.
//! The roster, batch count, maximum decisions and seeds are compile-bound.
use mtg_kernel::card_def::KERNEL_CARDDB_HASH;
use mtg_kernel::current_tree_learner_v1::{
    feature_identity_v1, AdamConfigV1, AuthenticatedInputV1, LearnerV1, ModelV1, SnapshotV1,
    TerminalExampleV1,
};
use mtg_kernel::ids::PlayerId;
use mtg_kernel::limited_match_v1::DeckFileV1;
use mtg_kernel::limited_return_consumer_v1::{reset_numeric_reveal_facts_v3, ResetV1, PROFILE_V1};
use mtg_kernel::rl::{PlayerSeatV1, TerminalClassificationV1, TerminalOutcomeV1};
use mtg_kernel::rl_session::{FastActorDecisionKindV1, FastActorResponseV1};
use mtg_kernel::state::SplitMix64;
use mtg_kernel::unclamped_softmax_sampler_v1::{
    unclamped_softmax_select_v1, unclamped_softmax_unit_v1,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, String>;
const SCHEMA: &str = "limited-original-deck-training-smoke/v2";
const MAX_STEPS: u64 = 4_000;
const RETAIN_PER_SEAT: usize = 32;
const TRAIN_SEED: u64 = 552_001;
const EVAL_SEED: u64 = 552_002;

macro_rules! fixture {
    ($name:literal) => {
        (
            $name,
            include_bytes!(concat!(
                "../../../data/limited/draftzero_sample_v1/decks/",
                $name,
                ".json"
            )) as &[u8],
        )
    };
}
const TRAIN: [(&str, &[u8]); 2] = [fixture!("FDN_top_15157_BR"), fixture!("FDN_top_09703_UB")];
const EVAL: [(&str, &[u8]); 2] = [fixture!("FDN_top_26941_WB"), fixture!("FDN_top_30836_UR")];

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn adam() -> AdamConfigV1 {
    AdamConfigV1 {
        learning_rate: 0.001,
        beta1: 0.9,
        beta2: 0.999,
        epsilon: 1e-8,
    }
}

fn config() -> Value {
    let roster = |decks: [(&str, &[u8]); 2]| {
        decks.map(|(name, bytes)| json!({"name": name, "source_sha256": hash(bytes), "cards": 40}))
    };
    json!({
        "schema": SCHEMA,
        "binary_source_sha256": hash(include_bytes!("kernel_limited_training_v1.rs")),
        "card_db_hash": format!("{KERNEL_CARDDB_HASH:016x}"),
        "feature_sha256": feature_identity_v1(),
        "actor_profile": PROFILE_V1,
        "actor_producer_sha256": mtg_kernel::limited_return_consumer_v1::producer_sha256_v1(),
        "hand_knowledge": "public-reveal-minima/v3",
        "reveal_producer_sha256": hash(include_bytes!("../reveal_meaning_v3.rs")),
        "reveal_reducer_sha256": hash(include_bytes!("../reveal_meaning_v3/reducer.rs")),
        "objective": "mean-terminal-reinforce-gamma1-zero-critic/v1",
        "sampler": "f64-exp-icdf-u53/v1",
        "optimizer": adam(),
        "max_policy_steps": MAX_STEPS,
        "retain_first_multi_action_decisions_per_seat": RETAIN_PER_SEAT,
        "train_decks": roster(TRAIN),
        "train_seed": TRAIN_SEED,
        "eval_decks": roster(EVAL),
        "eval_seed": EVAL_SEED,
        "eval_opponent": "uniform-legal/v1",
    })
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DecisionKind {
    Surface,
    AttackerInclusion,
    BlockerInclusion,
}

#[derive(Serialize, Deserialize)]
struct ActionRow {
    actor: usize,
    step: u64,
    kind: DecisionKind,
    legal: u32,
    selected: usize,
    retained_for_update: bool,
}

#[derive(Serialize, Deserialize)]
struct GameReceipt {
    leg: usize,
    seed: u64,
    decks: [String; 2],
    model_seat: Option<usize>,
    start: usize,
    reward: Option<[i8; 2]>,
    failure: Option<String>,
    actions: Vec<ActionRow>,
}

struct Captured {
    actor: usize,
    input: AuthenticatedInputV1,
    selected: usize,
}

struct Game {
    receipt: GameReceipt,
    captured: Vec<Captured>,
}

fn sample(scores: &[f64], rng: &mut SplitMix64) -> Result<usize> {
    let max = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut sum = 0.;
    let prefix = scores
        .iter()
        .map(|score| {
            sum += (score - max).exp();
            sum
        })
        .collect::<Vec<_>>();
    unclamped_softmax_select_v1(&prefix, unclamped_softmax_unit_v1(rng.next_u64()))
        .map_err(|error| error.to_string())
}

fn game(roster: [(&str, &[u8]); 2], seed: u64, leg: usize, model: &ModelV1, collect: bool) -> Game {
    let start = if collect && leg == 1 {
        PlayerId::P1
    } else {
        PlayerId::P0
    };
    let model_seat = (!collect).then_some(leg);
    let mut receipt = GameReceipt {
        leg,
        seed,
        decks: roster.map(|(name, _)| name.into()),
        model_seat,
        start: start.index(),
        reward: None,
        failure: None,
        actions: Vec::new(),
    };
    let mut captured = Vec::new();
    let attempt = (|| -> Result<[i8; 2]> {
        let resolve = |(name, bytes): (&str, &[u8])| -> Result<Vec<u16>> {
            let deck: DeckFileV1 = serde_json::from_slice(bytes)
                .map_err(|error| format!("configuration {name}: {error}"))?;
            let cards = deck
                .resolve()
                .map_err(|error| format!("configuration {name}: {error}"))?;
            if cards.len() != 40 || deck.name.as_deref() != Some(name) {
                return Err(format!(
                    "configuration {name}: expected the original 40-card deck"
                ));
            }
            Ok(cards)
        };
        let decks = [resolve(roster[0])?, resolve(roster[1])?];
        let mut session = reset_numeric_reveal_facts_v3(&ResetV1 {
            expected_card_db_hash: KERNEL_CARDDB_HASH,
            episode_id: seed,
            env_seed: seed,
            starting_player: start.into(),
            deck_ids: receipt.decks.clone(),
            mainboards: decks,
            max_physical_decisions: MAX_STEPS,
            max_policy_steps: MAX_STEPS,
        })
        .map_err(|error| format!("session: {error}"))?;
        let mut rng = [
            SplitMix64::seed(seed ^ 0x3ac6_352a_54aa_8ba0),
            SplitMix64::seed(seed ^ 0xda7a_2418_d42e_9b1d),
        ];
        let mut retained = [0; 2];
        loop {
            let decision = match session.current_response() {
                FastActorResponseV1::Decision(decision) => decision,
                FastActorResponseV1::Terminal(terminal) => {
                    if terminal.terminal_classification != TerminalClassificationV1::Natural {
                        return Err(format!(
                            "{:?}: {}",
                            terminal.terminal_classification, terminal.terminal_reason
                        ));
                    }
                    return Ok(match terminal.terminal_outcome {
                        TerminalOutcomeV1::P0Win => [1, -1],
                        TerminalOutcomeV1::P1Win => [-1, 1],
                        TerminalOutcomeV1::Draw => [0, 0],
                        _ => return Err("natural terminal has no W/D/L outcome".into()),
                    });
                }
            };
            let actor = match decision.acting_player {
                PlayerSeatV1::P0 => 0,
                PlayerSeatV1::P1 => 1,
            };
            let context = format!(
                "seat {actor} step {} {:?} width {}",
                decision.step, decision.decision_kind, decision.legal_action_count
            );
            let bound = session
                .encode(decision)
                .map_err(|error| format!("encode {context}: {error}"))?;
            let input = if model_seat.is_none_or(|seat| seat == actor) {
                Some(
                    session
                        .training_input_v1(&bound)
                        .map_err(|error| format!("tensor {context}: {error}"))?,
                )
            } else {
                None
            };
            let scores = match &input {
                Some(input) => model
                    .forward(input)
                    .map_err(|error| format!("inference {context}: {error}"))?,
                None => vec![0.; decision.legal_action_count as usize],
            };
            let selected = sample(&scores, &mut rng[actor])
                .map_err(|error| format!("sample {context}: {error}"))?;
            let retain = collect && scores.len() > 1 && retained[actor] < RETAIN_PER_SEAT;
            if retain {
                retained[actor] += 1;
                captured.push(Captured {
                    actor,
                    input: input.expect("self-play uses the learner"),
                    selected,
                });
            }
            receipt.actions.push(ActionRow {
                actor,
                step: decision.step,
                kind: match decision.decision_kind {
                    FastActorDecisionKindV1::Surface => DecisionKind::Surface,
                    FastActorDecisionKindV1::AttackerInclusion => DecisionKind::AttackerInclusion,
                    FastActorDecisionKindV1::BlockerInclusion => DecisionKind::BlockerInclusion,
                },
                legal: decision.legal_action_count,
                selected,
                retained_for_update: retain,
            });
            session
                .apply(&bound, selected as u32)
                .map_err(|error| format!("apply {context}: {error}"))?;
        }
    })();
    match attempt {
        Ok(reward) => receipt.reward = Some(reward),
        Err(error) => receipt.failure = Some(error),
    }
    Game { receipt, captured }
}

fn pair(
    roster: [(&str, &[u8]); 2],
    seed: u64,
    model: &ModelV1,
    collect: bool,
    workers: usize,
) -> [Game; 2] {
    let one = |leg| game(roster, seed, leg, model, collect);
    if workers == 1 {
        [one(0), one(1)]
    } else {
        std::thread::scope(|scope| {
            let first = scope.spawn(|| one(0));
            let second = one(1);
            [first.join().expect("first leg worker panicked"), second]
        })
    }
}

fn write_atomic(root: &Path, name: &str, value: &impl Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    let staging = root.join(format!(".{name}.staging"));
    let mut file = File::options()
        .create_new(true)
        .write(true)
        .open(&staging)
        .map_err(|error| error.to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| error.to_string())?;
    drop(file);
    fs::rename(staging, root.join(name)).map_err(|error| error.to_string())?;
    Ok(hash(&bytes))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Training {
    config: Value,
    initial: SnapshotV1,
    trained: SnapshotV1,
    loss_bits: u64,
    games: [GameReceipt; 2],
}

fn train(root: &Path, workers: usize) -> Result<Training> {
    let mut learner = LearnerV1::new(ModelV1::deterministic_fixture(), adam())
        .map_err(|error| error.to_string())?;
    let initial = learner.snapshot();
    let games = pair(TRAIN, TRAIN_SEED, learner.model(), true, workers);
    if games.iter().any(|game| game.receipt.reward.is_none()) {
        write_atomic(
            root,
            "failed-training.json",
            &json!({
                "config": config(), "initial": initial, "games": games.map(|game| game.receipt),
            }),
        )?;
        return Err("training pair did not finish naturally. See failed-training.json".into());
    }
    let batch = games
        .iter()
        .flat_map(|game| {
            let reward = game.receipt.reward.expect("checked natural game");
            game.captured.iter().map(move |sample| TerminalExampleV1 {
                input: &sample.input,
                selected: sample.selected,
                reward: reward[sample.actor],
            })
        })
        .collect::<Vec<_>>();
    let initial_parameters = learner.model().parameters().to_vec();
    let updated = learner
        .update_terminal(&batch)
        .map_err(|error| error.to_string())
        .and_then(|loss| {
            if learner.model().parameters() == initial_parameters.as_slice() {
                return Err("terminal batch did not change any parameter".into());
            }
            Ok(loss)
        });
    match updated {
        Ok(loss) => Ok(Training {
            config: config(),
            initial,
            trained: learner.snapshot(),
            loss_bits: loss.to_bits(),
            games: games.map(|game| game.receipt),
        }),
        Err(error) => {
            write_atomic(
                root,
                "failed-training.json",
                &json!({
                    "config": config(), "initial": initial, "games": games.map(|game| game.receipt), "update_error": error.to_string(),
                }),
            )?;
            Err(format!("training update: {error}"))
        }
    }
}

fn load_training(path: &Path, expected: Option<&str>) -> Result<(Training, String)> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let digest = hash(&bytes);
    if expected.is_some_and(|expected| expected != digest) {
        return Err("training artifact SHA-256 does not match the resume pin".into());
    }
    let training: Training = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if training.config != config() || training.games.iter().any(|game| game.reward.is_none()) {
        return Err("training artifact has another configuration or incomplete games".into());
    }
    Ok((training, digest))
}

fn run(root: &Path, workers: usize, resume: Option<(PathBuf, String)>) -> Result<()> {
    let local = root.join("training.json");
    let resumed = resume
        .as_ref()
        .map(|(path, digest)| load_training(path, Some(digest)))
        .transpose()?;
    fs::create_dir(root).map_err(|error| format!("{}: {error}", root.display()))?;
    let (training, parent_sha256) = if let Some(training) = resumed {
        training
    } else {
        let training = train(root, workers)?;
        write_atomic(root, "training.json", &training)?;
        load_training(&local, None)?
    };
    let initial = LearnerV1::restore(training.initial).map_err(|error| error.to_string())?;
    let trained = LearnerV1::restore(training.trained).map_err(|error| error.to_string())?;
    let eval = |learner: &LearnerV1| {
        pair(EVAL, EVAL_SEED, learner.model(), false, workers).map(|g| g.receipt)
    };
    let initial_eval = eval(&initial);
    let trained_eval = eval(&trained);
    let complete = initial_eval
        .iter()
        .chain(&trained_eval)
        .all(|game| game.reward.is_some());
    let tally = |games: &[GameReceipt; 2]| {
        let mut result = [0usize; 3];
        for game in games {
            if let (Some(seat), Some(reward)) = (game.model_seat, game.reward) {
                result[(1 - reward[seat]) as usize] += 1;
            }
        }
        json!({"wins": result[0], "draws": result[1], "losses": result[2]})
    };
    let result = json!({"config": config(), "training_sha256": parent_sha256,
        "initial_wdl": tally(&initial_eval), "trained_wdl": tally(&trained_eval),
        "complete": complete, "initial": initial_eval, "trained": trained_eval});
    let result_hash = write_atomic(root, "evaluation.json", &result)?;
    if !complete {
        return Err(
            "fixed opponent evaluation did not finish naturally. See evaluation.json".into(),
        );
    }
    println!("{result_hash}");
    Ok(())
}

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let parse = || -> Result<(PathBuf, usize, Option<(PathBuf, String)>)> {
        let usage = "kernel_limited_training_v1 OUT_DIR 1|2 [TRAINING_JSON SHA256]";
        let [out, workers, tail @ ..] = args.as_slice() else {
            return Err(usage.into());
        };
        let workers = match workers.to_str() {
            Some("1") => 1,
            Some("2") => 2,
            _ => return Err(usage.into()),
        };
        let resume = match tail {
            [] => None,
            [path, digest] => Some((
                PathBuf::from(path),
                digest.to_str().ok_or(usage)?.to_string(),
            )),
            _ => return Err(usage.into()),
        };
        Ok((PathBuf::from(out), workers, resume))
    };
    if let Err(error) = parse().and_then(|(root, workers, resume)| run(&root, workers, resume)) {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
