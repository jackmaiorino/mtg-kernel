#[cfg(not(target_os = "windows"))]
compile_error!("capture_mtgo_direct_visible_background_review_v1 is Windows-only");

use mtgo_dxgi_capture_v1::{
    qualify_stable_background_direct_visible_source_v1, verify_direct_visible_source_runtime_v1,
    write_stable_background_direct_visible_review_artifact_v1,
};
use std::path::Path;

fn main() {
    if let Err(error) = run() {
        eprintln!("MTGO_DIRECT_VISIBLE_BACKGROUND_REVIEW_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 5 {
        return Err(
            "expected BROKER_PATH BOOTSTRAP_PATH PRODUCER_PATH VALIDATOR_PATH OUTPUT_DIRECTORY"
                .to_owned(),
        );
    }
    let runtime = verify_direct_visible_source_runtime_v1(
        Path::new(&args[0]),
        Path::new(&args[1]),
        Path::new(&args[2]),
        Path::new(&args[3]),
    )?;
    let observation = qualify_stable_background_direct_visible_source_v1(&runtime, 10_000)?;
    let receipt = write_stable_background_direct_visible_review_artifact_v1(
        observation,
        Path::new(&args[4]),
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&receipt)
            .map_err(|error| format!("serialize background review receipt: {error}"))?
    );
    Ok(())
}
