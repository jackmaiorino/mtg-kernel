//! Per-decision progress shared by the cube census and pod supervisors.

use crate::cube_draft_v1::game_decision_counter;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

pub const LIVENESS: &str = "per-decision-v1";
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

pub struct WorkerProgress {
    game: Mutex<u64>,
    decisions: Arc<AtomicU64>,
}

impl WorkerProgress {
    pub fn new(start: u64) -> Self {
        let decisions = game_decision_counter();
        decisions.store(0, Ordering::Relaxed);
        Self {
            game: Mutex::new(start),
            decisions,
        }
    }

    // A sampler sees the index and count on the same side of a game boundary.
    pub fn begin(&self, index: u64) {
        let mut game = self.game.lock().unwrap();
        self.decisions.store(0, Ordering::Relaxed);
        *game = index;
    }

    pub fn decision_counter(&self) -> &AtomicU64 {
        &self.decisions
    }

    pub fn sample(&self) -> (u64, u64) {
        let game = self.game.lock().unwrap();
        (*game, self.decisions.load(Ordering::Relaxed))
    }
}

pub fn with_heartbeat<T>(
    progress: &Path,
    current: &WorkerProgress,
    interval: Duration,
    play: impl FnOnce() -> T,
) -> T {
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
        });
        let result = play();
        drop(stop);
        heartbeat.join().expect("heartbeat thread panicked");
        result
    })
}

/// Returns the time of the last change, or None if the worker is stuck.
pub fn check_watchdog<T: PartialEq>(
    previous: &T,
    current: &T,
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

/// Observes an in-process pilot. A subprocess supervisor can also kill and
/// replace a worker when its pilot never returns.
pub(crate) fn observe_decisions<T>(limit: Duration, play: impl FnOnce() -> T) -> (T, bool) {
    let counter = game_decision_counter();
    counter.store(0, Ordering::Relaxed);
    let interval = (limit / 4).clamp(Duration::from_millis(1), Duration::from_secs(1));
    std::thread::scope(|scope| {
        let (finished, wait) = mpsc::channel::<Instant>();
        let started = Instant::now();
        let monitor = scope.spawn(move || {
            let mut last_count = 0;
            let mut last_change = started;
            loop {
                let ended = match wait.recv_timeout(interval) {
                    Ok(ended) => Some(ended),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return false,
                };
                let now = ended.unwrap_or_else(Instant::now);
                let count = counter.load(Ordering::Relaxed);
                let Some(seen_at) = check_watchdog(&last_count, &count, last_change, now, limit)
                else {
                    return true;
                };
                if ended.is_some() {
                    return false;
                }
                last_change = seen_at;
                last_count = count;
            }
        });
        let value = play();
        let _ = finished.send(Instant::now());
        (value, monitor.join().expect("pilot monitor panicked"))
    })
}
