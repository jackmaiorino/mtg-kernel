use mtg_kernel::native_rollout_teacher_v1::{
    native_rollout_teacher_report_bytes_v1, run_native_rollout_teacher_v1,
};
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

fn usage_v1() -> ! {
    eprintln!("usage: native_rollout_teacher_v1 --source-outcome-root PATH --output-json PATH");
    std::process::exit(2);
}

fn parse_args_v1(raw: Vec<OsString>) -> Result<(PathBuf, PathBuf), ()> {
    if raw.len() != 4 {
        return Err(());
    }
    let mut source = None;
    let mut output = None;
    for pair in raw.chunks_exact(2) {
        if pair[0] == "--source-outcome-root" && source.is_none() {
            source = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--output-json" && output.is_none() {
            output = Some(PathBuf::from(&pair[1]));
        } else {
            return Err(());
        }
    }
    match (source, output) {
        (Some(source), Some(output)) => Ok((source, output)),
        _ => Err(()),
    }
}

fn main() {
    let (source, output) =
        parse_args_v1(std::env::args_os().skip(1).collect()).unwrap_or_else(|()| usage_v1());
    let envelope = run_native_rollout_teacher_v1(&source).unwrap_or_else(|error| {
        eprintln!("native rollout teacher failed: {error}");
        std::process::exit(1);
    });
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .unwrap_or_else(|error| {
            eprintln!("native rollout teacher output create failed: {error}");
            std::process::exit(1);
        });
    let mut writer = BufWriter::new(file);
    let report_bytes =
        native_rollout_teacher_report_bytes_v1(&envelope.report).unwrap_or_else(|error| {
            eprintln!("native rollout teacher output encode failed: {error}");
            std::process::exit(1);
        });
    writer.write_all(&report_bytes).unwrap_or_else(|error| {
        eprintln!("native rollout teacher output write failed: {error}");
        std::process::exit(1);
    });
    writer.flush().unwrap_or_else(|error| {
        eprintln!("native rollout teacher output flush failed: {error}");
        std::process::exit(1);
    });
    println!(
        "{} {} {}",
        envelope.deterministic_report_sha256, envelope.elapsed_milliseconds, envelope.disposition
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_order_independent_cli_v1() {
        let (source, output) = parse_args_v1(vec![
            "--output-json".into(),
            "report.json".into(),
            "--source-outcome-root".into(),
            "source".into(),
        ])
        .unwrap();
        assert_eq!(source, PathBuf::from("source"));
        assert_eq!(output, PathBuf::from("report.json"));
        assert!(parse_args_v1(vec![
            "--source-outcome-root".into(),
            "source".into(),
            "--source-outcome-root".into(),
            "other".into(),
        ])
        .is_err());
    }
}
