use mtg_kernel::native_checkpoint_shadow_stdio_v1::{
    run_checkpoint_shadow_stdio_with_native_population_exports_jsonl_v1,
    ShadowCheckpointAuthorityV1,
};
use std::path::PathBuf;

fn usage_v1() -> ! {
    eprintln!(
        "usage: native_population_corpus_stdio_v1 --candidate-outcome-root PATH --pool-root PATH --teacher-jsonl PATH --outcome-jsonl PATH"
    );
    std::process::exit(2);
}

fn main() {
    let raw = std::env::args_os().skip(1).collect::<Vec<_>>();
    if raw.len() != 8 {
        usage_v1();
    }
    let mut candidate_root = None;
    let mut pool_root = None;
    let mut teacher_jsonl = None;
    let mut outcome_jsonl = None;
    for pair in raw.chunks_exact(2) {
        if pair[0] == "--candidate-outcome-root" && candidate_root.is_none() {
            candidate_root = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--pool-root" && pool_root.is_none() {
            pool_root = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--teacher-jsonl" && teacher_jsonl.is_none() {
            teacher_jsonl = Some(PathBuf::from(&pair[1]));
        } else if pair[0] == "--outcome-jsonl" && outcome_jsonl.is_none() {
            outcome_jsonl = Some(PathBuf::from(&pair[1]));
        } else {
            usage_v1();
        }
    }
    let authority = ShadowCheckpointAuthorityV1::XmageCp7OutcomeDerivative {
        root: candidate_root.unwrap_or_else(|| usage_v1()),
    };
    if let Err(error) = run_checkpoint_shadow_stdio_with_native_population_exports_jsonl_v1(
        authority,
        pool_root.unwrap_or_else(|| usage_v1()),
        teacher_jsonl.unwrap_or_else(|| usage_v1()),
        outcome_jsonl.unwrap_or_else(|| usage_v1()),
    ) {
        eprintln!("native population corpus scorer failed: {error}");
        std::process::exit(1);
    }
}
