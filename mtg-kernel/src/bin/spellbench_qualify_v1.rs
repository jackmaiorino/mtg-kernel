use mtg_kernel::spellbench_qualify_v1::{
    run_spellbench_qualification_v1, SpellbenchQualificationArgsV1,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

const USAGE: &str = "usage: spellbench_qualify_v1 --tournament-dir DIR --decision-log-dir DIR --bot NAME=CONFIG [--bot NAME=CONFIG]...";
/// The model loads and replays run on a worker with this stack, as in the
/// crate's other config-loading binaries (the Windows main thread has
/// 1 MiB).
const WORKER_STACK_BYTES: usize = 16 * 1024 * 1024;

fn usage() -> ! {
    eprintln!("{USAGE}");
    std::process::exit(2)
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let (mut tournament_dir, mut decision_log_dir, mut bot_configs) = (None, None, BTreeMap::new());
    // A repeated flag, or a repeated bot name, is refused rather than
    // letting the last one win silently.
    while let Some(flag) = arguments.next() {
        let value = arguments.next().unwrap_or_else(|| usage());
        match flag.as_str() {
            "--tournament-dir" if tournament_dir.is_none() => {
                tournament_dir = Some(PathBuf::from(value))
            }
            "--decision-log-dir" if decision_log_dir.is_none() => {
                decision_log_dir = Some(PathBuf::from(value))
            }
            "--bot" => {
                let (name, path) = value.split_once('=').unwrap_or_else(|| usage());
                if bot_configs
                    .insert(name.to_string(), PathBuf::from(path))
                    .is_some()
                {
                    usage()
                }
            }
            _ => usage(),
        }
    }
    let (Some(tournament_dir), Some(decision_log_dir)) = (tournament_dir, decision_log_dir) else {
        usage()
    };
    let args = SpellbenchQualificationArgsV1 {
        tournament_dir,
        decision_log_dir,
        bot_configs,
    };
    let result = std::thread::Builder::new()
        .name("spellbench-qualify".into())
        .stack_size(WORKER_STACK_BYTES)
        .spawn(move || run_spellbench_qualification_v1(&args))
        .map_err(|error| format!("cannot start the qualification thread: {error}"))
        .and_then(|worker| {
            worker
                .join()
                .map_err(|_| "qualification thread panicked".to_owned())
                .and_then(|result| result)
        });
    match result {
        Ok(report) => {
            println!("{report}");
            std::process::exit(if report["verdict"] == "PASS" { 0 } else { 1 });
        }
        Err(message) => {
            eprintln!("spellbench qualify: {message}");
            std::process::exit(1);
        }
    }
}
