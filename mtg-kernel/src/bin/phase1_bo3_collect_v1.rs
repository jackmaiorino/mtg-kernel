//! Explicit one-match collector. This command never updates weights or starts
//! a training campaign. Current-executable package verification is mandatory.
use mtg_kernel::durable_publication_v1::{
    DurableFileExpectationV1, capture_existing_publication_parent_v1, publish_new_file_v1,
};
use mtg_kernel::phase1_bo3_collection_v1::{
    Bo3CollectionRequestV1, MAX_BO3_COLLECTION_REQUEST_BYTES_V1, collect_bo3_trajectory_v1,
};
use std::io::Read;
use std::path::PathBuf;

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 4 || args[0] != "--request" || args[2] != "--output" {
        return Err(
            "usage: phase1_bo3_collect_v1 --request request.json --output new-result.json".into(),
        );
    }
    let output = PathBuf::from(&args[3]);
    let parent_path = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let parent = capture_existing_publication_parent_v1(parent_path).map_err(|e| e.to_string())?;
    let final_name = output.file_name().ok_or("output requires a filename")?;
    let mut stage_name = final_name.to_os_string();
    stage_name.push(".stage");
    if output.try_exists().map_err(|e| e.to_string())?
        || parent_path
            .join(&stage_name)
            .try_exists()
            .map_err(|e| e.to_string())?
    {
        return Err(
            "output or staging file already exists; preserve and inspect it before replay".into(),
        );
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&args[1])
        .map_err(|e| e.to_string())?
        .take(MAX_BO3_COLLECTION_REQUEST_BYTES_V1 as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
    let request = Bo3CollectionRequestV1::from_json_v1(text)?;
    let result = collect_bo3_trajectory_v1(request.config, request.packages)?;
    let bytes = serde_json::to_vec(&result).map_err(|e| e.to_string())?;
    let expected = DurableFileExpectationV1::from_bytes(&bytes).map_err(|e| e.to_string())?;
    publish_new_file_v1(&parent, &stage_name, final_name, &bytes, expected)
        .map_err(|e| e.to_string())?;
    println!(
        "{} {} {:?}",
        output.display(),
        result.collected.trajectory_sha256,
        result.collected.trajectory.ending
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
