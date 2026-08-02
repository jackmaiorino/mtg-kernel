use mtg_kernel::native_cp7_behavior_clone_v1::write_structured_parent_logits_v1;
use std::ffi::OsString;
use std::path::PathBuf;

fn usage_v1() -> ! {
    eprintln!(
        "usage: structured_parent_logits_v1 --parent-outcome-root PATH --teacher-jsonl PATH --output-json PATH"
    );
    std::process::exit(2);
}

fn parse_args_v1(raw: Vec<OsString>) -> Result<(PathBuf, PathBuf, PathBuf), ()> {
    if raw.len() != 6 {
        return Err(());
    }
    let mut parent = None;
    let mut teacher = None;
    let mut output = None;
    for pair in raw.chunks_exact(2) {
        if pair[0] == "--parent-outcome-root" && parent.is_none() {
            parent = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--teacher-jsonl" && teacher.is_none() {
            teacher = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--output-json" && output.is_none() {
            output = Some(PathBuf::from(&pair[1]));
        } else {
            return Err(());
        }
    }
    match (parent, teacher, output) {
        (Some(parent), Some(teacher), Some(output)) => Ok((parent, teacher, output)),
        _ => Err(()),
    }
}

fn main() {
    let (parent, teacher, output) =
        parse_args_v1(std::env::args_os().skip(1).collect()).unwrap_or_else(|()| usage_v1());
    write_structured_parent_logits_v1(parent, teacher, output).unwrap_or_else(|error| {
        eprintln!("structured parent-logit export failed: {error}");
        std::process::exit(1);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_order_independent_cli_v1() {
        let parsed = parse_args_v1(vec![
            "--output-json".into(),
            "output.json".into(),
            "--teacher-jsonl".into(),
            "teacher.jsonl".into(),
            "--parent-outcome-root".into(),
            "parent".into(),
        ])
        .unwrap();
        assert_eq!(parsed.0, PathBuf::from("parent"));
        assert_eq!(parsed.1, PathBuf::from("teacher.jsonl"));
        assert_eq!(parsed.2, PathBuf::from("output.json"));
    }
}
