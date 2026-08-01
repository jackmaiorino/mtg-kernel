use mtg_kernel::native_checkpoint_shadow_stdio_v1::{
    run_checkpoint_shadow_stdio_v1, run_checkpoint_shadow_stdio_with_xmage_cp7_teacher_jsonl_v1,
    ShadowCheckpointAuthorityV1,
};
use std::ffi::OsString;
use std::path::PathBuf;

fn usage_v1() -> ! {
    eprintln!(
        "usage: checkpoint_shadow_stdio_v1 (--original-store-root PATH | --portable-derivative-root PATH) [--xmage-cp7-teacher-jsonl PATH]"
    );
    std::process::exit(2);
}

fn parse_args_v1(raw: Vec<OsString>) -> Result<(ShadowCheckpointAuthorityV1, Option<PathBuf>), ()> {
    if raw.len() != 2 && raw.len() != 4 {
        return Err(());
    }
    let mut authority = None;
    let mut teacher_jsonl = None;
    for pair in raw.chunks_exact(2) {
        let flag = &pair[0];
        let value = PathBuf::from(&pair[1]);
        if flag == "--original-store-root" && authority.is_none() {
            authority = Some(
                ShadowCheckpointAuthorityV1::OriginalPromoted2Generation384Store { root: value },
            );
        } else if flag == "--portable-derivative-root" && authority.is_none() {
            authority =
                Some(ShadowCheckpointAuthorityV1::PortablePromoted2WeightsGenesis { root: value });
        } else if flag == "--xmage-cp7-teacher-jsonl" && teacher_jsonl.is_none() {
            teacher_jsonl = Some(value);
        } else {
            return Err(());
        }
    }
    authority
        .map(|authority| (authority, teacher_jsonl))
        .ok_or(())
}

fn main() {
    let raw = std::env::args_os().skip(1).collect();
    let (authority, teacher_jsonl) = parse_args_v1(raw).unwrap_or_else(|()| usage_v1());
    let result = match teacher_jsonl {
        Some(path) => run_checkpoint_shadow_stdio_with_xmage_cp7_teacher_jsonl_v1(authority, path),
        None => run_checkpoint_shadow_stdio_v1(authority),
    };
    if let Err(error) = result {
        eprintln!("checkpoint shadow scorer failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_teacher_export_is_strict_and_order_independent_v1() {
        let (authority, export) = parse_args_v1(vec![
            "--xmage-cp7-teacher-jsonl".into(),
            "teacher.jsonl".into(),
            "--original-store-root".into(),
            "store".into(),
        ])
        .unwrap();
        assert!(matches!(
            authority,
            ShadowCheckpointAuthorityV1::OriginalPromoted2Generation384Store { .. }
        ));
        assert_eq!(export, Some(PathBuf::from("teacher.jsonl")));
        assert!(parse_args_v1(vec![
            "--original-store-root".into(),
            "store".into(),
            "--xmage-cp7-teacher-jsonl".into(),
            "a".into(),
            "--xmage-cp7-teacher-jsonl".into(),
            "b".into(),
        ])
        .is_err());
    }
}
