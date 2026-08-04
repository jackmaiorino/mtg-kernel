use mtg_kernel::native_checkpoint_shadow_stdio_v1::
    run_checkpoint_shadow_stdio_with_recurrent_native_population_exports_jsonl_v1;
use std::ffi::OsString;
use std::path::PathBuf;

fn usage_v1() -> ! {
    eprintln!(
        "usage: native_recurrent_population_corpus_stdio_v1 --recurrent-root PATH --python PATH --pool-root PATH --teacher-jsonl PATH --outcome-jsonl PATH"
    );
    std::process::exit(2);
}

fn parse_args_v1(
    raw: Vec<OsString>,
) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf, PathBuf), ()> {
    if raw.len() != 10 {
        return Err(());
    }
    let mut recurrent_root = None;
    let mut python = None;
    let mut pool_root = None;
    let mut teacher_jsonl = None;
    let mut outcome_jsonl = None;
    for pair in raw.chunks_exact(2) {
        if pair[0] == "--recurrent-root" && recurrent_root.is_none() {
            recurrent_root = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--python" && python.is_none() {
            python = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--pool-root" && pool_root.is_none() {
            pool_root = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--teacher-jsonl" && teacher_jsonl.is_none() {
            teacher_jsonl = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--outcome-jsonl" && outcome_jsonl.is_none() {
            outcome_jsonl = Some(PathBuf::from(&pair[1]));
        } else {
            return Err(());
        }
    }
    Ok((
        recurrent_root.ok_or(())?,
        python.ok_or(())?,
        pool_root.ok_or(())?,
        teacher_jsonl.ok_or(())?,
        outcome_jsonl.ok_or(())?,
    ))
}

fn main() {
    let (root, python, pool, teacher, outcome) =
        parse_args_v1(std::env::args_os().skip(1).collect())
            .unwrap_or_else(|()| usage_v1());
    if let Err(error) =
        run_checkpoint_shadow_stdio_with_recurrent_native_population_exports_jsonl_v1(
            root, python, pool, teacher, outcome,
        )
    {
        eprintln!("native recurrent population scorer failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_exact_v1() {
        assert!(parse_args_v1(Vec::new()).is_err());
        assert!(parse_args_v1(vec!["--wrong".into(); 10]).is_err());
        assert_eq!(
            parse_args_v1(vec![
                "--outcome-jsonl".into(),
                "outcome.jsonl".into(),
                "--recurrent-root".into(),
                "candidate".into(),
                "--pool-root".into(),
                "pool".into(),
                "--python".into(),
                "python.exe".into(),
                "--teacher-jsonl".into(),
                "teacher.jsonl".into(),
            ]),
            Ok((
                PathBuf::from("candidate"),
                PathBuf::from("python.exe"),
                PathBuf::from("pool"),
                PathBuf::from("teacher.jsonl"),
                PathBuf::from("outcome.jsonl"),
            ))
        );
    }
}
