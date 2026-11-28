//! Local stdio entry point for the fixed-chair Cube drafting and build phase.
mtg_kernel::fast_global_allocator!();

use mtg_kernel::cube_human_draft_build_v1::serve_draft_build_config;
use std::path::Path;

fn main() {
    if let Err(error) = run() {
        eprintln!("cube human pod: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let [flag, config] = args.as_slice() else {
        return Err("usage: cube_human_pod_v1 --draft-build CONFIG.json".into());
    };
    if flag != "--draft-build" {
        return Err("usage: cube_human_pod_v1 --draft-build CONFIG.json".into());
    }
    serve_draft_build_config(
        Path::new(config),
        std::io::stdin().lock(),
        std::io::stdout().lock(),
    )?;
    Ok(())
}
