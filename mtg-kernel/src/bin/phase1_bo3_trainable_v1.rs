//! Explicit BO3 capture, read-only preparation, one CPU update, or finite run.
//! Update and finite-run durability/recovery belong to their library APIs.
use mtg_kernel::durable_publication_v1::{
    DurableFileExpectationV1, capture_existing_publication_parent_v1, publish_new_file_v1,
};
use mtg_kernel::expanded_deck_training_v1::PinnedFileV1;
use mtg_kernel::phase1_bo3_learning_v1::{
    Bo3GameplayPreparationReportV1, Bo3GameplayPreparationRequestV1, Bo3GameplayUpdateRequestV1,
    MAX_BO3_PREPARATION_REQUEST_BYTES_V1, MAX_NATIVE_BO3_RUN_REQUEST_BYTES_V1,
    NativeBo3TrainingRunV1, TrainableBo3RequestV1, collect_trainable_bo3_v1,
    prepare_bo3_gameplay_batch_v1, run_native_bo3_training_v1, update_bo3_gameplay_v1,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::PathBuf;

const COLLECT_REQUEST_BYTES: usize = 4 * 1024 * 1024;
const COLLECT_ENVELOPE_BYTES: u64 = 16 * 1024 * 1024;
const PREPARE_REPORT_BYTES: u64 = 4 * 1024 * 1024;
const UPDATE_REQUEST_BYTES: usize = 1024 * 1024;
const UPDATE_RESULT_BYTES: u64 = 4 * 1024 * 1024;
const WORKER_STACK_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Collect,
    Prepare,
    Update,
    Run,
}
struct Arguments {
    command: Command,
    request: PathBuf,
    output: Option<PathBuf>,
    max_new_batches: Option<usize>,
}
fn arguments(args: Vec<OsString>) -> Result<Arguments, String> {
    if args.first().is_some_and(|mode| mode == "run") {
        if !matches!(args.len(), 3 | 5) || args[1] != "--request" {
            return Err("usage: phase1_bo3_trainable_v1 run --request absolute-run.json [--max-new-batches positive-integer]".into());
        }
        let request = PathBuf::from(&args[2]);
        if !request.is_absolute() {
            return Err("request path must be absolute".into());
        }
        let max_new_batches = if args.len() == 5 {
            if args[3] != "--max-new-batches" {
                return Err("run accepts only --max-new-batches after its request".into());
            }
            let value = args[4].to_str().ok_or("batch limit is not UTF8")?;
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err("batch limit must be a positive integer".into());
            }
            let value: usize = value
                .parse()
                .map_err(|_| "batch limit exceeds integer range")?;
            if value == 0 {
                return Err("batch limit must be positive".into());
            }
            Some(value)
        } else {
            None
        };
        return Ok(Arguments {
            command: Command::Run,
            request,
            output: None,
            max_new_batches,
        });
    }
    if args.first().is_some_and(|mode| mode == "update") {
        if args.len() != 3 || args[1] != "--request" {
            return Err("usage: phase1_bo3_trainable_v1 update --request absolute-request.json; output_directory belongs to the request".into());
        }
        let request = PathBuf::from(&args[2]);
        if !request.is_absolute() {
            return Err("request path must be absolute".into());
        }
        return Ok(Arguments {
            command: Command::Update,
            request,
            output: None,
            max_new_batches: None,
        });
    }
    if args.len() != 5 || args[1] != "--request" || args[3] != "--output" {
        return Err("usage: phase1_bo3_trainable_v1 collect|prepare --request absolute-request.json --output absolute-new-result.json".into());
    }
    let command = if args[0] == "collect" {
        Command::Collect
    } else if args[0] == "prepare" {
        Command::Prepare
    } else {
        return Err("only collect, prepare, update and run are supported".into());
    };
    let request = PathBuf::from(&args[2]);
    let output = PathBuf::from(&args[4]);
    if !request.is_absolute() || !output.is_absolute() {
        return Err("request and output paths must be absolute".into());
    }
    Ok(Arguments {
        command,
        request,
        output: Some(output),
        max_new_batches: None,
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
    if args.command == Command::Run {
        let (input, _) = read_request(&args.request, MAX_NATIVE_BO3_RUN_REQUEST_BYTES_V1)?;
        let request = NativeBo3TrainingRunV1::from_json_v1(
            std::str::from_utf8(&input).map_err(|e| e.to_string())?,
        )?;
        drop(input);
        let result = run_native_bo3_training_v1(&request, args.max_new_batches)?;
        let bytes = bounded_json(&result, MAX_NATIVE_BO3_RUN_REQUEST_BYTES_V1 as u64)?;
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(&bytes).map_err(|e| e.to_string())?;
        stdout.write_all(b"\n").map_err(|e| e.to_string())?;
        return stdout.flush().map_err(|e| e.to_string());
    }
    if args.command == Command::Update {
        let (input, _) = read_request(&args.request, UPDATE_REQUEST_BYTES)?;
        let request = Bo3GameplayUpdateRequestV1::from_json_v1(
            std::str::from_utf8(&input).map_err(|e| e.to_string())?,
        )?;
        drop(input);
        // The library owns exactly this operation's durable output and recovery.
        // No second output target, checkpoint copy, retry or loop is introduced.
        let result = update_bo3_gameplay_v1(request)?;
        let bytes = bounded_json(&result, UPDATE_RESULT_BYTES)?;
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(&bytes).map_err(|e| e.to_string())?;
        stdout.write_all(b"\n").map_err(|e| e.to_string())?;
        return stdout.flush().map_err(|e| e.to_string());
    }
    let output = args.output.as_ref().ok_or("artifact output missing")?;
    let parent_path = output
        .parent()
        .ok_or("output requires an existing parent")?;
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
            "output or staging file already exists; preserve it and choose a fresh output".into(),
        );
    }
    let input_limit = match args.command {
        Command::Collect => COLLECT_REQUEST_BYTES,
        Command::Prepare => MAX_BO3_PREPARATION_REQUEST_BYTES_V1,
        Command::Update | Command::Run => return Err("update/run own their library output".into()),
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
        Command::Update | Command::Run => return Err("update/run own their library output".into()),
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
fn run_worker() -> Result<(), String> {
    std::thread::Builder::new()
        .name("phase1-bo3-worker".into())
        .stack_size(WORKER_STACK_BYTES)
        .spawn(run)
        .map_err(|error| format!("could not start BO3 worker: {error}"))?
        .join()
        .map_err(|_| "BO3 worker panicked; see the panic diagnostic".to_owned())?
}

fn main() {
    if let Err(error) = run_worker() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trainable_cli_run_has_explicit_invocation_only_batch_limit() {
        let base = vec![
            "run".into(),
            "--request".into(),
            std::env::temp_dir().join("run.json").into_os_string(),
        ];
        let parsed = arguments(base.clone()).unwrap();
        assert_eq!(parsed.command, Command::Run);
        assert_eq!(parsed.max_new_batches, None);
        assert!(parsed.output.is_none());
        let mut bounded = base.clone();
        bounded.extend(["--max-new-batches".into(), "2".into()]);
        assert_eq!(arguments(bounded.clone()).unwrap().max_new_batches, Some(2));
        for invalid in ["0", "-1", "two", "+2", ""] {
            bounded[4] = invalid.into();
            assert!(arguments(bounded.clone()).is_err());
        }
        bounded[3] = "--output".into();
        assert!(arguments(bounded).is_err());
        assert!(arguments(base[..2].to_vec()).is_err());
    }

    #[test]
    fn trainable_cli_preserves_collect_and_prepare_argument_contracts() {
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
        assert!(arguments(args("unknown")).is_err());
        assert!(arguments(Vec::new()).is_err());
        let mut relative = args("collect");
        relative[2] = "relative-request.json".into();
        assert!(arguments(relative).is_err());
    }

    #[test]
    fn trainable_cli_update_requires_only_one_absolute_request() {
        let path = std::env::temp_dir().join("update-request.json");
        let input = vec![
            "update".into(),
            "--request".into(),
            path.clone().into_os_string(),
        ];
        let parsed = arguments(input.clone()).unwrap();
        assert_eq!(parsed.command, Command::Update);
        assert_eq!(parsed.request, path);
        assert!(parsed.output.is_none());
        assert!(arguments(input[..2].to_vec()).is_err());
        let mut extra = input.clone();
        extra.extend(["--output".into(), "another.json".into()]);
        assert!(arguments(extra).is_err());
        let mut wrong_flag = input.clone();
        wrong_flag[1] = "--output".into();
        assert!(arguments(wrong_flag).is_err());
        let mut relative = input;
        relative[2] = "relative.json".into();
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

    #[test]
    fn trainable_cli_update_uses_strict_bounded_request_parser() {
        assert!(
            Bo3GameplayUpdateRequestV1::from_json_v1(
                r#"{"input":{"kind":"bo3_checkpoint","kind":"ordinary_checkpoint_transition"}}"#
            )
            .unwrap_err()
            .contains("duplicate JSON object key")
        );
        assert!(
            Bo3GameplayUpdateRequestV1::from_json_v1(&" ".repeat(UPDATE_REQUEST_BYTES + 1))
                .unwrap_err()
                .contains("exceeds 1 MiB")
        );
    }
}
