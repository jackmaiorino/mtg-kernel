//! Converts RT1 checkpoints (such as the OH2 b512 doses) to the V8 inclusion
//! net and checks the preservation on real games
//! (`mtg_kernel::expanded_deck_training_v1::v8_inclusion_conversion_v1`).
//!
//! Usage: `v8_inclusion_convert_v1 <requests.json> <receipt.json>`, where
//! `requests.json` is a JSON array of
//! `mtg-kernel-v8-inclusion-conversion-request/v1` objects. The receipt is a
//! JSON object with one entry per request, in order, written once every
//! conversion has succeeded; the first failure stops the run.

use mtg_kernel::expanded_deck_training_v1::v8_inclusion_conversion_v1::{
    convert_checkpoint_to_v8_inclusion_v1, V8ConversionRequestV1,
};

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let [_, requests, receipt] = arguments.as_slice() else {
        eprintln!("usage: v8_inclusion_convert_v1 <requests.json> <receipt.json>");
        std::process::exit(2);
    };
    let text = std::fs::read_to_string(requests).expect("read the requests");
    let requests: Vec<V8ConversionRequestV1> =
        serde_json::from_str(&text).expect("parse the requests");
    let mut entries = Vec::with_capacity(requests.len());
    for request in &requests {
        let started = std::time::Instant::now();
        match convert_checkpoint_to_v8_inclusion_v1(request) {
            Ok(mut entry) => {
                entry["elapsed_seconds"] = serde_json::json!(started.elapsed().as_secs_f64());
                eprintln!(
                    "converted {} -> {}: {} rows, {} mismatches",
                    request.parent_checkpoint.path.display(),
                    request.output_checkpoint.display(),
                    entry["preservation"]["rows"],
                    entry["preservation"]["mismatches"]
                );
                entries.push(entry);
            }
            Err(error) => {
                eprintln!(
                    "conversion of {} failed: {error}",
                    request.parent_checkpoint.path.display()
                );
                std::process::exit(1);
            }
        }
    }
    let document = serde_json::json!({
        "schema": "mtg-kernel-v8-inclusion-conversion-receipts/v1",
        "entries": entries,
    });
    std::fs::write(
        receipt,
        serde_json::to_vec_pretty(&document).expect("serialize the receipt"),
    )
    .expect("write the receipt");
}
