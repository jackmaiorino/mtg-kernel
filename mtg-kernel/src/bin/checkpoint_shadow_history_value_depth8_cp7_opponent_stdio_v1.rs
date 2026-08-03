use mtg_kernel::native_checkpoint_shadow_stdio_v1::{
    run_checkpoint_shadow_stdio_with_cp7_opponent_selector_v1, ShadowCheckpointAuthorityV1,
};
use std::ffi::OsString;
use std::path::PathBuf;

fn usage_v1() -> ! {
    eprintln!(
        "usage: set MTG_KERNEL_CP7_OPPONENT_ROOT, then run checkpoint_shadow_history_value_depth8_cp7_opponent_stdio_v1 --xmage-cp7-outcome-root PATH"
    );
    std::process::exit(2);
}

fn parse_args_v1(raw: Vec<OsString>, opponent: Option<OsString>) -> Result<(PathBuf, PathBuf), ()> {
    if raw.len() != 2 || raw[0] != "--xmage-cp7-outcome-root" {
        return Err(());
    }
    Ok((
        PathBuf::from(&raw[1]),
        opponent.map(PathBuf::from).ok_or(())?,
    ))
}

fn main() {
    let (root, opponent) = parse_args_v1(
        std::env::args_os().skip(1).collect(),
        std::env::var_os("MTG_KERNEL_CP7_OPPONENT_ROOT"),
    )
    .unwrap_or_else(|()| usage_v1());
    let authority = ShadowCheckpointAuthorityV1::XmageCp7OutcomeDerivative { root };
    if let Err(error) =
        run_checkpoint_shadow_stdio_with_cp7_opponent_selector_v1(authority, opponent)
    {
        eprintln!("checkpoint CP7-opponent depth-8 shadow scorer failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_explicit_candidate_and_opponent_roots_v1() {
        let raw = vec!["--xmage-cp7-outcome-root".into(), "candidate".into()];
        let parsed = parse_args_v1(raw.clone(), Some("cp7-clone".into())).unwrap();
        assert_eq!(
            parsed,
            (PathBuf::from("candidate"), PathBuf::from("cp7-clone"))
        );
        assert!(parse_args_v1(raw, None).is_err());
    }
}
