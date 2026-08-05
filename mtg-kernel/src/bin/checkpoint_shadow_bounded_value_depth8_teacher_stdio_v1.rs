use mtg_kernel::native_checkpoint_shadow_stdio_v1::{
    run_checkpoint_shadow_stdio_with_depth8_teacher_exports_jsonl_v1, ShadowCheckpointAuthorityV1,
};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Eq, PartialEq)]
struct ArgsV1 {
    root: PathBuf,
    opponent_teacher_jsonl: PathBuf,
    outcome_jsonl: PathBuf,
}

fn usage_v1() -> ! {
    eprintln!(
        "usage: checkpoint_shadow_bounded_value_depth8_teacher_stdio_v1 --xmage-cp7-outcome-root PATH --xmage-cp7-teacher-jsonl PATH --xmage-cp7-outcome-jsonl PATH"
    );
    std::process::exit(2);
}

fn parse_args_v1(raw: Vec<OsString>) -> Result<ArgsV1, ()> {
    if raw.len() != 6 {
        return Err(());
    }
    let mut root = None;
    let mut opponent_teacher_jsonl = None;
    let mut outcome_jsonl = None;
    for pair in raw.chunks_exact(2) {
        if pair[0] == "--xmage-cp7-outcome-root" && root.is_none() {
            root = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--xmage-cp7-teacher-jsonl" && opponent_teacher_jsonl.is_none() {
            opponent_teacher_jsonl = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--xmage-cp7-outcome-jsonl" && outcome_jsonl.is_none() {
            outcome_jsonl = Some(PathBuf::from(&pair[1]));
        } else {
            return Err(());
        }
    }
    Ok(ArgsV1 {
        root: root.ok_or(())?,
        opponent_teacher_jsonl: opponent_teacher_jsonl.ok_or(())?,
        outcome_jsonl: outcome_jsonl.ok_or(())?,
    })
}

fn main() {
    let args = parse_args_v1(std::env::args_os().skip(1).collect()).unwrap_or_else(|()| usage_v1());
    let authority = ShadowCheckpointAuthorityV1::XmageCp7OutcomeDerivative { root: args.root };
    if let Err(error) = run_checkpoint_shadow_stdio_with_depth8_teacher_exports_jsonl_v1(
        authority,
        args.opponent_teacher_jsonl,
        args.outcome_jsonl,
    ) {
        eprintln!("checkpoint bounded-value depth-8 teacher scorer failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_one_root_and_both_exports_v1() {
        let parsed = parse_args_v1(vec![
            "--xmage-cp7-outcome-jsonl".into(),
            "outcome.jsonl".into(),
            "--xmage-cp7-outcome-root".into(),
            "candidate".into(),
            "--xmage-cp7-teacher-jsonl".into(),
            "opponent.jsonl".into(),
        ])
        .unwrap();
        assert_eq!(
            parsed,
            ArgsV1 {
                root: PathBuf::from("candidate"),
                opponent_teacher_jsonl: PathBuf::from("opponent.jsonl"),
                outcome_jsonl: PathBuf::from("outcome.jsonl"),
            }
        );
        assert!(
            parse_args_v1(vec!["--xmage-cp7-outcome-root".into(), "candidate".into(),]).is_err()
        );
    }
}
