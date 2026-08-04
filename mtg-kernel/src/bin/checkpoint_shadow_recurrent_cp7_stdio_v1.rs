use mtg_kernel::native_checkpoint_shadow_stdio_v1::{
    run_checkpoint_shadow_stdio_with_recurrent_cp7_exports_jsonl_v1,
    run_checkpoint_shadow_stdio_with_recurrent_cp7_outcome_jsonl_v1,
    run_checkpoint_shadow_stdio_with_recurrent_cp7_v1,
};
use std::ffi::OsString;
use std::path::PathBuf;

fn usage_v1() -> ! {
    eprintln!(
        "usage: checkpoint_shadow_recurrent_cp7_stdio_v1 --xmage-cp7-outcome-root PATH [--xmage-cp7-teacher-jsonl PATH --xmage-cp7-outcome-jsonl PATH]"
    );
    std::process::exit(2);
}

fn parse_args_v1(
    raw: Vec<OsString>,
) -> Result<(PathBuf, Option<PathBuf>, Option<PathBuf>), ()> {
    if !matches!(raw.len(), 2 | 4 | 6) {
        return Err(());
    }
    let mut root = None;
    let mut teacher = None;
    let mut outcome = None;
    for pair in raw.chunks_exact(2) {
        if pair[0] == "--xmage-cp7-outcome-root" && root.is_none() {
            root = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--xmage-cp7-teacher-jsonl" && teacher.is_none() {
            teacher = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--xmage-cp7-outcome-jsonl" && outcome.is_none() {
            outcome = Some(PathBuf::from(&pair[1]));
        } else {
            return Err(());
        }
    }
    if teacher.is_some() && outcome.is_none() {
        return Err(());
    }
    Ok((root.ok_or(())?, teacher, outcome))
}

fn main() {
    let (root, teacher, outcome) = parse_args_v1(std::env::args_os().skip(1).collect())
        .unwrap_or_else(|()| usage_v1());
    let python = std::env::var_os("MTG_KERNEL_RECURRENT_CP7_PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            eprintln!("MTG_KERNEL_RECURRENT_CP7_PYTHON is required");
            std::process::exit(2);
        });
    let result = match (teacher, outcome) {
        (Some(teacher), Some(outcome)) => {
            run_checkpoint_shadow_stdio_with_recurrent_cp7_exports_jsonl_v1(
                root, python, teacher, outcome,
            )
        }
        (None, Some(outcome)) => {
            run_checkpoint_shadow_stdio_with_recurrent_cp7_outcome_jsonl_v1(
                root, python, outcome,
            )
        }
        (None, None) => run_checkpoint_shadow_stdio_with_recurrent_cp7_v1(root, python),
        (Some(_), None) => unreachable!(),
    };
    if let Err(error) = result {
        eprintln!("recurrent CP7 shadow scorer failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_exact_v1() {
        assert_eq!(
            parse_args_v1(vec!["--xmage-cp7-outcome-root".into(), "root".into()]),
            Ok((PathBuf::from("root"), None, None))
        );
        assert_eq!(
            parse_args_v1(vec![
                "--xmage-cp7-outcome-jsonl".into(),
                "outcome.jsonl".into(),
                "--xmage-cp7-outcome-root".into(),
                "root".into(),
            ]),
            Ok((
                PathBuf::from("root"),
                None,
                Some(PathBuf::from("outcome.jsonl")),
            ))
        );
        assert_eq!(
            parse_args_v1(vec![
                "--xmage-cp7-teacher-jsonl".into(),
                "teacher.jsonl".into(),
                "--xmage-cp7-outcome-jsonl".into(),
                "outcome.jsonl".into(),
                "--xmage-cp7-outcome-root".into(),
                "root".into(),
            ]),
            Ok((
                PathBuf::from("root"),
                Some(PathBuf::from("teacher.jsonl")),
                Some(PathBuf::from("outcome.jsonl")),
            ))
        );
        assert!(parse_args_v1(vec!["--wrong".into(), "root".into()]).is_err());
        assert!(parse_args_v1(Vec::new()).is_err());
    }
}
