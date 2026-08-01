use mtg_kernel::native_bilinear_policy_residual_v1::run_native_bilinear_policy_residual_probe_v1;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

fn usage_v1() -> ! {
    eprintln!(
        "usage: native_bilinear_policy_residual_v1 --source-outcome-root PATH --outcome-jsonl PATH --output-dir NEW_PATH"
    );
    std::process::exit(2);
}

fn parse_args_v1(raw: Vec<OsString>) -> Result<(PathBuf, PathBuf, PathBuf), ()> {
    if raw.len() != 6 {
        return Err(());
    }
    let mut source = None;
    let mut corpus = None;
    let mut output = None;
    for pair in raw.chunks_exact(2) {
        if pair[0] == "--source-outcome-root" && source.is_none() {
            source = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--outcome-jsonl" && corpus.is_none() {
            corpus = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--output-dir" && output.is_none() {
            output = Some(PathBuf::from(&pair[1]));
        } else {
            return Err(());
        }
    }
    match (source, corpus, output) {
        (Some(source), Some(corpus), Some(output)) => Ok((source, corpus, output)),
        _ => Err(()),
    }
}

fn write_new_file_v1(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(bytes)?;
    writer.flush()?;
    writer.get_ref().sync_all()
}

fn main() {
    let (source, corpus, output) =
        parse_args_v1(std::env::args_os().skip(1).collect()).unwrap_or_else(|()| usage_v1());
    if output.as_os_str().is_empty() {
        usage_v1();
    }
    let envelope =
        run_native_bilinear_policy_residual_probe_v1(&source, &corpus).unwrap_or_else(|error| {
            eprintln!("native bilinear policy residual probe failed: {error}");
            std::process::exit(1);
        });
    fs::create_dir(&output).unwrap_or_else(|error| {
        eprintln!("native bilinear output directory create failed: {error}");
        std::process::exit(1);
    });
    if let Some(weights) = envelope.final_weights_f32le_v1() {
        write_new_file_v1(&output.join("weights.f32le"), weights).unwrap_or_else(|error| {
            eprintln!("native bilinear weights write failed: {error}");
            std::process::exit(1);
        });
    }
    write_new_file_v1(
        &output.join("report.json"),
        envelope.deterministic_report_bytes_v1(),
    )
    .unwrap_or_else(|error| {
        eprintln!("native bilinear report write failed: {error}");
        std::process::exit(1);
    });
    println!(
        "{} {} {} {}",
        envelope.deterministic_report_sha256_v1(),
        envelope.elapsed_milliseconds_v1(),
        envelope.disposition_v1(),
        if envelope.advance_to_cp7_v1() {
            "weights-emitted"
        } else {
            "no-weights"
        }
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_order_independent_cli_v1() {
        let (source, corpus, output) = parse_args_v1(vec![
            "--output-dir".into(),
            "output".into(),
            "--source-outcome-root".into(),
            "source".into(),
            "--outcome-jsonl".into(),
            "corpus.jsonl".into(),
        ])
        .unwrap();
        assert_eq!(source, PathBuf::from("source"));
        assert_eq!(corpus, PathBuf::from("corpus.jsonl"));
        assert_eq!(output, PathBuf::from("output"));
        assert!(parse_args_v1(vec![
            "--source-outcome-root".into(),
            "source".into(),
            "--source-outcome-root".into(),
            "other".into(),
            "--outcome-jsonl".into(),
            "corpus.jsonl".into(),
        ])
        .is_err());
    }
}
