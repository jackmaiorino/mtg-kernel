//! Explicit complete-agent human interface; V1 remains a separate entry point.
use mtg_kernel::human_match_v2::{
    MAX_RECORDED_REPLAY_INPUT_BYTES_V2, prepare_recorded_human_replay_v2, serve_human_match_v2,
};
use std::io::Read;

fn main() {
    let result = std::thread::Builder::new()
        .name("human-match-v2".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(run)
        .map_err(|_| "The human session could not start.".to_owned())
        .and_then(|worker| {
            worker
                .join()
                .map_err(|_| "The human session stopped unexpectedly.".to_owned())?
        });
    if let Err(error) = result {
        eprintln!("human match: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() == 4 && arguments[0] == "--prepare-replay" && arguments[2] == "--output" {
        return prepare_replay(
            std::path::Path::new(&arguments[1]),
            std::path::Path::new(&arguments[3]),
        );
    }
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--config")) {
        return Err("usage: human_match_v2 --config CONFIG.json".into());
    }
    let config = args
        .next()
        .ok_or("A server-local config path is required.")?;
    if args.next().is_some() {
        return Err("Unexpected command-line argument.".into());
    }
    serve_human_match_v2(
        std::path::Path::new(&config),
        std::io::stdin().lock(),
        std::io::stdout().lock(),
    )
}

fn prepare_replay(input: &std::path::Path, output: &std::path::Path) -> Result<(), String> {
    use mtg_kernel::durable_publication_v1::{
        DurableFileExpectationV1, capture_existing_publication_parent_v1, publish_new_file_v1,
    };
    let parent_path = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let parent = capture_existing_publication_parent_v1(parent_path).map_err(|e| e.to_string())?;
    let name = output.file_name().ok_or("Output requires a filename.")?;
    let mut stage = name.to_os_string();
    stage.push(".stage");
    if output.try_exists().map_err(|e| e.to_string())?
        || parent_path
            .join(&stage)
            .try_exists()
            .map_err(|e| e.to_string())?
    {
        return Err("Preserve the existing replay output or stage.".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(input)
        .map_err(|e| e.to_string())?
        .take(MAX_RECORDED_REPLAY_INPUT_BYTES_V2 as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let prepared =
        prepare_recorded_human_replay_v2(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?)?;
    let bytes = serde_json::to_vec(&prepared).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_RECORDED_REPLAY_INPUT_BYTES_V2 {
        return Err("Recorded public replay exceeds 256 MiB.".into());
    }
    let expected = DurableFileExpectationV1::from_bytes(&bytes).map_err(|e| e.to_string())?;
    publish_new_file_v1(&parent, &stage, name, &bytes, expected).map_err(|e| e.to_string())?;
    Ok(())
}
