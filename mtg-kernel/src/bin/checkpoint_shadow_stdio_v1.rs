use mtg_kernel::native_checkpoint_shadow_stdio_v1::{
    run_checkpoint_shadow_stdio_v1, run_checkpoint_shadow_stdio_with_xmage_cp7_exports_jsonl_v1,
    run_checkpoint_shadow_stdio_with_xmage_cp7_outcome_jsonl_v1,
    run_checkpoint_shadow_stdio_with_xmage_cp7_teacher_jsonl_v1, ShadowCheckpointAuthorityV1,
};
use std::ffi::OsString;
use std::path::PathBuf;

fn usage_v1() -> ! {
    eprintln!(
        "usage: checkpoint_shadow_stdio_v1 (--original-store-root PATH [--generation N] | --population-store-root PATH --generation N | --portable-derivative-root PATH | --cp7-behavior-clone-root PATH | --xmage-cp7-outcome-root PATH) [--xmage-cp7-teacher-jsonl PATH] [--xmage-cp7-outcome-jsonl PATH]"
    );
    std::process::exit(2);
}

enum AuthorityRootV1 {
    Original(PathBuf),
    Population(PathBuf),
    Portable(PathBuf),
    Cp7BehaviorClone(PathBuf),
    XmageCp7Outcome(PathBuf),
}

fn parse_args_v1(
    raw: Vec<OsString>,
) -> Result<
    (
        ShadowCheckpointAuthorityV1,
        Option<PathBuf>,
        Option<PathBuf>,
    ),
    (),
> {
    if raw.len() < 2 || raw.len() > 8 || !raw.len().is_multiple_of(2) {
        return Err(());
    }
    let mut authority_root = None;
    let mut generation = None;
    let mut teacher_jsonl = None;
    let mut outcome_jsonl = None;
    for pair in raw.chunks_exact(2) {
        let flag = &pair[0];
        if flag == "--original-store-root" && authority_root.is_none() {
            authority_root = Some(AuthorityRootV1::Original(PathBuf::from(&pair[1])));
        } else if flag == "--population-store-root" && authority_root.is_none() {
            authority_root = Some(AuthorityRootV1::Population(PathBuf::from(&pair[1])));
        } else if flag == "--portable-derivative-root" && authority_root.is_none() {
            authority_root = Some(AuthorityRootV1::Portable(PathBuf::from(&pair[1])));
        } else if flag == "--cp7-behavior-clone-root" && authority_root.is_none() {
            authority_root = Some(AuthorityRootV1::Cp7BehaviorClone(PathBuf::from(&pair[1])));
        } else if flag == "--xmage-cp7-outcome-root" && authority_root.is_none() {
            authority_root = Some(AuthorityRootV1::XmageCp7Outcome(PathBuf::from(&pair[1])));
        } else if flag == "--generation" && generation.is_none() {
            generation = Some(pair[1].to_str().ok_or(())?.parse::<u64>().map_err(|_| ())?);
        } else if flag == "--xmage-cp7-teacher-jsonl" && teacher_jsonl.is_none() {
            teacher_jsonl = Some(PathBuf::from(&pair[1]));
        } else if flag == "--xmage-cp7-outcome-jsonl" && outcome_jsonl.is_none() {
            outcome_jsonl = Some(PathBuf::from(&pair[1]));
        } else {
            return Err(());
        }
    }
    let authority = match (authority_root, generation) {
        (Some(AuthorityRootV1::Original(root)), None) => {
            ShadowCheckpointAuthorityV1::OriginalPromoted2Generation384Store { root }
        }
        (Some(AuthorityRootV1::Original(root)), Some(generation)) => {
            ShadowCheckpointAuthorityV1::OriginalPromoted2StoreGeneration { root, generation }
        }
        (Some(AuthorityRootV1::Population(root)), Some(generation)) => {
            ShadowCheckpointAuthorityV1::PopulationStoreGeneration { root, generation }
        }
        (Some(AuthorityRootV1::Portable(root)), None) => {
            ShadowCheckpointAuthorityV1::PortablePromoted2WeightsGenesis { root }
        }
        (Some(AuthorityRootV1::Cp7BehaviorClone(root)), None) => {
            ShadowCheckpointAuthorityV1::Cp7BehaviorCloneDerivative { root }
        }
        (Some(AuthorityRootV1::XmageCp7Outcome(root)), None) => {
            ShadowCheckpointAuthorityV1::XmageCp7OutcomeDerivative { root }
        }
        (Some(AuthorityRootV1::Portable(_)), Some(_))
        | (Some(AuthorityRootV1::Cp7BehaviorClone(_)), Some(_))
        | (Some(AuthorityRootV1::XmageCp7Outcome(_)), Some(_))
        | (Some(AuthorityRootV1::Population(_)), None)
        | (None, _) => return Err(()),
    };
    Ok((authority, teacher_jsonl, outcome_jsonl))
}

fn main() {
    let raw = std::env::args_os().skip(1).collect();
    let (authority, teacher_jsonl, outcome_jsonl) =
        parse_args_v1(raw).unwrap_or_else(|()| usage_v1());
    let result = match (teacher_jsonl, outcome_jsonl) {
        (Some(path), None) => {
            run_checkpoint_shadow_stdio_with_xmage_cp7_teacher_jsonl_v1(authority, path)
        }
        (None, Some(path)) => {
            run_checkpoint_shadow_stdio_with_xmage_cp7_outcome_jsonl_v1(authority, path)
        }
        (None, None) => run_checkpoint_shadow_stdio_v1(authority),
        (Some(teacher), Some(outcome)) => {
            run_checkpoint_shadow_stdio_with_xmage_cp7_exports_jsonl_v1(authority, teacher, outcome)
        }
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
        let (authority, teacher_export, outcome_export) = parse_args_v1(vec![
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
        assert_eq!(teacher_export, Some(PathBuf::from("teacher.jsonl")));
        assert_eq!(outcome_export, None);
        assert!(parse_args_v1(vec![
            "--original-store-root".into(),
            "store".into(),
            "--xmage-cp7-teacher-jsonl".into(),
            "a".into(),
            "--xmage-cp7-teacher-jsonl".into(),
            "b".into(),
        ])
        .is_err());
        let (_, teacher_export, outcome_export) = parse_args_v1(vec![
            "--original-store-root".into(),
            "store".into(),
            "--xmage-cp7-teacher-jsonl".into(),
            "teacher.jsonl".into(),
            "--xmage-cp7-outcome-jsonl".into(),
            "outcome.jsonl".into(),
        ])
        .unwrap();
        assert_eq!(teacher_export, Some(PathBuf::from("teacher.jsonl")));
        assert_eq!(outcome_export, Some(PathBuf::from("outcome.jsonl")));

        let (_, teacher_export, outcome_export) = parse_args_v1(vec![
            "--xmage-cp7-outcome-jsonl".into(),
            "outcome.jsonl".into(),
            "--original-store-root".into(),
            "store".into(),
        ])
        .unwrap();
        assert_eq!(teacher_export, None);
        assert_eq!(outcome_export, Some(PathBuf::from("outcome.jsonl")));
    }

    #[test]
    fn generation_is_original_store_only_and_keeps_g384_default_v1() {
        let (default_authority, teacher_export, outcome_export) =
            parse_args_v1(vec!["--original-store-root".into(), "store".into()]).unwrap();
        assert_eq!(teacher_export, None);
        assert_eq!(outcome_export, None);
        assert!(matches!(
            default_authority,
            ShadowCheckpointAuthorityV1::OriginalPromoted2Generation384Store { .. }
        ));

        let (selected_authority, teacher_export, outcome_export) = parse_args_v1(vec![
            "--xmage-cp7-teacher-jsonl".into(),
            "teacher.jsonl".into(),
            "--generation".into(),
            "256".into(),
            "--original-store-root".into(),
            "store".into(),
        ])
        .unwrap();
        assert_eq!(teacher_export, Some(PathBuf::from("teacher.jsonl")));
        assert_eq!(outcome_export, None);
        assert!(matches!(
            selected_authority,
            ShadowCheckpointAuthorityV1::OriginalPromoted2StoreGeneration {
                generation: 256,
                ..
            }
        ));

        assert!(parse_args_v1(vec![
            "--portable-derivative-root".into(),
            "portable".into(),
            "--generation".into(),
            "256".into(),
        ])
        .is_err());

        let (population, teacher_export, outcome_export) = parse_args_v1(vec![
            "--population-store-root".into(),
            "population-store".into(),
            "--generation".into(),
            "1024".into(),
        ])
        .unwrap();
        assert_eq!(teacher_export, None);
        assert_eq!(outcome_export, None);
        assert!(matches!(
            population,
            ShadowCheckpointAuthorityV1::PopulationStoreGeneration {
                generation: 1024,
                ..
            }
        ));
        assert!(parse_args_v1(vec![
            "--population-store-root".into(),
            "population-store".into(),
        ])
        .is_err());
        assert!(parse_args_v1(vec![
            "--population-store-root".into(),
            "population-store".into(),
            "--generation".into(),
            "1024".into(),
            "--original-store-root".into(),
            "original-store".into(),
        ])
        .is_err());
        assert!(parse_args_v1(vec![
            "--original-store-root".into(),
            "store".into(),
            "--generation".into(),
            "not-a-generation".into(),
        ])
        .is_err());

        let (derivative, teacher_export, outcome_export) = parse_args_v1(vec![
            "--cp7-behavior-clone-root".into(),
            "cp7-derivative".into(),
        ])
        .unwrap();
        assert_eq!(teacher_export, None);
        assert_eq!(outcome_export, None);
        assert!(matches!(
            derivative,
            ShadowCheckpointAuthorityV1::Cp7BehaviorCloneDerivative { .. }
        ));
        assert!(parse_args_v1(vec![
            "--cp7-behavior-clone-root".into(),
            "cp7-derivative".into(),
            "--generation".into(),
            "1".into(),
        ])
        .is_err());

        let (outcome, teacher_export, outcome_export) = parse_args_v1(vec![
            "--xmage-cp7-outcome-root".into(),
            "outcome-derivative".into(),
        ])
        .unwrap();
        assert_eq!(teacher_export, None);
        assert_eq!(outcome_export, None);
        assert!(matches!(
            outcome,
            ShadowCheckpointAuthorityV1::XmageCp7OutcomeDerivative { .. }
        ));
        assert!(parse_args_v1(vec![
            "--xmage-cp7-outcome-root".into(),
            "outcome-derivative".into(),
            "--generation".into(),
            "1".into(),
        ])
        .is_err());
    }
}
