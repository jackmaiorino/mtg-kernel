//! The cube B-run halt census (declarations A2 and B). Measurement only: no
//! training, no model; uniform-random play over the legal menu.
//!
//! Modes (`eternal-fixtures` build):
//! - `drafted`: drafts `[--draft-start, --draft-end)` of `--namespace` from
//!   the fenced pool (`--cube`, `--gaps`, `--fence`); each draft plays its
//!   28-game round robin ([`mtg_kernel::cube_b_census_v1`]).
//! - `dealt`: the episodes `[--episode-start, --episode-end)` (default: all)
//!   of a `tools/cube_v1/deal.py` schedule (`--schedule`), each played once
//!   in schedule order.
//!
//! Writes `--out` (one JSONL line per game, in logical order) and `--record`
//! (the run record: binary commit, card database hash, input hashes, caps,
//! namespace and range). Both are created fresh, only when the run
//! completes. The caps default to the A2.1 caps (driver decisions 16,384,
//! physical decisions 16,384, policy steps 32,768, stack objects 128).
//! Use `--max-stack-objects N` to set a stack cap, or
//! `--max-stack-objects none` to run uncapped. Caps outside A2.1 require
//! `--allow-non-a2-caps true`. The record includes the caps and A2 status.
//!
//! The games run in `--workers` child processes (default 1) over contiguous
//! index ranges, each game on a thread with a 256 MiB stack, and the parts
//! are concatenated in index order, so the output bytes do not depend on the
//! worker count or the host. When a worker process dies (a stack overflow,
//! for example), the game it was playing is written as a `process_died`
//! line and the worker restarts after it. A worker that makes no progress
//! for `--watchdog-seconds` (default 600) is a hang (A2.1): the run stops
//! with an error and writes no output; the batch is void.
mtg_kernel::fast_global_allocator!();

use mtg_kernel::card_def::{CARD_DEFS, KERNEL_CARDDB_HASH};
use mtg_kernel::cube_b_census_v1::{
    play_drafted_range, DealtSchedule, DraftedPod, GameLine, ProcessDeath, A2_CAPS,
    GAMES_PER_DRAFT, RUN_RECORD_SCHEMA,
};
use mtg_kernel::cube_draft_v1::{basic_land_ids, game_decision_counter, CubePool, GameCaps};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

const WORKER_STACK_BYTES: usize = 256 << 20;
const HEARTBEAT_SECONDS: u64 = 5;
const POLL: Duration = Duration::from_millis(500);

type Fallible<T> = Result<T, String>;

const VALUE_FLAGS: &[&str] = &[
    "--mode",
    "--namespace",
    "--cube",
    "--gaps",
    "--fence",
    "--draft-start",
    "--draft-end",
    "--schedule",
    "--episode-start",
    "--episode-end",
    "--out",
    "--record",
    "--workers",
    "--watchdog-seconds",
    "--max-driver-decisions",
    "--max-physical-decisions",
    "--max-policy-steps",
    "--max-stack-objects",
    "--allow-non-a2-caps",
    // Set by the supervisor for its worker processes only.
    "--worker-start",
    "--worker-end",
    "--worker-out",
    "--worker-progress",
];

struct Args {
    values: BTreeMap<String, String>,
    /// The arguments as given, without the worker flags.
    passthrough: Vec<String>,
}

impl Args {
    fn parse(raw: Vec<String>) -> Fallible<Self> {
        let mut values = BTreeMap::new();
        let mut passthrough = Vec::new();
        let mut items = raw.into_iter();
        while let Some(name) = items.next() {
            if !VALUE_FLAGS.contains(&name.as_str()) {
                return Err(format!("unknown argument {name:?}"));
            }
            let value = items
                .next()
                .ok_or_else(|| format!("{name} needs a value"))?;
            if !name.starts_with("--worker-") {
                passthrough.push(name.clone());
                passthrough.push(value.clone());
            }
            if values.insert(name.clone(), value).is_some() {
                return Err(format!("{name} given twice"));
            }
        }
        Ok(Args {
            values,
            passthrough,
        })
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    fn required(&self, name: &str) -> Fallible<&str> {
        self.get(name).ok_or_else(|| format!("{name} is required"))
    }

    fn number(&self, name: &str) -> Fallible<Option<u64>> {
        self.get(name)
            .map(|text| {
                text.parse::<u64>()
                    .map_err(|error| format!("{name} {text:?}: {error}"))
            })
            .transpose()
    }

    fn forbid(&self, names: &[&str], mode: &str) -> Fallible<()> {
        match names.iter().find(|name| self.values.contains_key(**name)) {
            Some(name) => Err(format!("{name} does not apply to --mode {mode}")),
            None => Ok(()),
        }
    }
}

fn read_bytes(path: &str) -> Fallible<Vec<u8>> {
    std::fs::read(path).map_err(|error| format!("{path}: {error}"))
}

fn utf8(path: &str, bytes: &[u8]) -> Fallible<String> {
    String::from_utf8(bytes.to_vec()).map_err(|error| format!("{path}: {error}"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn card_names(ids: impl IntoIterator<Item = u16>) -> Value {
    let unique: BTreeSet<u16> = ids.into_iter().collect();
    let names: serde_json::Map<String, Value> = unique
        .into_iter()
        .map(|id| (id.to_string(), json!(CARD_DEFS[usize::from(id)].name)))
        .collect();
    Value::Object(names)
}

/// What a run plays: the inputs, and how to play or bury one logical game.
enum Source {
    Drafted { pool: CubePool, namespace: String },
    Dealt { schedule: DealtSchedule },
}

struct Run {
    source: Source,
    caps: GameCaps,
    /// Logical index range of the run.
    start: u64,
    end: u64,
    /// Logical indices per unit a worker range is aligned to (a draft's 28
    /// games, or one episode).
    unit: u64,
    record: Value,
}

fn caps_from(args: &Args) -> Fallible<(GameCaps, bool)> {
    let caps = GameCaps {
        max_decisions: args
            .number("--max-driver-decisions")?
            .unwrap_or(A2_CAPS.max_decisions),
        max_physical_decisions: args
            .number("--max-physical-decisions")?
            .unwrap_or(A2_CAPS.max_physical_decisions),
        max_policy_steps: args
            .number("--max-policy-steps")?
            .unwrap_or(A2_CAPS.max_policy_steps),
        max_stack_objects: match args.get("--max-stack-objects") {
            None => A2_CAPS.max_stack_objects,
            Some("none") => None,
            Some(text) => Some(
                text.parse::<u32>()
                    .map_err(|error| format!("--max-stack-objects {text:?}: {error}"))?,
            ),
        },
    };
    let allow = match args.get("--allow-non-a2-caps") {
        None | Some("false") => false,
        Some("true") => true,
        Some(other) => return Err(format!("--allow-non-a2-caps {other:?}: true or false")),
    };
    let a2 = caps == A2_CAPS;
    if !a2 && !allow {
        return Err(format!(
            "caps {caps:?} are not the A2.1 caps {A2_CAPS:?}; pass --allow-non-a2-caps true for a non-census run"
        ));
    }
    Ok((caps, a2))
}

fn load_run(args: &Args) -> Fallible<Run> {
    let mode = args.required("--mode")?;
    let (caps, a2) = caps_from(args)?;
    let caps_record = json!({
        "max_driver_decisions": caps.max_decisions,
        "max_physical_decisions": caps.max_physical_decisions,
        "max_policy_steps": caps.max_policy_steps,
        "max_stack_objects": caps.max_stack_objects,
        "a2_caps": a2,
    });
    let card_db_hash = format!("{KERNEL_CARDDB_HASH:016x}");
    let binary = json!({
        "git_commit": env!("MTG_KERNEL_BUILD_GIT_HEAD"),
        "git_tree": env!("MTG_KERNEL_BUILD_GIT_TREE"),
        "git_clean": env!("MTG_KERNEL_BUILD_GIT_CLEAN"),
    });
    match mode {
        "drafted" => {
            args.forbid(&["--schedule", "--episode-start", "--episode-end"], mode)?;
            let namespace = args.required("--namespace")?.to_owned();
            let draft_start = args
                .number("--draft-start")?
                .ok_or("--draft-start is required")?;
            let draft_end = args
                .number("--draft-end")?
                .ok_or("--draft-end is required")?;
            if draft_end <= draft_start {
                return Err("--draft-end must be greater than --draft-start".to_owned());
            }
            let start = draft_start
                .checked_mul(GAMES_PER_DRAFT)
                .ok_or("--draft-start is too large")?;
            let end = draft_end
                .checked_mul(GAMES_PER_DRAFT)
                .ok_or("--draft-end is too large")?;
            let (cube_path, gaps_path, fence_path) = (
                args.required("--cube")?,
                args.required("--gaps")?,
                args.required("--fence")?,
            );
            let (cube, gaps, fence) = (
                read_bytes(cube_path)?,
                read_bytes(gaps_path)?,
                read_bytes(fence_path)?,
            );
            let pool = CubePool::from_json_texts(
                &utf8(cube_path, &cube)?,
                &utf8(gaps_path, &gaps)?,
                Some(&utf8(fence_path, &fence)?),
            )
            .map_err(|error| error.to_string())?;
            let basics = basic_land_ids().map_err(|error| error.to_string())?;
            let record = json!({
                "schema": RUN_RECORD_SCHEMA,
                "liveness": "per-decision-v1",
                "mode": mode,
                "binary": binary,
                "card_db_hash": card_db_hash,
                "caps": caps_record,
                "namespace": namespace,
                "draft_start": draft_start,
                "draft_end": draft_end,
                "index_start": start,
                "index_end": end,
                "games": end - start,
                "cube_list_sha256": sha256_hex(&cube),
                "gap_record_sha256": sha256_hex(&gaps),
                "fence_sha256": sha256_hex(&fence),
                "pool_cards": pool.card_count(),
                "worker_stack_bytes": WORKER_STACK_BYTES,
                "card_names": card_names(pool.ids().iter().copied().chain(basics)),
            });
            Ok(Run {
                source: Source::Drafted { pool, namespace },
                caps,
                start,
                end,
                unit: GAMES_PER_DRAFT,
                record,
            })
        }
        "dealt" => {
            args.forbid(
                &[
                    "--namespace",
                    "--cube",
                    "--gaps",
                    "--fence",
                    "--draft-start",
                    "--draft-end",
                ],
                mode,
            )?;
            let path = args.required("--schedule")?;
            let bytes = read_bytes(path)?;
            let schedule = mtg_kernel::cube_b_census_v1::parse_dealt_schedule(
                &utf8(path, &bytes)?,
                &card_db_hash,
                &caps,
            )?;
            let count = schedule.episodes.len() as u64;
            let start = args.number("--episode-start")?.unwrap_or(0);
            let end = args.number("--episode-end")?.unwrap_or(count);
            if end <= start || end > count {
                return Err(format!(
                    "episode range [{start}, {end}) is empty or outside the schedule's {count} episodes"
                ));
            }
            let ids = schedule.episodes[start as usize..end as usize]
                .iter()
                .flat_map(|episode| episode.decks.iter())
                .flat_map(|deck| deck.card_ids());
            let record = json!({
                "schema": RUN_RECORD_SCHEMA,
                "liveness": "per-decision-v1",
                "mode": mode,
                "binary": binary,
                "card_db_hash": card_db_hash,
                "caps": caps_record,
                "namespace": schedule.namespace,
                "schedule_sha256": sha256_hex(&bytes),
                "schedule": {
                    "schema": schedule.schema,
                    "namespace": schedule.namespace,
                    "run": schedule.run,
                    "kernel_carddb_hash": schedule.kernel_carddb_hash,
                    "pool_sha256": schedule.pool_sha256,
                    "constants_sha256": schedule.constants_sha256,
                    "episodes": count,
                },
                "episode_start": start,
                "episode_end": end,
                "index_start": start,
                "index_end": end,
                "games": end - start,
                "worker_stack_bytes": WORKER_STACK_BYTES,
                "card_names": card_names(ids),
            });
            Ok(Run {
                source: Source::Dealt { schedule },
                caps,
                start,
                end,
                unit: 1,
                record,
            })
        }
        other => Err(format!("unknown --mode {other:?}: drafted or dealt")),
    }
}

impl Run {
    fn record_text(&self) -> String {
        format!(
            "{}\n",
            serde_json::to_string_pretty(&self.record).expect("a run record is plain data")
        )
    }

    /// Plays `[start, end)` in order, writing each line before the next game
    /// starts and naming the game in play in `progress` first.
    fn play(
        &self,
        start: u64,
        end: u64,
        out: &mut impl Write,
        progress: &Path,
        current: &WorkerProgress,
    ) -> Fallible<()> {
        let mut emit = |line: &GameLine| -> Fallible<()> {
            let text = format!("{}\n", line.to_json_line());
            out.write_all(text.as_bytes())
                .and_then(|()| out.flush())
                .map_err(|error| format!("writing a line: {error}"))?;
            // The next game, so a death names it.
            current.begin(line.index + 1);
            std::fs::write(progress, (line.index + 1).to_string())
                .map_err(|error| format!("{}: {error}", progress.display()))
        };
        std::fs::write(progress, start.to_string())
            .map_err(|error| format!("{}: {error}", progress.display()))?;
        match &self.source {
            Source::Drafted { pool, namespace } => {
                play_drafted_range(pool, namespace, start, end, &self.caps, |line| {
                    emit(line).map_err(StringError)
                })
                .map_err(|error| error.0)
            }
            Source::Dealt { schedule } => {
                for index in start..end {
                    emit(&schedule.episodes[index as usize].play(index, &self.caps))?;
                }
                Ok(())
            }
        }
    }

    fn death_line(&self, index: u64, death: ProcessDeath) -> Fallible<GameLine> {
        match &self.source {
            Source::Drafted { pool, namespace } => {
                let pod = DraftedPod::prepare(pool, namespace, index / GAMES_PER_DRAFT)
                    .map_err(|error| error.to_string())?;
                pod.death((index % GAMES_PER_DRAFT) as usize, death)
                    .map_err(|error| error.to_string())
            }
            Source::Dealt { schedule } => Ok(schedule.episodes[index as usize].death(index, death)),
        }
    }
}

/// A `String` error that a [`mtg_kernel::cube_draft_v1::CubeDraftError`]
/// converts into, for [`play_drafted_range`].
struct StringError(String);

impl From<mtg_kernel::cube_draft_v1::CubeDraftError> for StringError {
    fn from(error: mtg_kernel::cube_draft_v1::CubeDraftError) -> Self {
        StringError(error.to_string())
    }
}

// ---------------------------------------------------------------------------
// Worker
// ---------------------------------------------------------------------------

struct WorkerProgress {
    game: Mutex<u64>,
    decisions: Arc<AtomicU64>,
}

impl WorkerProgress {
    fn new(start: u64) -> Self {
        let decisions = game_decision_counter();
        decisions.store(0, Ordering::Relaxed);
        Self {
            game: Mutex::new(start),
            decisions,
        }
    }

    // A sampler sees the index and count on the same side of a game boundary.
    fn begin(&self, index: u64) {
        let mut game = self.game.lock().unwrap();
        self.decisions.store(0, Ordering::Relaxed);
        *game = index;
    }

    fn sample(&self) -> (u64, u64) {
        let game = self.game.lock().unwrap();
        (*game, self.decisions.load(Ordering::Relaxed))
    }
}

fn with_heartbeat(
    progress: &Path,
    current: &WorkerProgress,
    interval: Duration,
    play: impl FnOnce() -> Fallible<()>,
) -> Fallible<()> {
    let live = progress.with_added_extension("live");
    let temp = live.with_added_extension("tmp");
    std::thread::scope(|scope| {
        let (stop, wait) = mpsc::channel::<()>();
        let heartbeat = scope.spawn(move || {
            let mut last = None;
            while wait.recv_timeout(interval) == Err(mpsc::RecvTimeoutError::Timeout) {
                let (game, decisions) = current.sample();
                if decisions == 0 || last == Some((game, decisions)) {
                    continue;
                }
                if let Err(error) = std::fs::write(&temp, format!("{game} {decisions}"))
                    .and_then(|()| std::fs::rename(&temp, &live))
                {
                    let _ = std::fs::remove_file(&temp);
                    let _ = writeln!(std::io::stderr(), "{}: {error}", live.display());
                    continue;
                }
                last = Some((game, decisions));
            }
            Ok(())
        });
        let result = play();
        drop(stop);
        result.and(heartbeat.join().expect("heartbeat thread panicked"))
    })
}

fn worker(args: &Args) -> Fallible<()> {
    let run = load_run(args)?;
    let start = args
        .number("--worker-start")?
        .ok_or("--worker-start is required")?;
    let end = args
        .number("--worker-end")?
        .ok_or("--worker-end is required")?;
    if start >= end || start < run.start || end > run.end {
        return Err(format!(
            "worker range [{start}, {end}) is outside [{}, {})",
            run.start, run.end
        ));
    }
    let out_path = args.required("--worker-out")?;
    let progress = PathBuf::from(args.required("--worker-progress")?);
    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(out_path)
        .map_err(|error| format!("{out_path}: {error}"))?;
    // A panic is a recorded game end, not a message.
    std::panic::set_hook(Box::new(|_| {}));
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("cube-b-census".to_owned())
            .stack_size(WORKER_STACK_BYTES)
            .spawn_scoped(scope, || {
                let current = WorkerProgress::new(start);
                with_heartbeat(
                    &progress,
                    &current,
                    Duration::from_secs(HEARTBEAT_SECONDS),
                    || run.play(start, end, &mut out, &progress, &current),
                )
            })
            .map_err(|error| format!("spawning the game thread: {error}"))?
            .join()
            .map_err(|_| "the game thread panicked outside a game".to_owned())?
    })
}

// ---------------------------------------------------------------------------
// Supervisor
// ---------------------------------------------------------------------------

struct Part {
    start: u64,
    end: u64,
    /// The first index the current worker process has not yet written.
    next: u64,
    out: PathBuf,
    progress: PathBuf,
    child: Option<Child>,
    /// What last showed progress, and when.
    seen: PartProgress,
    seen_at: Instant,
}

fn death_of(status: ExitStatus) -> ProcessDeath {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return ProcessDeath::Signal(signal);
        }
    }
    ProcessDeath::Exit(status.code().map_or(u32::MAX, |code| code as u32))
}

/// Complete lines in `path`; a partial last line (a worker that died while
/// writing) is cut off.
fn complete_lines(path: &Path) -> Fallible<u64> {
    let mut bytes = Vec::new();
    match File::open(path) {
        Ok(mut file) => {
            file.read_to_end(&mut bytes)
                .map_err(|error| format!("{}: {error}", path.display()))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    }
    let kept = bytes
        .iter()
        .rposition(|&byte| byte == b'\n')
        .map_or(0, |at| at + 1);
    if kept < bytes.len() {
        OpenOptions::new()
            .write(true)
            .open(path)
            .and_then(|file| file.set_len(kept as u64))
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(bytes[..kept].iter().filter(|&&byte| byte == b'\n').count() as u64)
}

fn spawn(args: &Args, part: &Part) -> Fallible<Child> {
    let exe = std::env::current_exe().map_err(|error| format!("current_exe: {error}"))?;
    Command::new(exe)
        .args(&args.passthrough)
        .arg("--worker-start")
        .arg(part.next.to_string())
        .arg("--worker-end")
        .arg(part.end.to_string())
        .arg("--worker-out")
        .arg(&part.out)
        .arg("--worker-progress")
        .arg(&part.progress)
        .spawn()
        .map_err(|error| format!("spawning a worker: {error}"))
}

#[derive(Default, PartialEq, Eq)]
struct PartProgress {
    size: u64,
    game: String,
    live: String,
}

fn part_progress(part: &Part) -> PartProgress {
    PartProgress {
        size: std::fs::metadata(&part.out).map_or(0, |meta| meta.len()),
        game: std::fs::read_to_string(&part.progress).unwrap_or_default(),
        live: std::fs::read_to_string(part.progress.with_added_extension("live"))
            .unwrap_or_default(),
    }
}

/// Returns the time of the last change, or None if the worker is stuck.
fn check_watchdog(
    previous: &PartProgress,
    current: &PartProgress,
    seen_at: Instant,
    now: Instant,
    watchdog: Duration,
) -> Option<Instant> {
    if current != previous {
        Some(now)
    } else if now.duration_since(seen_at) <= watchdog {
        Some(seen_at)
    } else {
        None
    }
}

fn hang_message(k: usize, next: u64, current: &PartProgress, watchdog: Duration) -> String {
    let game = current.game.trim().parse().unwrap_or(next).max(next);
    let decision = current
        .live
        .split_once(' ')
        .filter(|(index, _)| index.parse::<u64>() == Ok(game))
        .and_then(|(_, count)| count.parse::<u64>().ok())
        .unwrap_or(0);
    format!(
        "hang: worker {k} made no progress for {}s at game {game} (last decision {decision}); the batch is void (A2.1), no output written",
        watchdog.as_secs()
    )
}

fn kill_all(parts: &mut [Part]) {
    for part in parts.iter_mut() {
        if let Some(child) = part.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn supervise(args: &Args) -> Fallible<()> {
    let run = load_run(args)?;
    let out = PathBuf::from(args.required("--out")?);
    let record = PathBuf::from(args.required("--record")?);
    for path in [&out, &record] {
        if path.exists() {
            return Err(format!(
                "{} exists; outputs are created fresh",
                path.display()
            ));
        }
    }
    let workers = args.number("--workers")?.unwrap_or(1).max(1);
    let watchdog = Duration::from_secs(args.number("--watchdog-seconds")?.unwrap_or(600));
    let units = (run.end - run.start) / run.unit;
    let workers = workers.min(units);
    let started = Instant::now();
    let mut parts: Vec<Part> = (0..workers)
        .map(|k| {
            let start = run.start + run.unit * (units * k / workers);
            let end = run.start + run.unit * (units * (k + 1) / workers);
            let stem = out.as_os_str().to_owned();
            let with = |suffix: &str| {
                let mut path = stem.clone();
                path.push(format!(".w{k:03}.{suffix}"));
                PathBuf::from(path)
            };
            Part {
                start,
                end,
                next: start,
                out: with("part"),
                progress: with("progress"),
                child: None,
                seen: PartProgress::default(),
                seen_at: Instant::now(),
            }
        })
        .collect();
    for part in &parts {
        let live = part.progress.with_added_extension("live");
        for path in [
            &part.out,
            &part.progress,
            &live,
            &live.with_added_extension("tmp"),
        ] {
            if path.exists() {
                return Err(format!("{} exists from an earlier run", path.display()));
            }
        }
    }
    for part in parts.iter_mut() {
        part.child = Some(spawn(args, part)?);
        part.seen_at = Instant::now();
    }
    let mut deaths = 0u64;
    while parts.iter().any(|part| part.child.is_some()) {
        std::thread::sleep(POLL);
        for k in 0..parts.len() {
            let Some(child) = parts[k].child.as_mut() else {
                continue;
            };
            let status = match child.try_wait() {
                Ok(status) => status,
                Err(error) => {
                    kill_all(&mut parts);
                    return Err(format!("waiting for worker {k}: {error}"));
                }
            };
            let part = &mut parts[k];
            let Some(status) = status else {
                let seen = part_progress(part);
                let Some(at) =
                    check_watchdog(&part.seen, &seen, part.seen_at, Instant::now(), watchdog)
                else {
                    let message = hang_message(k, part.next, &seen, watchdog);
                    kill_all(&mut parts);
                    return Err(message);
                };
                part.seen = seen;
                part.seen_at = at;
                continue;
            };
            part.child = None;
            let written = part.start + complete_lines(&part.out)?;
            if status.success() {
                if written != part.end {
                    let message = format!(
                        "worker {k} exited cleanly after {written} of [{}, {})",
                        part.start, part.end
                    );
                    kill_all(&mut parts);
                    return Err(message);
                }
                continue;
            }
            // The worker names the game it starts before playing it; a
            // death anywhere else (start-up, writing) stops the run.
            let named = std::fs::read_to_string(&part.progress).unwrap_or_default();
            if named.trim() != written.to_string() || written >= part.end || written < part.next {
                kill_all(&mut parts);
                return Err(format!(
                    "worker {k} died ({status}) outside a game (progress {named:?}, written up to {written})"
                ));
            }
            let death = death_of(status);
            let line = run.death_line(written, death)?;
            OpenOptions::new()
                .append(true)
                .open(&part.out)
                .and_then(|mut file| {
                    file.write_all(format!("{}\n", line.to_json_line()).as_bytes())
                })
                .map_err(|error| format!("{}: {error}", part.out.display()))?;
            deaths += 1;
            eprintln!(
                "{}",
                json!({"event": "process_died", "index": written, "stop_kind": death.stop_kind()})
            );
            part.next = written + 1;
            if part.next < part.end {
                let live = part.progress.with_added_extension("live");
                let temp = live.with_added_extension("tmp");
                for path in [part.progress.clone(), live, temp] {
                    match std::fs::remove_file(&path) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => {
                            kill_all(&mut parts);
                            return Err(format!("{}: {error}", path.display()));
                        }
                    }
                }
                part.seen = part_progress(part);
                part.child = Some(spawn(args, part)?);
                part.seen_at = Instant::now();
            }
        }
    }
    let mut output =
        File::create_new(&out).map_err(|error| format!("{}: {error}", out.display()))?;
    let mut games = 0u64;
    for part in &parts {
        let file =
            File::open(&part.out).map_err(|error| format!("{}: {error}", part.out.display()))?;
        for line in BufReader::new(file).lines() {
            let line = line.map_err(|error| format!("{}: {error}", part.out.display()))?;
            let index = serde_json::from_str::<Value>(&line)
                .ok()
                .and_then(|row| row["index"].as_u64());
            if index != Some(run.start + games) {
                return Err(format!(
                    "{}: line {games} has index {index:?}, expected {}",
                    part.out.display(),
                    run.start + games
                ));
            }
            output
                .write_all(format!("{line}\n").as_bytes())
                .map_err(|error| format!("{}: {error}", out.display()))?;
            games += 1;
        }
    }
    if games != run.end - run.start {
        return Err(format!("{games} lines, expected {}", run.end - run.start));
    }
    output
        .flush()
        .map_err(|error| format!("{}: {error}", out.display()))?;
    std::fs::write(&record, run.record_text())
        .map_err(|error| format!("{}: {error}", record.display()))?;
    for part in &parts {
        let _ = std::fs::remove_file(&part.out);
        let _ = std::fs::remove_file(&part.progress);
        let live = part.progress.with_added_extension("live");
        let _ = std::fs::remove_file(live.with_added_extension("tmp"));
        let _ = std::fs::remove_file(live);
    }
    let seconds = started.elapsed().as_secs_f64();
    eprintln!(
        "{}",
        json!({"games": games, "process_deaths": deaths, "workers": workers,
            "wall_seconds": seconds,
            "games_per_core_hour": games as f64 / (seconds * workers as f64) * 3600.0,
            "out": out.display().to_string(), "record": record.display().to_string()})
    );
    Ok(())
}

fn main() {
    let result = Args::parse(std::env::args().skip(1).collect()).and_then(|args| {
        if args.get("--worker-out").is_none() {
            return supervise(&args);
        }
        let result = worker(&args);
        if let (Err(error), Some(progress)) = (&result, args.get("--worker-progress")) {
            // Not a number, so the supervisor stops the run instead of
            // recording a process death.
            let _ = std::fs::write(progress, format!("error: {error}"));
        }
        result
    });
    if let Err(error) = result {
        eprintln!("cube_b_census_v1: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("cube-census-{name}-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        dir
    }

    struct TestOutput {
        file: File,
        first_heartbeat: Option<PathBuf>,
    }

    impl Write for TestOutput {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.file.write(bytes)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.file.flush()?;
            // Hold the first game's decision count until the sampler publishes it.
            if let Some(live) = self.first_heartbeat.take() {
                let deadline = Instant::now() + Duration::from_secs(2);
                while Instant::now() < deadline
                    && std::fs::read_to_string(&live).ok().as_deref() != Some("0 2")
                {
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert_eq!(std::fs::read_to_string(live)?, "0 2");
            }
            Ok(())
        }
    }

    fn assert_same_outputs(dir: &Path, flags: &[&str], games: usize) {
        let args = Args::parse(
            flags
                .iter()
                .copied()
                .chain(["--max-driver-decisions", "2", "--allow-non-a2-caps", "true"])
                .map(str::to_owned)
                .collect(),
        )
        .unwrap();
        let run = load_run(&args).unwrap();
        let current = WorkerProgress::new(0);
        let line = match &run.source {
            Source::Drafted { pool, namespace } => DraftedPod::prepare(pool, namespace, 0)
                .unwrap()
                .play(0, &run.caps)
                .unwrap(),
            Source::Dealt { schedule } => schedule.episodes[0].play(0, &run.caps),
        };
        assert_eq!(line.driver_decisions, Some(2));
        assert_eq!(current.sample(), (0, 2));

        let play = |heartbeat: bool| {
            let run = load_run(&args).unwrap();
            let out = dir.join(format!("{}-{heartbeat}", args.required("--mode").unwrap()));
            let progress = out.with_added_extension("progress");
            let mut file = TestOutput {
                file: File::create(&out).unwrap(),
                first_heartbeat: heartbeat.then(|| progress.with_added_extension("live")),
            };
            let current = WorkerProgress::new(run.start);
            let mut work = || run.play(run.start, run.end, &mut file, &progress, &current);
            if heartbeat {
                with_heartbeat(&progress, &current, Duration::from_millis(1), work).unwrap();
            } else {
                work().unwrap();
            }
            assert_eq!(
                std::fs::read_to_string(progress).unwrap(),
                run.end.to_string()
            );
            let bytes = std::fs::read(out).unwrap();
            assert_eq!(bytes.iter().filter(|&&b| b == b'\n').count(), games);
            assert_eq!(run.record["liveness"], "per-decision-v1");
            (bytes, run.record_text())
        };
        assert_eq!(play(false), play(true));
    }

    fn write_dealt_schedule(dir: &Path) -> PathBuf {
        let deck = json!({"mainboard": vec![basic_land_ids().unwrap()[0]; 40]});
        let episode = |seed: u64| {
            json!({"episode": {
                "id": format!("game-{seed}"), "seed": seed, "starting_player": 0,
                "selected": [&deck, &deck], "max_physical_decisions": A2_CAPS.max_physical_decisions,
                "max_policy_steps": A2_CAPS.max_policy_steps
            }})
        };
        let schedule = dir.join("schedule.json");
        std::fs::write(
            &schedule,
            json!({
                "kernel_carddb_hash": format!("{KERNEL_CARDDB_HASH:016x}"),
                "iterations": [{"episodes": [episode(11), episode(12)]}]
            })
            .to_string(),
        )
        .unwrap();
        schedule
    }

    #[test]
    fn drafted_and_dealt_stack_caps_are_recorded_and_checked() {
        let dir = temp_dir("stack-cap");
        let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../data/cube_v1");
        let cube = data.join("mtgo_vintage_cube_20261007.json");
        let gaps = data.join("mtgo_vintage_cube_20261007_gaps.json");
        let fence = dir.join("fence.json");
        std::fs::write(&fence, r#"{"cards":[]}"#).unwrap();
        let schedule = write_dealt_schedule(&dir);
        let drafted = [
            "--mode",
            "drafted",
            "--namespace",
            "stack-cap-test",
            "--draft-start",
            "0",
            "--draft-end",
            "1",
            "--cube",
            cube.to_str().unwrap(),
            "--gaps",
            gaps.to_str().unwrap(),
            "--fence",
            fence.to_str().unwrap(),
        ];
        let dealt = ["--mode", "dealt", "--schedule", schedule.to_str().unwrap()];
        for flags in [drafted.as_slice(), dealt.as_slice()] {
            let load = |extra: &[&str]| {
                let raw = flags.iter().chain(extra).map(|flag| (*flag).to_owned());
                load_run(&Args::parse(raw.collect()).unwrap())
            };
            for (extra, limit, a2) in [
                (vec![], Some(128), true),
                (vec!["--max-stack-objects", "128"], Some(128), true),
                (vec!["--max-stack-objects", "7"], Some(7), false),
                (vec!["--max-stack-objects", "0"], Some(0), false),
                (
                    vec!["--max-stack-objects", "4294967295"],
                    Some(u32::MAX),
                    false,
                ),
                (vec!["--max-stack-objects", "none"], None, false),
            ] {
                let mut extra = extra;
                if !a2 {
                    let error = load(&extra).err().expect("non-A2 caps must be refused");
                    assert!(error.contains("--allow-non-a2-caps true"), "{error}");
                    extra.extend(["--allow-non-a2-caps", "true"]);
                }
                let run = load(&extra).unwrap();
                assert_eq!(
                    run.caps,
                    GameCaps {
                        max_stack_objects: limit,
                        ..A2_CAPS
                    }
                );
                let record: Value = serde_json::from_str(&run.record_text()).unwrap();
                assert_eq!(
                    record["caps"],
                    json!({
                        "max_driver_decisions": A2_CAPS.max_decisions,
                        "max_physical_decisions": A2_CAPS.max_physical_decisions,
                        "max_policy_steps": A2_CAPS.max_policy_steps,
                        "max_stack_objects": limit,
                        "a2_caps": a2,
                    })
                );
            }
            let error = load(&[
                "--max-stack-objects",
                "4294967296",
                "--allow-non-a2-caps",
                "true",
            ])
            .err()
            .expect("out-of-range stack cap must be refused");
            assert!(
                error.starts_with(r#"--max-stack-objects "4294967296":"#),
                "{error}"
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn heartbeat_preserves_drafted_and_dealt_output_bytes() {
        std::thread::Builder::new()
            .stack_size(WORKER_STACK_BYTES)
            .spawn(|| {
                let dir = temp_dir("bytes");
                let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../data/cube_v1");
                let cube = data.join("mtgo_vintage_cube_20261007.json");
                let gaps = data.join("mtgo_vintage_cube_20261007_gaps.json");
                let fence = dir.join("fence.json");
                std::fs::write(&fence, r#"{"cards":[]}"#).unwrap();
                assert_same_outputs(
                    &dir,
                    &[
                        "--mode",
                        "drafted",
                        "--namespace",
                        "census-b-test",
                        "--draft-start",
                        "0",
                        "--draft-end",
                        "2",
                        "--cube",
                        cube.to_str().unwrap(),
                        "--gaps",
                        gaps.to_str().unwrap(),
                        "--fence",
                        fence.to_str().unwrap(),
                    ],
                    (2 * GAMES_PER_DRAFT) as usize,
                );

                let schedule = write_dealt_schedule(&dir);
                assert_same_outputs(
                    &dir,
                    &["--mode", "dealt", "--schedule", schedule.to_str().unwrap()],
                    2,
                );
                std::fs::remove_dir_all(dir).unwrap();
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn decision_progress_resets_the_watchdog() {
        let start = Instant::now();
        let limit = Duration::from_secs(2);
        let tick = Duration::from_millis(1);
        let sample = |live: &str| PartProgress {
            size: 17,
            game: "8".into(),
            live: live.into(),
        };
        let first = sample("8 40");
        let second = sample("8 41");
        // Even if the poll is late, an advancing game is alive.
        let late = start + limit + tick;
        assert_eq!(
            check_watchdog(&first, &second, start, late, limit),
            Some(late)
        );
        assert_eq!(check_watchdog(&first, &first, start, late, limit), None);

        let just_before = start + limit - tick;
        let reset = check_watchdog(&first, &second, start, just_before, limit).unwrap();
        assert_eq!(reset, just_before);
        assert_eq!(
            check_watchdog(&second, &second, reset, late, limit),
            Some(reset)
        );
        assert_eq!(
            check_watchdog(&second, &second, reset, reset + limit + tick, limit),
            None
        );
        assert!(hang_message(0, 8, &sample("7 99"), limit).contains("at game 8 (last decision 0)"));
        let stale = PartProgress {
            game: "7".into(),
            ..sample("7 99")
        };
        assert!(hang_message(0, 8, &stale, limit).contains("at game 8 (last decision 0)"));
    }

    #[test]
    fn heartbeat_file_keeps_a_slow_worker_alive_until_its_decisions_stop() {
        let dir = temp_dir("slow");
        let mut part = Part {
            start: 8,
            end: 9,
            next: 8,
            out: dir.join("part"),
            progress: dir.join("progress"),
            child: None,
            seen: PartProgress::default(),
            seen_at: Instant::now(),
        };
        std::fs::write(&part.progress, "8").unwrap();
        part.seen = part_progress(&part);
        let progress = part.progress.clone();
        let current = WorkerProgress::new(8);
        let poll = Duration::from_millis(10);
        let limit = Duration::from_secs(2);
        with_heartbeat(&progress, &current, poll, || {
            for decision in 1..=3 {
                std::thread::sleep(Duration::from_secs(1));
                game_decision_counter().store(decision, Ordering::Relaxed);
                let expected = format!("8 {decision}");
                loop {
                    let seen = part_progress(&part);
                    part.seen_at = check_watchdog(&part.seen, &seen, part.seen_at, Instant::now(), limit)
                        .expect("advancing worker timed out");
                    part.seen = seen;
                    if part.seen.live == expected {
                        break;
                    }
                    std::thread::sleep(poll);
                }
            }
            let live = progress.with_added_extension("live");
            let modified = std::fs::metadata(&live).unwrap().modified().unwrap();
            std::thread::sleep(limit + poll);
            assert_eq!(std::fs::metadata(&live).unwrap().modified().unwrap(), modified);
            let seen = part_progress(&part);
            assert_eq!(check_watchdog(&part.seen, &seen, part.seen_at, Instant::now(), limit), None);
            assert_eq!(hang_message(0, part.next, &seen, limit),
                "hang: worker 0 made no progress for 2s at game 8 (last decision 3); the batch is void (A2.1), no output written");
            Ok(())
        }).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
