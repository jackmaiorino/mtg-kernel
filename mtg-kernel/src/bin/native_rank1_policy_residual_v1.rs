use mtg_kernel::native_bilinear_policy_residual_v1::run_native_rank1_policy_residual_v1;
use std::ffi::OsString;
use std::path::PathBuf;

fn usage_v1() -> ! {
    eprintln!(
        "usage: native_rank1_policy_residual_v1 --source-outcome-root PATH --outcome-jsonl PATH --output-dir NEW_PATH"
    );
    std::process::exit(2);
}

fn parse_args_v1(raw: Vec<OsString>) -> Result<(PathBuf, PathBuf, PathBuf), ()> {
    if raw.len() != 6 {
        return Err(());
    }
    let mut source = None;
    let mut outcome = None;
    let mut output = None;
    for pair in raw.chunks_exact(2) {
        if pair[0] == "--source-outcome-root" && source.is_none() {
            source = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--outcome-jsonl" && outcome.is_none() {
            outcome = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--output-dir" && output.is_none() {
            output = Some(PathBuf::from(&pair[1]));
        } else {
            return Err(());
        }
    }
    match (source, outcome, output) {
        (Some(source), Some(outcome), Some(output)) => Ok((source, outcome, output)),
        _ => Err(()),
    }
}

fn main() {
    let (source, outcome, output) =
        parse_args_v1(std::env::args_os().skip(1).collect()).unwrap_or_else(|()| usage_v1());
    let envelope =
        run_native_rank1_policy_residual_v1(&source, &outcome, &output).unwrap_or_else(|error| {
            eprintln!("native rank1 policy residual failed: {error}");
            std::process::exit(1);
        });
    println!(
        "{} {} {} {} {}",
        envelope.candidate_json_sha256_v1(),
        envelope.report_sha256_v1(),
        envelope.weights_sha256_v1(),
        envelope.elapsed_milliseconds_v1(),
        envelope.disposition_v1()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_order_independent_cli_v1() {
        let (source, outcome, output) = parse_args_v1(vec![
            "--output-dir".into(),
            "candidate".into(),
            "--outcome-jsonl".into(),
            "outcomes.jsonl".into(),
            "--source-outcome-root".into(),
            "parent".into(),
        ])
        .unwrap();
        assert_eq!(source, PathBuf::from("parent"));
        assert_eq!(outcome, PathBuf::from("outcomes.jsonl"));
        assert_eq!(output, PathBuf::from("candidate"));
    }
}
