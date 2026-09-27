use mtg_kernel::spellbench_scorer_v1::run_spellbench_scorer_config_path_v1;
use std::path::PathBuf;

/// The model load and every forward run on a worker with this stack, as in
/// the crate's other config-loading binaries (the Windows main thread has
/// 1 MiB).
const WORKER_STACK_BYTES: usize = 16 * 1024 * 1024;

fn run() -> Result<(), String> {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new("--config")) {
        return Err("usage: spellbench_scorer_v1 --config PATH".into());
    }
    let config = PathBuf::from(arguments.next().ok_or("missing config path")?);
    if arguments.next().is_some() {
        return Err("unexpected argument".into());
    }
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    run_spellbench_scorer_config_path_v1(&config, &mut stdin.lock(), &mut stdout.lock())
}

fn main() {
    let result = std::thread::Builder::new()
        .name("spellbench-scorer".into())
        .stack_size(WORKER_STACK_BYTES)
        .spawn(run)
        .map_err(|error| format!("cannot start the scorer thread: {error}"))
        .and_then(|worker| {
            worker
                .join()
                .map_err(|_| "scorer thread panicked".to_owned())
                .and_then(|result| result)
        });
    if let Err(code) = result {
        eprintln!("spellbench scorer: {code}");
        std::process::exit(1);
    }
}
