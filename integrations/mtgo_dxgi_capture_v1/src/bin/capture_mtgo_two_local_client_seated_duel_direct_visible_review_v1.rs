#[cfg(not(target_os = "windows"))]
compile_error!(
    "capture_mtgo_two_local_client_seated_duel_direct_visible_review_v1 is Windows-only"
);

use mtgo_dxgi_capture_v1::{
    load_approved_mtgo_client_two_local_client_target_binding_v1,
    qualify_attested_direct_visible_source_current_duel_with_two_local_clients_v1,
    verify_two_local_client_direct_visible_source_runtime_v1,
    write_two_local_client_seated_duel_direct_visible_review_artifact_v1,
};
use std::path::Path;

fn main() {
    if let Err(error) = run() {
        eprintln!("MTGO_TWO_LOCAL_CLIENT_SEATED_DUEL_REVIEW_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 7 {
        return Err(
            "expected GAME_FORMAT TARGET_BINDING_FILE BROKER_PATH BOOTSTRAP_PATH PRODUCER_PATH VALIDATOR_PATH OUTPUT_DIRECTORY"
                .to_owned(),
        );
    }
    let game_format = args[0].to_str().ok_or("game format is not valid Unicode")?;
    let target_binding =
        load_approved_mtgo_client_two_local_client_target_binding_v1(Path::new(&args[1]))?;
    let runtime = verify_two_local_client_direct_visible_source_runtime_v1(
        Path::new(&args[2]),
        Path::new(&args[3]),
        Path::new(&args[4]),
        Path::new(&args[5]),
    )?;
    let observation =
        qualify_attested_direct_visible_source_current_duel_with_two_local_clients_v1(
            game_format,
            &runtime,
            target_binding,
            2_000,
            10_000,
        )?;
    let receipt = write_two_local_client_seated_duel_direct_visible_review_artifact_v1(
        observation,
        Path::new(&args[6]),
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&receipt)
            .map_err(|error| format!("serialize two-local-client review receipt: {error}"))?
    );
    Ok(())
}
