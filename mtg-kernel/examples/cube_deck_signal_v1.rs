//! The deck-signal experiment of the cube drafting design note (section 10,
//! parts A and B), under the uniform-legal pilot, as a labelled shakedown.
//! Measurement only: no training, no model.
//!
//! ```text
//! cube_deck_signal_v1 --part A|B --cube CUBE --gaps GAPS --fence FENCE \
//!     --out games.jsonl --record run.json [--workers N]
//! ```
//!
//! Part A plays 400 coherent-against-pile pairs (800 games), part B the 128
//! ring matchups of 32 coherent decks (1,024 games); see
//! [`mtg_kernel::cube_deck_signal_v1`] for the deck rules and the seeds. The
//! pool is the cube list minus the gap record minus the fence. Every game
//! runs under the A2.1 caps (driver decisions 16,384, physical decisions
//! 16,384, policy steps 32,768) and the stack-size cap of 128 objects
//! (A2 amendment 1), passed explicitly.
//!
//! Writes `--out` (one JSONL line per game, in logical order) and `--record`
//! (the run record: binary commit, card database hash, input hashes
//! including the fence's sha256, caps, the seed namespace and tuples, and
//! for part B the ring), both created fresh and only when the run completes.
//!
//! The games run in `--workers` child processes (default 1) over contiguous
//! index ranges, each game on a thread with a 256 MiB stack, and the parts
//! are concatenated in index order, so the output bytes do not depend on the
//! worker count. When a worker process dies (a stack overflow, for example)
//! the game it was playing is written as a `process_died` line and the
//! worker restarts after it. A worker that makes no progress for 600 seconds
//! is a hang: the run stops with an error and writes no output.
mtg_kernel::fast_global_allocator!();

use mtg_kernel::card_def::{CARD_DEFS, KERNEL_CARDDB_HASH};
use mtg_kernel::cube_b_census_v1::{ProcessDeath, A2_CAPS};
use mtg_kernel::cube_deck_signal_v1::{
    seed_tuples, DeckSignalRun, GameLine, Part, BASIC_LANDS, MAX_PAIR_DRAWS, NONBASIC_LANDS,
    NONLAND_CARDS, PART_A_GAMES_PER_PAIR, RUN_RECORD_SCHEMA, SEED_NAMESPACE,
};
use mtg_kernel::cube_draft_v1::{CubeDraftError, CubePool, GameCaps};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::time::{Duration, Instant};

const WORKER_STACK_BYTES: usize = 256 << 20;
const POLL: Duration = Duration::from_millis(500);
const WATCHDOG: Duration = Duration::from_secs(600);

type Fallible<T> = Result<T, String>;

const VALUE_FLAGS: &[&str] = &[
    "--part",
    "--cube",
    "--gaps",
    "--fence",
    "--out",
    "--record",
    "--workers",
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

struct Run {
    run: DeckSignalRun,
    /// Logical indices per unit a worker range is aligned to (a part A pair,
    /// or one part B game).
    unit: u64,
    record: Value,
}

fn load_run(args: &Args) -> Fallible<Run> {
    let part_text = args.required("--part")?;
    let part = Part::parse(part_text).ok_or_else(|| format!("--part {part_text:?}: A or B"))?;
    // The A2.1 caps, passed explicitly; no flag changes them.
    let caps: GameCaps = A2_CAPS;
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
    let run = DeckSignalRun::new(part, &pool, caps).map_err(|error| error.to_string())?;
    let mut record = json!({
        "schema": RUN_RECORD_SCHEMA,
        "label": "shakedown: deck-signal experiment under the uniform-legal pilot (design note section 10)",
        "part": part.label(),
        "binary": {
            "git_commit": env!("MTG_KERNEL_BUILD_GIT_HEAD"),
            "git_tree": env!("MTG_KERNEL_BUILD_GIT_TREE"),
            "git_clean": env!("MTG_KERNEL_BUILD_GIT_CLEAN"),
        },
        "card_db_hash": format!("{KERNEL_CARDDB_HASH:016x}"),
        "caps": {
            "max_driver_decisions": caps.max_decisions,
            "max_physical_decisions": caps.max_physical_decisions,
            "max_policy_steps": caps.max_policy_steps,
            "max_stack_objects": caps.max_stack_objects,
            "a2_caps": caps == A2_CAPS,
        },
        "seed_namespace": SEED_NAMESPACE,
        "seeds": seed_tuples(part),
        "deck_rule": {
            "nonland_cards": NONLAND_CARDS,
            "nonbasic_lands": NONBASIC_LANDS,
            "basic_lands": BASIC_LANDS,
            "max_pair_draws": MAX_PAIR_DRAWS,
        },
        "cube_list_sha256": sha256_hex(&cube),
        "gap_record_sha256": sha256_hex(&gaps),
        "fence_sha256": sha256_hex(&fence),
        "pool_cards": pool.card_count(),
        "pool_spells": run.pool.spells.len(),
        "pool_lands": run.pool.lands.len(),
        "games": run.games(),
        "worker_stack_bytes": WORKER_STACK_BYTES,
        "watchdog_seconds": WATCHDOG.as_secs(),
        "card_names": card_names(pool.ids().iter().copied().chain(run.pool.basics)),
    });
    if part == Part::B {
        record["ring"] = run.ring_record();
    }
    let unit = match part {
        Part::A => PART_A_GAMES_PER_PAIR,
        Part::B => 1,
    };
    Ok(Run { run, unit, record })
}

/// A `String` error that a [`CubeDraftError`] converts into.
struct StringError(String);

impl From<CubeDraftError> for StringError {
    fn from(error: CubeDraftError) -> Self {
        StringError(error.to_string())
    }
}

impl Run {
    /// Plays `[start, end)` in order, writing each line before the next game
    /// starts and naming the game in play in `progress` first.
    fn play(&self, start: u64, end: u64, out: &mut File, progress: &Path) -> Fallible<()> {
        std::fs::write(progress, start.to_string())
            .map_err(|error| format!("{}: {error}", progress.display()))?;
        self.run
            .play_range(start, end, |line: &GameLine| -> Result<(), StringError> {
                let text = format!("{}\n", line.to_json_line());
                out.write_all(text.as_bytes())
                    .and_then(|()| out.flush())
                    .map_err(|error| StringError(format!("writing a line: {error}")))?;
                // The next game, so a death names it.
                std::fs::write(progress, (line.index + 1).to_string())
                    .map_err(|error| StringError(format!("{}: {error}", progress.display())))
            })
            .map_err(|error| error.0)
    }
}

// ---------------------------------------------------------------------------
// Worker
// ---------------------------------------------------------------------------

fn worker(args: &Args) -> Fallible<()> {
    let run = load_run(args)?;
    let start = args
        .number("--worker-start")?
        .ok_or("--worker-start is required")?;
    let end = args
        .number("--worker-end")?
        .ok_or("--worker-end is required")?;
    if start >= end || end > run.run.games() {
        return Err(format!(
            "worker range [{start}, {end}) is outside [0, {})",
            run.run.games()
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
            .name("cube-deck-signal".to_owned())
            .stack_size(WORKER_STACK_BYTES)
            .spawn_scoped(scope, || run.play(start, end, &mut out, &progress))
            .map_err(|error| format!("spawning the game thread: {error}"))?
            .join()
            .map_err(|_| "the game thread panicked outside a game".to_owned())?
    })
}

// ---------------------------------------------------------------------------
// Supervisor
// ---------------------------------------------------------------------------

struct WorkerPart {
    start: u64,
    end: u64,
    /// The first index the current worker process has not yet written.
    next: u64,
    out: PathBuf,
    progress: PathBuf,
    child: Option<Child>,
    /// What last showed progress, and when.
    seen: (u64, String),
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

fn spawn(args: &Args, part: &WorkerPart) -> Fallible<Child> {
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

fn part_progress(part: &WorkerPart) -> (u64, String) {
    let size = std::fs::metadata(&part.out).map_or(0, |meta| meta.len());
    let current = std::fs::read_to_string(&part.progress).unwrap_or_default();
    (size, current)
}

fn kill_all(parts: &mut [WorkerPart]) {
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
    let total = run.run.games();
    let units = total / run.unit;
    let workers = args.number("--workers")?.unwrap_or(1).clamp(1, units);
    let started = Instant::now();
    let mut parts: Vec<WorkerPart> = (0..workers)
        .map(|k| {
            let start = run.unit * (units * k / workers);
            let end = run.unit * (units * (k + 1) / workers);
            let stem = out.as_os_str().to_owned();
            let with = |suffix: &str| {
                let mut path = stem.clone();
                path.push(format!(".w{k:03}.{suffix}"));
                PathBuf::from(path)
            };
            WorkerPart {
                start,
                end,
                next: start,
                out: with("part"),
                progress: with("progress"),
                child: None,
                seen: (0, String::new()),
                seen_at: Instant::now(),
            }
        })
        .collect();
    for part in &parts {
        for path in [&part.out, &part.progress] {
            if path.exists() {
                return Err(format!("{} exists from an earlier run", path.display()));
            }
        }
    }
    for k in 0..parts.len() {
        match spawn(args, &parts[k]) {
            Ok(child) => {
                parts[k].child = Some(child);
                parts[k].seen_at = Instant::now();
            }
            Err(error) => {
                kill_all(&mut parts);
                return Err(error);
            }
        }
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
                if seen != part.seen {
                    part.seen = seen;
                    part.seen_at = Instant::now();
                } else if part.seen_at.elapsed() > WATCHDOG {
                    let game = part.seen.1.clone();
                    kill_all(&mut parts);
                    return Err(format!(
                        "hang: worker {k} made no progress for {}s at game {game}; no output written",
                        WATCHDOG.as_secs()
                    ));
                }
                continue;
            };
            part.child = None;
            let written = match complete_lines(&part.out) {
                Ok(lines) => part.start + lines,
                Err(error) => {
                    kill_all(&mut parts);
                    return Err(error);
                }
            };
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
            let recorded = run
                .run
                .death(written, death)
                .map_err(|error| error.to_string())
                .and_then(|line| {
                    OpenOptions::new()
                        .append(true)
                        .open(&part.out)
                        .and_then(|mut file| {
                            file.write_all(format!("{}\n", line.to_json_line()).as_bytes())
                        })
                        .map_err(|error| format!("{}: {error}", part.out.display()))
                });
            if let Err(error) = recorded {
                kill_all(&mut parts);
                return Err(error);
            }
            deaths += 1;
            eprintln!(
                "{}",
                json!({"event": "process_died", "index": written, "stop_kind": death.stop_kind()})
            );
            part.next = written + 1;
            if part.next < part.end {
                match spawn(args, part) {
                    Ok(child) => part.child = Some(child),
                    Err(error) => {
                        kill_all(&mut parts);
                        return Err(error);
                    }
                }
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
            if index != Some(games) {
                return Err(format!(
                    "{}: line {games} has index {index:?}, expected {games}",
                    part.out.display()
                ));
            }
            output
                .write_all(format!("{line}\n").as_bytes())
                .map_err(|error| format!("{}: {error}", out.display()))?;
            games += 1;
        }
    }
    if games != total {
        return Err(format!("{games} lines, expected {total}"));
    }
    output
        .flush()
        .map_err(|error| format!("{}: {error}", out.display()))?;
    let record_text = format!(
        "{}\n",
        serde_json::to_string_pretty(&run.record).expect("a run record is plain data")
    );
    std::fs::write(&record, record_text)
        .map_err(|error| format!("{}: {error}", record.display()))?;
    for part in &parts {
        let _ = std::fs::remove_file(&part.out);
        let _ = std::fs::remove_file(&part.progress);
    }
    let seconds = started.elapsed().as_secs_f64();
    eprintln!(
        "{}",
        json!({"games": games, "process_deaths": deaths, "workers": workers,
            "wall_seconds": seconds,
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
        eprintln!("cube_deck_signal_v1: {error}");
        std::process::exit(2);
    }
}
