//! Explicit original-time BO3 capture or read-only native batch preparation.
//! No optimizer, checkpoint publication, training loop or campaign dispatch.
use mtg_kernel::durable_publication_v1::{
    DurableFileExpectationV1, capture_existing_publication_parent_v1, publish_new_file_v1,
};
use mtg_kernel::expanded_deck_training_v1::PinnedFileV1;
use mtg_kernel::phase1_bo3_learning_v1::{
    Bo3GameplayPreparationReportV1, Bo3GameplayPreparationRequestV1,
    MAX_BO3_PREPARATION_REQUEST_BYTES_V1, TrainableBo3RequestV1, collect_trainable_bo3_v1,
    prepare_bo3_gameplay_batch_v1,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::PathBuf;

const COLLECT_REQUEST_BYTES: usize = 4 * 1024 * 1024;
const COLLECT_ENVELOPE_BYTES: u64 = 16 * 1024 * 1024;
const PREPARE_REPORT_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Collect,
    Prepare,
}
struct Arguments {
    command: Command,
    request: PathBuf,
    output: PathBuf,
}
fn arguments(args: Vec<OsString>) -> Result<Arguments, String> {
    if args.len() != 5 || args[1] != "--request" || args[3] != "--output" {
        return Err("usage: phase1_bo3_trainable_v1 collect|prepare --request absolute-request.json --output absolute-new-result.json".into());
    }
    let command = if args[0] == "collect" {
        Command::Collect
    } else if args[0] == "prepare" {
        Command::Prepare
    } else {
        return Err("only collect and prepare are supported; no optimizer command exists".into());
    };
    let request = PathBuf::from(&args[2]);
    let output = PathBuf::from(&args[4]);
    if !request.is_absolute() || !output.is_absolute() {
        return Err("request and output paths must be absolute".into());
    }
    Ok(Arguments {
        command,
        request,
        output,
    })
}

fn read_request(path: &PathBuf, maximum: usize) -> Result<(Vec<u8>, PinnedFileV1), String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > maximum as u64 {
        return Err("request must be a regular file within the command's byte bound".into());
    }
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > maximum {
        return Err("request grew beyond the command's byte bound".into());
    }
    let pin = PinnedFileV1 {
        path: path.clone(),
        sha256: format!("{:x}", Sha256::digest(&bytes)),
    };
    Ok((bytes, pin))
}

/// The existing durable publisher accepts borrowed bytes. Retain one bounded
/// serialization buffer, with no serde Value tree or second JSON byte copy.
fn bounded_json<T: Serialize>(value: &T, maximum: u64) -> Result<Vec<u8>, String> {
    struct Buffer {
        bytes: Vec<u8>,
        maximum: u64,
    }
    impl Write for Buffer {
        fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
            let next = self
                .bytes
                .len()
                .checked_add(input.len())
                .filter(|n| *n as u64 <= self.maximum)
                .ok_or_else(|| std::io::Error::other("output JSON byte bound exceeded"))?;
            self.bytes
                .try_reserve(next - self.bytes.len())
                .map_err(std::io::Error::other)?;
            self.bytes.extend_from_slice(input);
            Ok(input.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut buffer = Buffer {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut buffer, value).map_err(|e| e.to_string())?;
    Ok(buffer.bytes)
}

#[derive(Serialize)]
struct PreparationReceipt {
    schema: &'static str,
    request_artifact: PinnedFileV1,
    /// Preserves the exact learner, attempted request/result pins and limits.
    request: Bo3GameplayPreparationRequestV1,
    report: Bo3GameplayPreparationReportV1,
}

fn run() -> Result<(), String> {
    let args = arguments(std::env::args_os().skip(1).collect())?;
    let parent_path = args
        .output
        .parent()
        .ok_or("output requires an existing parent")?;
    let parent = capture_existing_publication_parent_v1(parent_path).map_err(|e| e.to_string())?;
    let final_name = args
        .output
        .file_name()
        .ok_or("output requires a filename")?;
    let mut stage_name = final_name.to_os_string();
    stage_name.push(".stage");
    if args.output.try_exists().map_err(|e| e.to_string())?
        || parent_path
            .join(&stage_name)
            .try_exists()
            .map_err(|e| e.to_string())?
    {
        return Err(
            "output or staging file already exists; preserve it and choose a fresh output".into(),
        );
    }
    let input_limit = match args.command {
        Command::Collect => COLLECT_REQUEST_BYTES,
        Command::Prepare => MAX_BO3_PREPARATION_REQUEST_BYTES_V1,
    };
    let (input, input_pin) = read_request(&args.request, input_limit)?;
    let text = std::str::from_utf8(&input).map_err(|e| e.to_string())?;
    let bytes = match args.command {
        Command::Collect => {
            let request = TrainableBo3RequestV1::from_json_v1(text)?;
            let maximum = request
                .config
                .max_decision_json_bytes
                .checked_add(request.capture_limits.max_json_bytes)
                .and_then(|n| n.checked_add(COLLECT_ENVELOPE_BYTES))
                .ok_or("collection output bound overflow")?;
            drop(input);
            let result = collect_trainable_bo3_v1(request)?;
            bounded_json(&result, maximum)?
        }
        Command::Prepare => {
            let request = Bo3GameplayPreparationRequestV1::from_json_v1(text)?;
            drop(input);
            let prepared = prepare_bo3_gameplay_batch_v1(request.clone())?;
            let report = prepared.report_v1().clone();
            // This command deliberately drops the private native groups. The
            // receipt neither publishes nor claims an optimizer continuation.
            drop(prepared);
            let receipt = PreparationReceipt {
                schema: "mtg-kernel-bo3-gameplay-preparation-report/v1",
                request_artifact: input_pin,
                request,
                report,
            };
            bounded_json(
                &receipt,
                MAX_BO3_PREPARATION_REQUEST_BYTES_V1 as u64 + PREPARE_REPORT_BYTES,
            )?
        }
    };
    let expected = DurableFileExpectationV1::from_bytes(&bytes).map_err(|e| e.to_string())?;
    let receipt = publish_new_file_v1(&parent, &stage_name, final_name, &bytes, expected)
        .map_err(|e| e.to_string())?;
    let digest: String = receipt
        .sha256()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    println!(
        "{} {} {} bytes",
        receipt.final_path().display(),
        digest,
        receipt.exact_length()
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trainable_cli_only_exposes_two_explicit_commands() {
        let root = std::env::temp_dir();
        let args = |mode: &str| {
            vec![
                OsString::from(mode),
                "--request".into(),
                root.join("request.json").into_os_string(),
                "--output".into(),
                root.join("new-output.json").into_os_string(),
            ]
        };
        assert_eq!(
            arguments(args("collect")).unwrap().command,
            Command::Collect
        );
        assert_eq!(
            arguments(args("prepare")).unwrap().command,
            Command::Prepare
        );
        assert!(arguments(args("update")).is_err());
        assert!(arguments(Vec::new()).is_err());
        let mut relative = args("collect");
        relative[2] = "relative-request.json".into();
        assert!(arguments(relative).is_err());
    }

    #[test]
    fn trainable_cli_serialization_is_exact_and_bounded() {
        let value = vec!["original", "records"];
        let expected = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            bounded_json(&value, expected.len() as u64).unwrap(),
            expected
        );
        assert!(bounded_json(&value, expected.len() as u64 - 1).is_err());
    }

    #[test]
    fn trainable_cli_public_preparation_parser_rejects_duplicate_and_oversize_json() {
        assert!(
            Bo3GameplayPreparationRequestV1::from_json_v1(
                r#"{"learner":{"source":{"x":1,"x":2}}}"#
            )
            .unwrap_err()
            .contains("duplicate JSON object key")
        );
        assert!(
            Bo3GameplayPreparationRequestV1::from_json_v1(
                &" ".repeat(MAX_BO3_PREPARATION_REQUEST_BYTES_V1 + 1)
            )
            .unwrap_err()
            .contains("exceeds 1 MiB")
        );
    }
}
