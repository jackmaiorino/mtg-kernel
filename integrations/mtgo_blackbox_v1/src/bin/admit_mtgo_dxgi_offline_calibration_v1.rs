use mtgo_blackbox_v1::admit_ratified_dxgi_offline_calibration_artifact_v1;
use serde_json::json;
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    if let Err(error) = run() {
        eprintln!("MTGO_DXGI_OFFLINE_CALIBRATION_REJECTED:{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let artifact_directory = PathBuf::from(
        arguments
            .next()
            .ok_or("usage: admit_mtgo_dxgi_offline_calibration_v1 <artifact-directory>")?,
    );
    if arguments.next().is_some() {
        return Err("exactly one artifact directory is required".to_owned());
    }
    if !artifact_directory.is_absolute() || !artifact_directory.is_dir() {
        return Err("artifact directory must be an existing absolute directory".to_owned());
    }

    let manifest = fs::read(artifact_directory.join("manifest.json"))
        .map_err(|error| format!("read manifest.json: {error}"))?;
    let canonical = fs::read(artifact_directory.join("frame.bgra"))
        .map_err(|error| format!("read frame.bgra: {error}"))?
        .into_boxed_slice();
    let preview = fs::read(artifact_directory.join("frame.png"))
        .map_err(|error| format!("read frame.png: {error}"))?;
    let admitted =
        admit_ratified_dxgi_offline_calibration_artifact_v1(&manifest, canonical, &preview)
            .map_err(|error| error.to_string())?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "ratified_offline_calibration_only",
            "scope": format!("{:?}", admitted.scope()),
            "admission_commitment_sha256": admitted.admission_commitment_sha256(),
            "manifest_sha256": admitted.manifest_sha256(),
            "canonical_bgra8_sha256": admitted.canonical_bgra8_sha256(),
            "preview_png_sha256": admitted.preview_png_sha256(),
            "output_identity_sha256": admitted.output_identity_sha256(),
            "client_size_px": admitted.client_size_px(),
            "captured_at_unix_millis": admitted.captured_at_unix_millis(),
            "capture_role": format!("{:?}", admitted.capture_role()),
            "safe_for_live_ocr": admitted.safe_for_live_ocr(),
            "safe_for_semantic_evidence": admitted.safe_for_semantic_evidence(),
            "safe_for_policy_scoring": admitted.safe_for_policy_scoring(),
            "safe_for_input": admitted.safe_for_input()
        }))
        .map_err(|error| format!("serialize result: {error}"))?
    );
    Ok(())
}
