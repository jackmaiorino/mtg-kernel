//! Local stdio entry point for a stand-in Cube pod.
mtg_kernel::fast_global_allocator!();

use mtg_kernel::cube_human_draft_build_v1::{pod::serve_pod_config, serve_draft_build_config};
use std::path::Path;

fn main() {
    let result = std::thread::Builder::new()
        .name("cube-human-pod-v1".into())
        // Match the Cube bot workers' engine stack, including on Windows.
        .stack_size(64 * 1024 * 1024)
        .spawn(run)
        .map_err(|_| "The Cube pod could not start.".to_owned())
        .and_then(|worker| {
            worker
                .join()
                .map_err(|_| "The Cube pod stopped unexpectedly.".to_owned())?
        });
    if let Err(error) = result {
        eprintln!("cube human pod: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let input = std::io::stdin();
    let output = std::io::stdout();
    match args.as_slice() {
        [flag, config] if flag == "--config" => {
            serve_pod_config(Path::new(config), input.lock(), output.lock())
        }
        [flag, config] if flag == "--draft-build" => {
            serve_draft_build_config(Path::new(config), input.lock(), output.lock()).map(|_| ())
        }
        _ => Err("usage: cube_human_pod_v1 (--config|--draft-build) CONFIG.json".into()),
    }
}
