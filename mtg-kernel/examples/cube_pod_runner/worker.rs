use mtg_kernel::cube_draft_v1::{game_decision_counter, GameCaps, GameEnd};
use mtg_kernel::cube_pod_runner_v1::watchdog_game_end;
use mtg_kernel::cube_seat_v1::{
    canonical_deck, AttemptOutcome, AttemptSpec, GamePilot, UniformPilot,
};
use mtg_kernel::cube_worker_liveness_v1::{
    check_watchdog, with_heartbeat, WorkerProgress, HEARTBEAT_INTERVAL,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use super::Fallible;

pub(super) const FLAG: &str = "--attempt-worker";
const POLL: Duration = Duration::from_millis(250);
const STACK_BYTES: usize = 64 << 20;
static NEXT_WORKER: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize)]
struct Request {
    id: u64,
    cards: [Vec<u16>; 2],
    seats: [usize; 2],
    episode: u64,
    env_seed: u64,
    action_seed: u64,
    caps: GameCaps,
}

#[derive(Serialize, Deserialize)]
struct Reply {
    id: u64,
    decisions: u64,
    physical_decisions: Option<u64>,
    policy_steps: Option<u64>,
    end: GameEnd,
    last_action_source: Option<u16>,
}

impl Reply {
    fn into_outcome(self) -> AttemptOutcome {
        AttemptOutcome {
            decisions: self.decisions,
            physical_decisions: self.physical_decisions,
            policy_steps: self.policy_steps,
            end: self.end,
            last_action_source: self.last_action_source,
        }
    }
}

fn worker_loop(progress_path: &Path, interval: Duration) -> Fallible<()> {
    let (input, requests) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            if input.send(line).is_err() {
                return;
            }
        }
        // The supervisor owns the other end of stdin. Its exit must also
        // stop a child whose game driver is stuck in an engine call.
        std::process::exit(0);
    });
    let progress = WorkerProgress::new(0);
    with_heartbeat(progress_path, &progress, interval, || {
        let mut stdout = io::stdout().lock();
        for line in requests {
            let line = line.map_err(|error| error.to_string())?;
            let request: Request =
                serde_json::from_str(&line).map_err(|error| error.to_string())?;
            progress.begin(request.id);
            let decks = request.cards.each_ref().map(|cards| canonical_deck(cards));
            let outcome = UniformPilot.play(
                &AttemptSpec {
                    decks: [&decks[0], &decks[1]],
                    seats: request.seats,
                    episode: request.episode,
                    env_seed: request.env_seed,
                    action_seed: request.action_seed,
                },
                &request.caps,
            );
            let reply = Reply {
                id: request.id,
                decisions: outcome.decisions,
                physical_decisions: outcome.physical_decisions,
                policy_steps: outcome.policy_steps,
                end: outcome.end,
                last_action_source: outcome.last_action_source,
            };
            serde_json::to_writer(&mut stdout, &reply).map_err(|error| error.to_string())?;
            writeln!(stdout).map_err(|error| error.to_string())?;
            stdout.flush().map_err(|error| error.to_string())?;
        }
        Ok(())
    })
}

pub(super) fn run_worker(path: PathBuf, interval: Duration) -> Fallible<()> {
    std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(move || worker_loop(&path, interval))
        .map_err(|error| error.to_string())?
        .join()
        .map_err(|_| "attempt worker panicked".to_owned())?
}

struct AttemptWorker {
    child: Child,
    input: ChildStdin,
    replies: Receiver<Fallible<Reply>>,
    directory: PathBuf,
    next_id: u64,
}

impl AttemptWorker {
    fn launch(limit: Duration, mut command: Command) -> io::Result<Self> {
        let directory = loop {
            let index = NEXT_WORKER.fetch_add(1, Ordering::Relaxed);
            let candidate =
                std::env::temp_dir().join(format!("cube-pod-{}-{index}", std::process::id()));
            match std::fs::create_dir(&candidate) {
                Ok(()) => break candidate,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        };
        let interval = (limit / 4).clamp(Duration::from_millis(1), HEARTBEAT_INTERVAL);
        command
            .arg(FLAG)
            .arg(directory.join("progress"))
            .arg(interval.as_millis().to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let _ = std::fs::remove_dir(&directory);
                return Err(error);
            }
        };
        let input = child.stdin.take().expect("piped attempt input");
        let output = child.stdout.take().expect("piped attempt output");
        let (send, replies) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let reply = line.map_err(|error| error.to_string()).and_then(|line| {
                    serde_json::from_str(&line).map_err(|error| error.to_string())
                });
                if send.send(reply).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            replies,
            directory,
            next_id: 0,
        })
    }

    fn play(
        &mut self,
        spec: &AttemptSpec<'_>,
        caps: &GameCaps,
        limit: Duration,
    ) -> io::Result<AttemptOutcome> {
        let id = self.next_id;
        self.next_id += 1;
        let request = Request {
            id,
            cards: spec.decks.map(|deck| deck.card_ids()),
            seats: spec.seats,
            episode: spec.episode,
            env_seed: spec.env_seed,
            action_seed: spec.action_seed,
            caps: *caps,
        };
        serde_json::to_writer(&mut self.input, &request)?;
        writeln!(self.input)?;
        self.input.flush()?;
        let counter = game_decision_counter();
        let mut last_count = 0;
        let mut seen_at = Instant::now();
        let live = self.directory.join("progress.live");
        loop {
            match self.replies.recv_timeout(POLL.min(limit / 4)) {
                Ok(reply) => {
                    let reply = reply.map_err(io::Error::other)?;
                    if reply.id != id {
                        return Err(io::Error::other(format!(
                            "unexpected attempt worker reply id {}",
                            reply.id
                        )));
                    }
                    counter.store(reply.decisions, Ordering::Relaxed);
                    return Ok(reply.into_outcome());
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "attempt process closed its output",
                    ))
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            let count = std::fs::read_to_string(&live)
                .ok()
                .and_then(|sample| {
                    let (attempt, count) = sample.split_once(' ')?;
                    (attempt.parse::<u64>().ok()? == id).then(|| count.parse::<u64>().ok())?
                })
                .unwrap_or(last_count);
            let Some(next_seen_at) =
                check_watchdog(&last_count, &count, seen_at, Instant::now(), limit)
            else {
                return Ok(AttemptOutcome {
                    decisions: last_count,
                    physical_decisions: None,
                    policy_steps: None,
                    end: watchdog_game_end(),
                    last_action_source: None,
                });
            };
            seen_at = next_seen_at;
            last_count = count;
            counter.store(count, Ordering::Relaxed);
        }
    }
}

impl Drop for AttemptWorker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

thread_local! {
    static WORKER: RefCell<Option<AttemptWorker>> = const { RefCell::new(None) };
}

pub(super) struct SupervisedUniformPilot {
    limit: Duration,
    #[cfg(test)]
    pub(super) worker_command: Option<fn() -> Command>,
}

impl SupervisedUniformPilot {
    pub(super) fn new(limit: Duration) -> Self {
        Self {
            limit,
            #[cfg(test)]
            worker_command: None,
        }
    }

    fn launch(&self) -> io::Result<AttemptWorker> {
        #[cfg(test)]
        if let Some(command) = self.worker_command {
            return AttemptWorker::launch(self.limit, command());
        }
        AttemptWorker::launch(self.limit, Command::new(std::env::current_exe()?))
    }
}

impl GamePilot for SupervisedUniformPilot {
    fn play(&self, spec: &AttemptSpec<'_>, caps: &GameCaps) -> AttemptOutcome {
        WORKER.with_borrow_mut(|slot| {
            if slot.is_none() {
                match self.launch() {
                    Ok(worker) => *slot = Some(worker),
                    Err(error) => {
                        return worker_failure(
                            mtg_kernel::cube_pod_runner_v1::ATTEMPT_WORKER_IO,
                            error.to_string(),
                        )
                    }
                }
            }
            let result = slot
                .as_mut()
                .expect("started attempt worker")
                .play(spec, caps, self.limit);
            match result {
                Ok(outcome) => {
                    if mtg_kernel::cube_pod_runner_v1::is_watchdog_end(&outcome.end) {
                        *slot = None;
                    }
                    outcome
                }
                Err(error) => {
                    let mut worker = slot.take().expect("started attempt worker");
                    // EOF and broken pipes can arrive just before the child becomes
                    // waitable. Reap it and retain its OS exit code or signal.
                    let pid = worker.child.id();
                    // The attempt worker holds its pipes for its lifetime. A protocol
                    // error from a live worker is an I/O failure, not evidence of death.
                    let status = match worker.child.try_wait() {
                        Ok(None)
                            if matches!(
                                error.kind(),
                                io::ErrorKind::UnexpectedEof | io::ErrorKind::BrokenPipe
                            ) =>
                        {
                            worker.child.wait().map(Some)
                        }
                        status => status,
                    };
                    match status {
                        Ok(Some(status)) => worker_failure(
                            mtg_kernel::cube_pod_runner_v1::ATTEMPT_PROCESS_DEATH,
                            format!("pid {pid}, {status}, {error}"),
                        ),
                        Ok(None) => worker_failure(
                            mtg_kernel::cube_pod_runner_v1::ATTEMPT_WORKER_IO,
                            format!("pid {pid}: {error}"),
                        ),
                        Err(wait) => worker_failure(
                            mtg_kernel::cube_pod_runner_v1::ATTEMPT_WORKER_IO,
                            format!("pid {pid}: {error}, wait failed: {wait}"),
                        ),
                    }
                }
            }
        })
    }
}

fn worker_failure(code: &str, message: String) -> AttemptOutcome {
    AttemptOutcome {
        decisions: 0,
        physical_decisions: None,
        policy_steps: None,
        end: GameEnd::SessionError {
            code: code.to_owned(),
            message,
        },
        last_action_source: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_kernel::cube_pod_runner_v1::{
        is_watchdog_end, run_read_plan, BuilderRef, DeckSource, GameRole, PilotInfo, PodCaps,
        PodSeeds, PodUse, ReadDeckSpec, ReadGameSpec, ReadPlan,
    };

    fn scripted_worker() -> Command {
        // Startup is outside the subprocess's game watchdog. The first game
        // replies before a heartbeat, the second advances, and the third stalls.
        std::thread::sleep(Duration::from_millis(1_150));
        let mut command = Command::new(if cfg!(windows) { "python" } else { "python3" });
        command.args([
            "-u",
            "-c",
            r#"
import json, os, pathlib, sys, time
heartbeat = pathlib.Path(sys.argv[2]).with_suffix(".live")
temporary = heartbeat.with_suffix(".tmp")
for line in sys.stdin:
    request = json.loads(line)
    attempt = request["id"]
    episode = request["episode"]
    decisions = 1
    if episode == 1:
        for decisions in range(1, 9):
            temporary.write_text(f"{attempt} {decisions}", encoding="ascii")
            os.replace(temporary, heartbeat)
            time.sleep(0.18)
    elif episode == 2:
        time.sleep(10)
    print(json.dumps({"id": attempt, "decisions": decisions, "physical_decisions": None,
                      "policy_steps": None, "end": {"kind": "natural", "winner_seat": 0,
                      "reason": "fixture"}, "last_action_source": None}))
"#,
        ]);
        command
    }

    #[test]
    fn supervised_pilot_keeps_completed_and_advancing_games_and_kills_a_stall() {
        let decks = (0..2)
            .map(|chair| ReadDeckSpec {
                source: DeckSource::Build,
                chair: Some(chair),
                build: Some(0),
                builder: BuilderRef::uniform(),
                build_seed: Some(0),
                cards: vec![0; 40],
                build_rows: None,
                dealt: None,
            })
            .collect();
        let games = (0..3)
            .map(|index| ReadGameSpec {
                group: index,
                role: GameRole::Cross,
                pairing: None,
                decks: [0, 1],
                chairs: [Some(0), Some(1)],
                seats: [0, 1],
                game_seed: u64::from(index),
                episode: u64::from(index),
            })
            .collect();
        let plan = ReadPlan {
            pod_use: PodUse::Test,
            namespace: "test".to_owned(),
            run: "supervised-pilot".to_owned(),
            pod_index: 0,
            seeds: PodSeeds::for_read(1, 2, None, "test", "test"),
            pins: Default::default(),
            pilot: PilotInfo::uniform(),
            caps: PodCaps {
                watchdog_seconds: 1,
                ..PodCaps::read_a2()
            },
            chairs: Vec::new(),
            decks,
            games,
            paired_builds: Vec::new(),
        };
        let record = run_read_plan(
            &plan,
            &SupervisedUniformPilot {
                limit: Duration::from_secs(1),
                worker_command: Some(scripted_worker),
            },
            1,
        )
        .unwrap();

        assert_eq!(record.games[0].scores, Some([1, -1]));
        assert_eq!(record.games[0].attempts[0].decisions, 1);
        assert_eq!(record.games[1].scores, Some([1, -1]));
        assert_eq!(record.games[1].attempts[0].decisions, 8);
        assert!(is_watchdog_end(&record.games[2].attempts[0].end));
        assert_eq!(record.games[2].scores, None);
        assert_eq!(record.halts.len(), 1);
        assert_eq!(record.halts[0].game, 2);
    }
}
