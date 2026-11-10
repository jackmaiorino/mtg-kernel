//! Bounded original-deck training and fixed-opponent engineering replay.
//! The roster, batch count, maximum decisions and seeds are compile-bound.
use mtg_kernel::card_def::KERNEL_CARDDB_HASH;
use mtg_kernel::current_mixed_policy_v1::KaitoSession;
use mtg_kernel::current_tree_learner_v1::{
    feature_identity_v1, AdamConfigV1, AuthenticatedInputV1, LearnerV1, ModelV1, SnapshotV1,
    TerminalExampleV1,
};
use mtg_kernel::ids::PlayerId;
use mtg_kernel::limited_match_v1::{DeckFileV1, SessionKindV1};
use mtg_kernel::limited_return_consumer_v1::ResetV1;
use mtg_kernel::rl::{PlayerSeatV1, TerminalClassificationV1, TerminalOutcomeV1};
use mtg_kernel::rl_session::{FastActorDecisionKindV1, FastActorResponseV1};
use mtg_kernel::state::SplitMix64;
use mtg_kernel::unclamped_softmax_sampler_v1::{
    unclamped_softmax_select_v1, unclamped_softmax_unit_v1,
};
use mtg_kernel::{
    limited_full_pool_consumer_v1 as full_pool, limited_return_consumer_v1 as returns,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

type Result<T> = std::result::Result<T, String>;
const SCHEMA: &str = "limited-original-deck-training-smoke/v2";
const MAX_STEPS: u64 = 4_000;
const RETAIN_PER_SEAT: usize = 32;
const TRAIN_SEED: u64 = 552_001;
const EVAL_SEED: u64 = 552_002;
const DIAGNOSTIC_INTERVAL: usize = 64;

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

#[derive(Clone, Copy)]
enum Session {
    Return,
    FullPool,
}

impl Session {
    fn profile(self) -> &'static str {
        match self {
            Self::Return => returns::PROFILE_V1,
            Self::FullPool => full_pool::PROFILE_V1,
        }
    }

    fn producer_sha256(self) -> String {
        match self {
            Self::Return => returns::producer_sha256_v1(),
            Self::FullPool => full_pool::producer_sha256_v1(),
        }
    }

    fn reset(self, config: &ResetV1) -> Result<KaitoSession> {
        match self {
            Self::Return => returns::reset_numeric_reveal_facts_v3(config),
            Self::FullPool => full_pool::reset_numeric_reveal_facts_v3(config),
        }
    }
}

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

fn config(session: Session) -> Value {
    let roster = |decks: [(&str, &[u8]); 2]| {
        decks.map(|(name, bytes)| json!({"name": name, "source_sha256": hash(bytes), "cards": 40}))
    };
    json!({
        "schema": SCHEMA,
        "binary_source_sha256": hash(include_bytes!("kernel_limited_training_v1.rs")),
        "card_db_hash": format!("{KERNEL_CARDDB_HASH:016x}"),
        "feature_sha256": feature_identity_v1(),
        "actor_profile": session.profile(),
        "actor_producer_sha256": session.producer_sha256(),
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

struct Diagnostics {
    started: Instant,
    file: Mutex<Option<File>>,
}

impl Diagnostics {
    fn create(root: &Path) -> Result<Self> {
        let file = File::options()
            .write(true)
            .create_new(true)
            .open(root.join("stage-diagnostics.jsonl"))
            .map_err(|error| format!("stage diagnostics: {error}"))?;
        Ok(Self {
            started: Instant::now(),
            file: Mutex::new(Some(file)),
        })
    }

    fn record(&self, event: Value) {
        let mut file = self.file.lock().expect("stage diagnostics writer panicked");
        if let Some(writer) = file.as_mut() {
            let mut row = json!({
                "schema": "limited-training-stage-diagnostics/v1",
                "elapsed_ns": elapsed_ns(self.started),
                "event": event,
            })
            .to_string()
            .into_bytes();
            row.push(b'\n');
            // Write directly so a process timeout leaves the completed lines available.
            if let Err(error) = writer.write_all(&row) {
                *file = None;
                eprintln!("stage diagnostics write failed: {error}");
            }
        }
    }
}

fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn measure_phase<T>(
    diagnostics: Option<&Diagnostics>,
    phase: &str,
    operation: impl FnOnce() -> T,
) -> T {
    let Some(diagnostics) = diagnostics else {
        return operation();
    };
    diagnostics.record(json!({"phase": phase, "checkpoint": "started"}));
    let started = Instant::now();
    let result = operation();
    let duration_ns = elapsed_ns(started);
    diagnostics
        .record(json!({"phase": phase, "checkpoint": "returned", "duration_ns": duration_ns}));
    result
}

#[derive(Default, Serialize)]
struct StageCounter {
    count: u64,
    total_ns: u64,
    max_ns: u64,
}

fn measure_stage<T>(counter: Option<&mut StageCounter>, operation: impl FnOnce() -> T) -> T {
    let Some(counter) = counter else {
        return operation();
    };
    let started = Instant::now();
    let result = operation();
    let duration_ns = elapsed_ns(started);
    counter.count += 1;
    counter.total_ns = counter.total_ns.saturating_add(duration_ns);
    counter.max_ns = counter.max_ns.max(duration_ns);
    result
}

#[derive(Default, Serialize)]
struct GameStages {
    reset: StageCounter,
    current_response: StageCounter,
    encode: StageCounter,
    authenticated_input: StageCounter,
    model_forward: StageCounter,
    sample: StageCounter,
    apply: StageCounter,
}

#[derive(Clone, Copy)]
enum PairPhase {
    Training,
    InitialEvaluation,
    TrainedEvaluation,
}

impl PairPhase {
    fn name(self) -> &'static str {
        match self {
            Self::Training => "training_pair",
            Self::InitialEvaluation => "initial_evaluation_pair",
            Self::TrainedEvaluation => "trained_evaluation_pair",
        }
    }
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

fn reset_game(
    session: Session,
    roster: [(&str, &[u8]); 2],
    seed: u64,
    start: PlayerId,
) -> Result<KaitoSession> {
    let resolve = |(name, bytes): (&str, &[u8])| -> Result<Vec<u16>> {
        let deck: DeckFileV1 = serde_json::from_slice(bytes)
            .map_err(|error| format!("configuration {name}: {error}"))?;
        let cards = match session {
            Session::Return => deck.resolve(),
            Session::FullPool => deck.resolve_for(SessionKindV1::FullPoolV1),
        }
        .map_err(|error| format!("configuration {name}: {error}"))?;
        if cards.len() != 40 || deck.name.as_deref() != Some(name) {
            return Err(format!(
                "configuration {name}: expected the original 40-card deck"
            ));
        }
        Ok(cards)
    };
    session
        .reset(&ResetV1 {
            expected_card_db_hash: KERNEL_CARDDB_HASH,
            episode_id: seed,
            env_seed: seed,
            starting_player: start.into(),
            deck_ids: roster.map(|(name, _)| name.into()),
            mainboards: [resolve(roster[0])?, resolve(roster[1])?],
            max_physical_decisions: MAX_STEPS,
            max_policy_steps: MAX_STEPS,
        })
        .map_err(|error| format!("session: {error}"))
}

fn game(
    session: Session,
    roster: [(&str, &[u8]); 2],
    seed: u64,
    leg: usize,
    model: &ModelV1,
    phase: PairPhase,
    diagnostics: Option<&Diagnostics>,
) -> Game {
    let collect = matches!(phase, PairPhase::Training);
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
    let mut stages = diagnostics.map(|_| GameStages::default());
    let record = |checkpoint, action_rows, stages: &Option<GameStages>| {
        if let Some((diagnostics, stages)) = diagnostics.zip(stages.as_ref()) {
            diagnostics.record(json!({
                "phase": phase.name(),
                "leg": leg,
                "checkpoint": checkpoint,
                "action_rows": action_rows,
                "stages": stages,
            }));
        }
    };
    record("started", 0, &stages);
    let attempt = (|| -> Result<[i8; 2]> {
        let mut session = measure_stage(stages.as_mut().map(|stage| &mut stage.reset), || {
            reset_game(session, roster, seed, start)
        })?;
        record("reset_returned", 0, &stages);
        let mut rng = [
            SplitMix64::seed(seed ^ 0x3ac6_352a_54aa_8ba0),
            SplitMix64::seed(seed ^ 0xda7a_2418_d42e_9b1d),
        ];
        let mut retained = [0; 2];
        loop {
            let response = measure_stage(
                stages.as_mut().map(|stage| &mut stage.current_response),
                || session.current_response(),
            );
            let decision = match response {
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
                "seat {actor} step {} {:?} native width {}",
                decision.step, decision.decision_kind, decision.legal_action_count
            );
            let bound = measure_stage(stages.as_mut().map(|stage| &mut stage.encode), || {
                session.encode(decision)
            })
            .map_err(|error| format!("encode {context}: {error}"))?;
            // Combat projection can remove an infeasible native answer.
            // Selection and the receipt both use the sealed public ordinals.
            let legal = u32::try_from(bound.packet().actions().len())
                .map_err(|_| format!("sealed action count exceeds u32: {context}"))?;
            let input = if model_seat.is_none_or(|seat| seat == actor) {
                Some(
                    measure_stage(
                        stages.as_mut().map(|stage| &mut stage.authenticated_input),
                        || session.training_input_v1(&bound),
                    )
                    .map_err(|error| format!("tensor {context}: {error}"))?,
                )
            } else {
                None
            };
            let scores = match &input {
                Some(input) => measure_stage(
                    stages.as_mut().map(|stage| &mut stage.model_forward),
                    || model.forward(input),
                )
                .map_err(|error| format!("inference {context}: {error}"))?,
                None => vec![0.; legal as usize],
            };
            let selected = measure_stage(stages.as_mut().map(|stage| &mut stage.sample), || {
                sample(&scores, &mut rng[actor])
            })
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
                legal,
                selected,
                retained_for_update: retain,
            });
            measure_stage(stages.as_mut().map(|stage| &mut stage.apply), || {
                session.apply(&bound, selected as u32)
            })
            .map_err(|error| format!("apply {context}: {error}"))?;
            if stages.is_some() && receipt.actions.len().is_multiple_of(DIAGNOSTIC_INTERVAL) {
                record("progress", receipt.actions.len(), &stages);
            }
        }
    })();
    record("returned", receipt.actions.len(), &stages);
    match attempt {
        Ok(reward) => receipt.reward = Some(reward),
        Err(error) => receipt.failure = Some(error),
    }
    Game { receipt, captured }
}

fn pair(
    session: Session,
    roster: [(&str, &[u8]); 2],
    seed: u64,
    model: &ModelV1,
    phase: PairPhase,
    workers: usize,
    diagnostics: Option<&Diagnostics>,
) -> [Game; 2] {
    measure_phase(diagnostics, phase.name(), || {
        let one = |leg| game(session, roster, seed, leg, model, phase, diagnostics);
        if workers == 1 {
            [one(0), one(1)]
        } else {
            std::thread::scope(|scope| {
                let first = scope.spawn(|| one(0));
                let second = one(1);
                [first.join().expect("first leg worker panicked"), second]
            })
        }
    })
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

fn train(
    session: Session,
    root: &Path,
    workers: usize,
    diagnostics: Option<&Diagnostics>,
) -> Result<Training> {
    let mut learner = LearnerV1::new(ModelV1::deterministic_fixture(), adam())
        .map_err(|error| error.to_string())?;
    let initial = learner.snapshot();
    let games = pair(
        session,
        TRAIN,
        TRAIN_SEED,
        learner.model(),
        PairPhase::Training,
        workers,
        diagnostics,
    );
    if games.iter().any(|game| game.receipt.reward.is_none()) {
        measure_phase(diagnostics, "failed_training_publication", || {
            write_atomic(
                root,
                "failed-training.json",
                &json!({
                    "config": config(session), "initial": initial, "games": games.map(|game| game.receipt),
                }),
            )
        })?;
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
    let updated = measure_phase(diagnostics, "terminal_update", || {
        learner.update_terminal(&batch)
    })
    .map_err(|error| error.to_string())
    .and_then(|loss| {
        if learner.model().parameters() == initial_parameters.as_slice() {
            return Err("terminal batch did not change any parameter".into());
        }
        Ok(loss)
    });
    match updated {
        Ok(loss) => Ok(Training {
            config: config(session),
            initial,
            trained: learner.snapshot(),
            loss_bits: loss.to_bits(),
            games: games.map(|game| game.receipt),
        }),
        Err(error) => {
            measure_phase(diagnostics, "failed_training_publication", || {
                write_atomic(
                    root,
                    "failed-training.json",
                    &json!({
                        "config": config(session), "initial": initial, "games": games.map(|game| game.receipt), "update_error": error.to_string(),
                    }),
                )
            })?;
            Err(format!("training update: {error}"))
        }
    }
}

fn load_training(
    session: Session,
    path: &Path,
    expected: Option<&str>,
) -> Result<(Training, String)> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let digest = hash(&bytes);
    if expected.is_some_and(|expected| expected != digest) {
        return Err("training artifact SHA-256 does not match the resume pin".into());
    }
    let training: Training = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    validate_config(session, &training.config)?;
    if training.games.iter().any(|game| game.reward.is_none()) {
        return Err("training artifact has another configuration or incomplete games".into());
    }
    Ok((training, digest))
}

fn validate_config(session: Session, saved: &Value) -> Result<()> {
    if saved.get("actor_profile").and_then(Value::as_str) != Some(session.profile()) {
        return Err("training artifact has another actor profile".into());
    }
    if saved != &config(session) {
        return Err("training artifact has another configuration".into());
    }
    Ok(())
}

fn run(
    session: Session,
    root: &Path,
    workers: usize,
    resume: Option<(PathBuf, String)>,
    stage_diagnostics: bool,
) -> Result<()> {
    let local = root.join("training.json");
    let resumed = resume
        .as_ref()
        .map(|(path, digest)| load_training(session, path, Some(digest)))
        .transpose()?;
    fs::create_dir(root).map_err(|error| format!("{}: {error}", root.display()))?;
    let diagnostics = stage_diagnostics
        .then(|| Diagnostics::create(root))
        .transpose()?;
    let diagnostics = diagnostics.as_ref();
    let (training, parent_sha256) = if let Some(training) = resumed {
        training
    } else {
        let training = train(session, root, workers, diagnostics)?;
        measure_phase(diagnostics, "training_publication", || {
            write_atomic(root, "training.json", &training)
        })?;
        measure_phase(diagnostics, "training_reload", || {
            load_training(session, &local, None)
        })?
    };
    let (initial, trained) = measure_phase(diagnostics, "evaluation_restore", || {
        let initial = LearnerV1::restore(training.initial).map_err(|error| error.to_string())?;
        let trained = LearnerV1::restore(training.trained).map_err(|error| error.to_string())?;
        Ok::<_, String>((initial, trained))
    })?;
    let eval = |learner: &LearnerV1, phase| {
        pair(
            session,
            EVAL,
            EVAL_SEED,
            learner.model(),
            phase,
            workers,
            diagnostics,
        )
        .map(|g| g.receipt)
    };
    let initial_eval = eval(&initial, PairPhase::InitialEvaluation);
    let trained_eval = eval(&trained, PairPhase::TrainedEvaluation);
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
    let result = json!({"config": config(session), "training_sha256": parent_sha256,
        "initial_wdl": tally(&initial_eval), "trained_wdl": tally(&trained_eval),
        "complete": complete, "initial": initial_eval, "trained": trained_eval});
    let result_hash = measure_phase(diagnostics, "evaluation_publication", || {
        write_atomic(root, "evaluation.json", &result)
    })?;
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
    let (stage_diagnostics, args) = match args.as_slice() {
        [flag, tail @ ..] if flag == "--stage-diagnostics" => (true, tail),
        args => (false, args),
    };
    let parse = || -> Result<(Session, PathBuf, usize, Option<(PathBuf, String)>)> {
        let usage = "kernel_limited_training_v1 [--stage-diagnostics] [-session return|full-pool] OUT_DIR 1|2 [TRAINING_JSON SHA256]";
        let (session, positional) = match args {
            [flag, value, tail @ ..] if flag == "-session" => {
                let session = match value.to_str() {
                    Some("return") => Session::Return,
                    Some("full-pool") => Session::FullPool,
                    _ => return Err(usage.into()),
                };
                (session, tail)
            }
            positional => (Session::Return, positional),
        };
        let [out, workers, tail @ ..] = positional else {
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
        Ok((session, PathBuf::from(out), workers, resume))
    };
    if let Err(error) = parse().and_then(|(session, root, workers, resume)| {
        run(session, &root, workers, resume, stage_diagnostics)
    }) {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(all(test, not(feature = "standard-magezero-fixtures")))]
mod tests {
    use super::*;

    #[test]
    fn stage_diagnostics_preserve_failed_calls_and_publish_completed_lines() {
        let root = std::env::temp_dir().join(format!(
            "limited-training-stage-diagnostics-{}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        let diagnostics = Diagnostics::create(&root).unwrap();
        let mut stages = GameStages::default();
        let result = measure_phase(Some(&diagnostics), "test_phase", || {
            measure_stage(Some(&mut stages.reset), || Err::<(), _>("test failure"))
        });
        assert_eq!(result, Err("test failure"));
        assert_eq!(stages.reset.count, 1);
        assert_eq!(stages.reset.total_ns, stages.reset.max_ns);
        assert_eq!(stages.apply.count, 0);

        let path = root.join("stage-diagnostics.jsonl");
        let contents = fs::read_to_string(&path).unwrap();
        let rows: Vec<Value> = contents
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        drop(diagnostics);
        fs::remove_file(path).unwrap();
        fs::remove_dir(root).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["event"]["checkpoint"], "started");
        assert_eq!(rows[1]["event"]["checkpoint"], "returned");
        assert!(rows[1]["event"]["duration_ns"].as_u64().is_some());
    }

    #[test]
    fn full_pool_original_draftzero_pairs_reach_the_sealed_learner() {
        assert!(reset_game(Session::Return, TRAIN, TRAIN_SEED, PlayerId::P0).is_err());
        let model = ModelV1::deterministic_fixture();
        for (roster, seed) in [(TRAIN, TRAIN_SEED), (EVAL, EVAL_SEED)] {
            for start in [PlayerId::P0, PlayerId::P1] {
                let mut owner = reset_game(Session::FullPool, roster, seed, start).unwrap();
                let other_owner = reset_game(Session::FullPool, roster, seed, start).unwrap();
                let FastActorResponseV1::Decision(decision) = owner.current_response() else {
                    panic!("the original DraftZero pair has no opening decision");
                };
                let bound = owner.encode(decision).unwrap();
                assert!(other_owner.training_input_v1(&bound).is_err());
                let input = owner.training_input_v1(&bound).unwrap();
                let scores = model.forward(&input).unwrap();
                assert_eq!(scores.len(), bound.packet().actions().len());
                assert!(scores.iter().all(|score| score.is_finite()));
                owner.apply(&bound, 0).unwrap();
            }
        }
    }

    #[test]
    fn resume_config_requires_the_exact_source_and_actor_profile() {
        let mut saved = config(Session::FullPool);
        assert!(validate_config(Session::FullPool, &saved).is_ok());
        assert_eq!(
            validate_config(Session::Return, &saved).unwrap_err(),
            "training artifact has another actor profile",
        );
        saved["binary_source_sha256"] = json!("00".repeat(32));
        assert_eq!(
            validate_config(Session::FullPool, &saved).unwrap_err(),
            "training artifact has another configuration",
        );
    }
}
