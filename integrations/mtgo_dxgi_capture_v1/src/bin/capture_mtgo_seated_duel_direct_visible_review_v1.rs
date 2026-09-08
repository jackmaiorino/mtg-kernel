#[cfg(not(target_os = "windows"))]
compile_error!("capture_mtgo_seated_duel_direct_visible_review_v1 is Windows-only");

use mtgo_dxgi_capture_v1::{
    qualify_attested_direct_visible_source_current_duel_v1,
    verify_direct_visible_source_runtime_v1, write_seated_duel_direct_visible_review_artifact_v1,
};
use std::path::Path;

fn main() {
    if let Err(error) = run() {
        eprintln!("MTGO_SEATED_DUEL_DIRECT_VISIBLE_REVIEW_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 6 {
        return Err(
            "expected GAME_FORMAT BROKER_PATH BOOTSTRAP_PATH PRODUCER_PATH VALIDATOR_PATH OUTPUT_DIRECTORY"
                .to_owned(),
        );
    }
    let game_format = args[0].to_str().ok_or("game format is not valid Unicode")?;
    let runtime = verify_direct_visible_source_runtime_v1(
        Path::new(&args[1]),
        Path::new(&args[2]),
        Path::new(&args[3]),
        Path::new(&args[4]),
    )?;
    let observation = qualify_attested_direct_visible_source_current_duel_v1(
        game_format,
        &runtime,
        2_000,
        10_000,
    )?;
    let receipt =
        write_seated_duel_direct_visible_review_artifact_v1(observation, Path::new(&args[5]))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&receipt)
            .map_err(|error| format!("serialize seated duel review receipt: {error}"))?
    );
    Ok(())
}
