use mtg_kernel::native_checkpoint_shadow_stdio_v1::{
    ShadowCandidateSelectorV1, ShadowCheckpointAuthorityV1,
    run_checkpoint_shadow_stdio_with_selector_v1,
};
use std::ffi::OsString;
use std::path::PathBuf;

fn usage_v1() -> ! {
    eprintln!(
        "usage: checkpoint_shadow_bounded_value_search_stdio_v1 --xmage-cp7-outcome-root PATH"
    );
    std::process::exit(2);
}

fn parse_args_v1(raw: Vec<OsString>) -> Result<PathBuf, ()> {
    if raw.len() == 2 && raw[0] == "--xmage-cp7-outcome-root" {
        Ok(PathBuf::from(&raw[1]))
    } else {
        Err(())
    }
}

fn main() {
    let root = parse_args_v1(std::env::args_os().skip(1).collect()).unwrap_or_else(|()| usage_v1());
    let authority = ShadowCheckpointAuthorityV1::XmageCp7OutcomeDerivative { root };
    if let Err(error) = run_checkpoint_shadow_stdio_with_selector_v1(
        authority,
        ShadowCandidateSelectorV1::CandidateTurnOnlyOneStepBoundedValueBootstrap,
    ) {
        eprintln!("checkpoint bounded-value search shadow scorer failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_one_explicit_search_root_v1() {
        assert_eq!(
            parse_args_v1(vec!["--xmage-cp7-outcome-root".into(), "search".into()]).unwrap(),
            PathBuf::from("search")
        );
        assert!(parse_args_v1(vec!["--policy-root".into(), "policy".into()]).is_err());
    }
}
