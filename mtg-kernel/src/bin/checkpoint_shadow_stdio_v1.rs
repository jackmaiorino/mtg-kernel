use mtg_kernel::kernel_native_search_opponent_v1::KernelNativeSearchTierV1;
use mtg_kernel::native_checkpoint_shadow_stdio_v1::{
    run_checkpoint_shadow_stdio_with_model_guided_search_v1,
    run_checkpoint_shadow_stdio_with_protocol_and_exports_v1, ShadowCheckpointAuthorityV1,
    ShadowStdioProtocolV1,
};
use std::ffi::OsString;
use std::path::PathBuf;

fn usage_v1() -> ! {
    eprintln!(
        "usage: checkpoint_shadow_stdio_v1 (--original-store-root PATH [--generation N] | --population-store-root PATH --generation N | --portable-derivative-root PATH | --cp7-behavior-clone-root PATH | --xmage-cp7-outcome-root PATH) ([--xmage-cp7-teacher-jsonl PATH] [--xmage-cp7-outcome-jsonl PATH] | --model-guided-search-tier (t512|t2048|t8192|t32768) --model-guided-search-seed-block N --model-guided-search-diagnostics-dir PATH [--model-guided-search-stability-halves (on|off)]) [--protocol v1|v2]"
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

/// The three flags that together select the test-time-search wrapper
/// (`LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md` Section 5, S0: "selectable
/// ONLY through strict CLI flags on the scorer bin (no environment
/// variables)"). All three or none: a partial specification is a usage
/// error rather than a silently defaulted tier, seed block, or output
/// path, each of which would be a pre-registered constant chosen by
/// accident.
#[derive(Debug, Eq, PartialEq)]
struct ModelGuidedSearchArgsV1 {
    tier: KernelNativeSearchTierV1,
    seed_block_id: usize,
    diagnostics_directory: PathBuf,
    /// Whether the two diagnostic stability halves run inside each
    /// decision. Defaults to ON, which is the S2-diagnostics
    /// configuration and the one every existing record was produced
    /// under; a formal panel measuring product latency passes `off`.
    ///
    /// Spelled with an explicit `on`/`off` value rather than as a bare
    /// `--disable-...` switch so a command line always states the
    /// configuration it ran under, and a reviewer reading a launch
    /// invocation never has to know the default to know what happened.
    stability_halves_enabled: bool,
}

#[derive(Debug, Eq, PartialEq)]
struct ParsedArgsV1 {
    authority: ShadowCheckpointAuthorityV1,
    teacher_jsonl: Option<PathBuf>,
    outcome_jsonl: Option<PathBuf>,
    model_guided_search: Option<ModelGuidedSearchArgsV1>,
    /// The stdio wire version to serve. Defaults to V1: the pinned scorer
    /// executables and the Java bridge codec (which rejects unknown
    /// fields) only speak V1.
    protocol: ShadowStdioProtocolV1,
}

/// The tier ladder, spelled exactly as the sketch pre-registers it
/// (Section 4: "Ladder: T in {512, 2048, 8192, 32768}"). No alias, no
/// numeric form, no case folding: a tier is a pre-registered constant, and
/// a typo must be a usage error, never a neighbouring tier.
fn parse_tier_v1(value: &OsString) -> Result<KernelNativeSearchTierV1, ()> {
    match value.to_str().ok_or(())? {
        "t512" => Ok(KernelNativeSearchTierV1::T512),
        "t2048" => Ok(KernelNativeSearchTierV1::T2048),
        "t8192" => Ok(KernelNativeSearchTierV1::T8192),
        "t32768" => Ok(KernelNativeSearchTierV1::T32768),
        _ => Err(()),
    }
}

fn parse_args_v1(raw: Vec<OsString>) -> Result<ParsedArgsV1, ()> {
    if raw.len() < 2 || raw.len() > 18 || !raw.len().is_multiple_of(2) {
        return Err(());
    }
    let mut authority_root = None;
    let mut generation = None;
    let mut teacher_jsonl = None;
    let mut outcome_jsonl = None;
    let mut search_tier = None;
    let mut search_seed_block = None;
    let mut search_diagnostics_dir = None;
    let mut search_stability_halves = None;
    let mut protocol = None;
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
        } else if flag == "--model-guided-search-tier" && search_tier.is_none() {
            search_tier = Some(parse_tier_v1(&pair[1])?);
        } else if flag == "--model-guided-search-seed-block" && search_seed_block.is_none() {
            search_seed_block = Some(
                pair[1]
                    .to_str()
                    .ok_or(())?
                    .parse::<usize>()
                    .map_err(|_| ())?,
            );
        } else if flag == "--model-guided-search-diagnostics-dir"
            && search_diagnostics_dir.is_none()
        {
            search_diagnostics_dir = Some(PathBuf::from(&pair[1]));
        } else if flag == "--model-guided-search-stability-halves"
            && search_stability_halves.is_none()
        {
            search_stability_halves = Some(match pair[1].to_str().ok_or(())? {
                "on" => true,
                "off" => false,
                _ => return Err(()),
            });
        } else if flag == "--protocol" && protocol.is_none() {
            protocol = Some(match pair[1].to_str().ok_or(())? {
                "v1" => ShadowStdioProtocolV1::V1,
                "v2" => ShadowStdioProtocolV1::V2,
                _ => return Err(()),
            });
        } else {
            return Err(());
        }
    }
    let model_guided_search = match (search_tier, search_seed_block, search_diagnostics_dir) {
        (Some(tier), Some(seed_block_id), Some(diagnostics_directory)) => {
            Some(ModelGuidedSearchArgsV1 {
                tier,
                seed_block_id,
                diagnostics_directory,
                stability_halves_enabled: search_stability_halves.unwrap_or(true),
            })
        }
        // The stability-halves flag is a MODIFIER of the wrapper, never a
        // way to reach it: on its own it selects nothing, so passing it
        // without the three required flags is a usage error rather than a
        // silently ignored argument.
        (None, None, None) if search_stability_halves.is_none() => None,
        // Any partial combination is a usage error.
        _ => return Err(()),
    };
    // The wrapper and the trajectory exports are mutually exclusive: those
    // export schemas record `selected_action_index` as a direct
    // checkpoint-policy sample, which a search-chosen action is not. The
    // library entry point rejects this too; catching it here turns a
    // startup failure into a usage message.
    if model_guided_search.is_some() && (teacher_jsonl.is_some() || outcome_jsonl.is_some()) {
        return Err(());
    }
    // The wrapper serves the frozen V1 wire only: its entry point takes no
    // protocol, so accepting `--protocol v2` beside it would silently drop
    // the flag and leave a launcher believing it had negotiated V2. An
    // explicit `--protocol v1` beside the wrapper is redundant but honest,
    // and stays allowed.
    if model_guided_search.is_some() && protocol == Some(ShadowStdioProtocolV1::V2) {
        return Err(());
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
    Ok(ParsedArgsV1 {
        authority,
        teacher_jsonl,
        outcome_jsonl,
        model_guided_search,
        protocol: protocol.unwrap_or_default(),
    })
}

fn main() {
    let raw = std::env::args_os().skip(1).collect();
    let parsed = parse_args_v1(raw).unwrap_or_else(|()| usage_v1());
    let result = match parsed.model_guided_search {
        Some(search) => run_checkpoint_shadow_stdio_with_model_guided_search_v1(
            parsed.authority,
            search.tier,
            search.seed_block_id,
            search.diagnostics_directory,
            search.stability_halves_enabled,
        ),
        None => run_checkpoint_shadow_stdio_with_protocol_and_exports_v1(
            parsed.authority,
            parsed.protocol,
            parsed.teacher_jsonl,
            parsed.outcome_jsonl,
        ),
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
        let parsed = parse_args_v1(vec![
            "--xmage-cp7-teacher-jsonl".into(),
            "teacher.jsonl".into(),
            "--original-store-root".into(),
            "store".into(),
        ])
        .unwrap();
        assert!(matches!(
            parsed.authority,
            ShadowCheckpointAuthorityV1::OriginalPromoted2Generation384Store { .. }
        ));
        assert_eq!(parsed.teacher_jsonl, Some(PathBuf::from("teacher.jsonl")));
        assert_eq!(parsed.outcome_jsonl, None);
        assert_eq!(parsed.model_guided_search, None);
        assert_eq!(
            parsed.protocol,
            ShadowStdioProtocolV1::V1,
            "protocol defaults to v1"
        );
        assert!(parse_args_v1(vec![
            "--original-store-root".into(),
            "store".into(),
            "--xmage-cp7-teacher-jsonl".into(),
            "a".into(),
            "--xmage-cp7-teacher-jsonl".into(),
            "b".into(),
        ])
        .is_err());
        let parsed = parse_args_v1(vec![
            "--original-store-root".into(),
            "store".into(),
            "--xmage-cp7-teacher-jsonl".into(),
            "teacher.jsonl".into(),
            "--xmage-cp7-outcome-jsonl".into(),
            "outcome.jsonl".into(),
        ])
        .unwrap();
        assert_eq!(parsed.teacher_jsonl, Some(PathBuf::from("teacher.jsonl")));
        assert_eq!(parsed.outcome_jsonl, Some(PathBuf::from("outcome.jsonl")));

        let parsed = parse_args_v1(vec![
            "--xmage-cp7-outcome-jsonl".into(),
            "outcome.jsonl".into(),
            "--original-store-root".into(),
            "store".into(),
        ])
        .unwrap();
        assert_eq!(parsed.teacher_jsonl, None);
        assert_eq!(parsed.outcome_jsonl, Some(PathBuf::from("outcome.jsonl")));
    }

    #[test]
    fn protocol_flag_is_opt_in_strict_and_order_independent_v1() {
        // The default must stay V1: the pinned scorer executables and the
        // Java bridge codec (which rejects unknown fields) only speak V1.
        let parsed = parse_args_v1(vec!["--original-store-root".into(), "store".into()]).unwrap();
        assert_eq!(parsed.protocol, ShadowStdioProtocolV1::V1);

        let parsed = parse_args_v1(vec![
            "--original-store-root".into(),
            "store".into(),
            "--protocol".into(),
            "v1".into(),
        ])
        .unwrap();
        assert_eq!(parsed.protocol, ShadowStdioProtocolV1::V1);

        let parsed = parse_args_v1(vec![
            "--protocol".into(),
            "v2".into(),
            "--xmage-cp7-outcome-jsonl".into(),
            "outcome.jsonl".into(),
            "--population-store-root".into(),
            "population-store".into(),
            "--generation".into(),
            "2048".into(),
        ])
        .unwrap();
        assert_eq!(parsed.protocol, ShadowStdioProtocolV1::V2);
        assert_eq!(parsed.outcome_jsonl, Some(PathBuf::from("outcome.jsonl")));

        assert!(parse_args_v1(vec![
            "--original-store-root".into(),
            "store".into(),
            "--protocol".into(),
            "v3".into(),
        ])
        .is_err());
        assert!(parse_args_v1(vec![
            "--original-store-root".into(),
            "store".into(),
            "--protocol".into(),
            "v1".into(),
            "--protocol".into(),
            "v2".into(),
        ])
        .is_err());
        assert!(parse_args_v1(vec!["--protocol".into(), "v2".into()]).is_err());
    }

    /// The wrapper entry point serves the frozen V1 wire and takes no
    /// protocol, so `--protocol v2` beside it is a usage error rather than
    /// a flag that would be accepted and then quietly dropped.
    #[test]
    fn the_wrapper_refuses_protocol_v2_and_accepts_an_explicit_v1_v1() {
        let mut argv = model_guided_search_argv_v1("t512");
        argv.push("--protocol".into());
        argv.push("v2".into());
        assert!(parse_args_v1(argv).is_err());

        let mut argv = model_guided_search_argv_v1("t512");
        argv.push("--protocol".into());
        argv.push("v1".into());
        let parsed = parse_args_v1(argv).unwrap();
        assert_eq!(parsed.protocol, ShadowStdioProtocolV1::V1);
        assert!(parsed.model_guided_search.is_some());
    }

    #[test]
    fn generation_is_original_store_only_and_keeps_g384_default_v1() {
        let parsed = parse_args_v1(vec!["--original-store-root".into(), "store".into()]).unwrap();
        assert_eq!(parsed.teacher_jsonl, None);
        assert_eq!(parsed.outcome_jsonl, None);
        assert_eq!(parsed.protocol, ShadowStdioProtocolV1::V1);
        assert!(matches!(
            parsed.authority,
            ShadowCheckpointAuthorityV1::OriginalPromoted2Generation384Store { .. }
        ));

        let parsed = parse_args_v1(vec![
            "--xmage-cp7-teacher-jsonl".into(),
            "teacher.jsonl".into(),
            "--generation".into(),
            "256".into(),
            "--original-store-root".into(),
            "store".into(),
        ])
        .unwrap();
        assert_eq!(parsed.teacher_jsonl, Some(PathBuf::from("teacher.jsonl")));
        assert_eq!(parsed.outcome_jsonl, None);
        assert!(matches!(
            parsed.authority,
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

        let parsed = parse_args_v1(vec![
            "--population-store-root".into(),
            "population-store".into(),
            "--generation".into(),
            "1024".into(),
        ])
        .unwrap();
        assert_eq!(parsed.teacher_jsonl, None);
        assert_eq!(parsed.outcome_jsonl, None);
        assert!(matches!(
            parsed.authority,
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

        let parsed = parse_args_v1(vec![
            "--cp7-behavior-clone-root".into(),
            "cp7-derivative".into(),
        ])
        .unwrap();
        assert_eq!(parsed.teacher_jsonl, None);
        assert_eq!(parsed.outcome_jsonl, None);
        assert!(matches!(
            parsed.authority,
            ShadowCheckpointAuthorityV1::Cp7BehaviorCloneDerivative { .. }
        ));
        assert!(parse_args_v1(vec![
            "--cp7-behavior-clone-root".into(),
            "cp7-derivative".into(),
            "--generation".into(),
            "1".into(),
        ])
        .is_err());

        let parsed = parse_args_v1(vec![
            "--xmage-cp7-outcome-root".into(),
            "outcome-derivative".into(),
        ])
        .unwrap();
        assert_eq!(parsed.teacher_jsonl, None);
        assert_eq!(parsed.outcome_jsonl, None);
        assert!(matches!(
            parsed.authority,
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

    #[test]
    fn stability_halves_flag_is_explicit_and_defaults_to_on_v1() {
        // Default: absent means ON, the configuration every existing
        // record was produced under.
        let parsed = parse_args_v1(model_guided_search_argv_v1("t512")).unwrap();
        assert!(parsed.model_guided_search.unwrap().stability_halves_enabled);

        for (value, expected) in [("on", true), ("off", false)] {
            let mut argv = model_guided_search_argv_v1("t512");
            argv.push("--model-guided-search-stability-halves".into());
            argv.push(value.into());
            let parsed = parse_args_v1(argv).unwrap();
            assert_eq!(
                parsed.model_guided_search.unwrap().stability_halves_enabled,
                expected
            );
        }

        // Only the two spellings, and only in lower case: a tri-state
        // pre-registered switch must never be set by a typo.
        for bad in ["ON", "true", "1", "yes", "", "disabled"] {
            let mut argv = model_guided_search_argv_v1("t512");
            argv.push("--model-guided-search-stability-halves".into());
            argv.push(bad.into());
            assert!(parse_args_v1(argv).is_err(), "{bad:?} must be rejected");
        }

        // Repeating it is rejected, like every other flag here.
        let mut repeated = model_guided_search_argv_v1("t512");
        for value in ["on", "off"] {
            repeated.push("--model-guided-search-stability-halves".into());
            repeated.push(value.into());
        }
        assert!(parse_args_v1(repeated).is_err());

        // It is a MODIFIER, not a selector: on its own it cannot turn the
        // wrapper on, and it cannot be silently ignored either.
        assert!(parse_args_v1(vec![
            "--original-store-root".into(),
            "store".into(),
            "--model-guided-search-stability-halves".into(),
            "off".into(),
        ])
        .is_err());
    }

    fn model_guided_search_argv_v1(tier: &str) -> Vec<OsString> {
        vec![
            "--population-store-root".into(),
            "population-store".into(),
            "--generation".into(),
            "1024".into(),
            "--model-guided-search-tier".into(),
            tier.into(),
            "--model-guided-search-seed-block".into(),
            "0".into(),
            "--model-guided-search-diagnostics-dir".into(),
            "diag".into(),
        ]
    }

    #[test]
    fn model_guided_search_flags_parse_every_tier_and_are_order_independent_v1() {
        for (text, expected) in [
            ("t512", KernelNativeSearchTierV1::T512),
            ("t2048", KernelNativeSearchTierV1::T2048),
            ("t8192", KernelNativeSearchTierV1::T8192),
            ("t32768", KernelNativeSearchTierV1::T32768),
        ] {
            let parsed = parse_args_v1(model_guided_search_argv_v1(text)).unwrap();
            let search = parsed.model_guided_search.expect("search args present");
            assert_eq!(search.tier, expected);
            assert_eq!(search.seed_block_id, 0);
            assert_eq!(search.diagnostics_directory, PathBuf::from("diag"));
            assert_eq!(parsed.teacher_jsonl, None);
            assert_eq!(parsed.outcome_jsonl, None);
        }

        // Order independence: the search flags first, the authority last.
        let mut reordered = model_guided_search_argv_v1("t2048");
        reordered.rotate_left(4);
        let parsed = parse_args_v1(reordered).unwrap();
        assert_eq!(
            parsed.model_guided_search.unwrap().tier,
            KernelNativeSearchTierV1::T2048
        );
    }

    #[test]
    fn model_guided_search_flags_are_strict_and_all_or_nothing_v1() {
        // An unknown or differently-spelled tier is a usage error, never a
        // neighbouring tier.
        for bad_tier in ["512", "T512", "t512 ", "t1024", ""] {
            assert!(
                parse_args_v1(model_guided_search_argv_v1(bad_tier)).is_err(),
                "tier {bad_tier:?} must be rejected"
            );
        }

        // Every proper subset of the three flags is rejected.
        let full = model_guided_search_argv_v1("t512");
        for drop_pair in [4usize, 6, 8] {
            let mut partial = full.clone();
            partial.drain(drop_pair..drop_pair + 2);
            assert!(
                parse_args_v1(partial).is_err(),
                "dropping the flag pair at {drop_pair} must be a usage error"
            );
        }

        // A repeated search flag is rejected, like every other flag here.
        let mut repeated = full.clone();
        repeated.push("--model-guided-search-seed-block".into());
        repeated.push("1".into());
        assert!(parse_args_v1(repeated).is_err());

        // A non-numeric seed block is rejected rather than defaulted.
        let mut bad_block = full.clone();
        bad_block[7] = "first".into();
        assert!(parse_args_v1(bad_block).is_err());

        // The wrapper is mutually exclusive with both trajectory exports.
        for export_flag in ["--xmage-cp7-teacher-jsonl", "--xmage-cp7-outcome-jsonl"] {
            let mut with_export = full.clone();
            with_export.push(export_flag.into());
            with_export.push("export.jsonl".into());
            assert!(
                parse_args_v1(with_export).is_err(),
                "{export_flag} must not combine with the search wrapper"
            );
        }

        // No environment variable can turn the wrapper on: parsing reads
        // only argv, and with no search flag the result carries none.
        let parsed = parse_args_v1(vec!["--original-store-root".into(), "store".into()]).unwrap();
        assert_eq!(parsed.model_guided_search, None);
    }
}
