//! Test-time-search wrapper, stage S1: the per-tier feasibility replay
//! (`LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md` Section 5, S1).
//!
//! Reconstructs every decision of a frozen corpus through the kernel, runs
//! the production `ModelGuidedSearch` selector over it at one ladder tier
//! with the stability halves off, and publishes an immutable, hash-chained
//! canonical JSON report carrying p50/p99/max of both timings, decisions
//! per second, and the tier's feasibility verdict.
//!
//! EXIT CODES. `0` the tier is FEASIBLE, `4` the tier is INFEASIBLE (the
//! report is published either way, and the verdict line is printed either
//! way), `1` a real failure with no report, `2` a usage error. An
//! INFEASIBLE tier is therefore impossible to mistake for a pass and
//! equally impossible to mistake for a crash.
//!
//! Every input is an explicit flag; no environment variable is read.

use mtg_kernel::kernel_native_search_opponent_v1::KernelNativeSearchTierV1;
use mtg_kernel::native_checkpoint_shadow_stdio_v1::ShadowCheckpointAuthorityV1;
use mtg_kernel::native_tts_s1_replay_v1::{
    parse_tts_s1_tier_v1, publish_tts_s1_replay_report_v1, run_tts_s1_replay_v1,
    TtsS1ReplayConfigV1, TtsS1TierVerdictV1,
};
use std::ffi::OsString;
use std::path::PathBuf;

fn usage_v1() -> ! {
    eprintln!(
        "usage: tts_s1_replay_v1 (--original-store-root PATH [--generation N] | --population-store-root PATH --generation N | --portable-derivative-root PATH) --corpus PATH --tier (t512|t2048|t8192|t32768) --seed-block N --diagnostics-dir PATH --output PATH [--limit-decisions N]"
    );
    std::process::exit(2);
}

enum AuthorityRootV1 {
    Original(PathBuf),
    Population(PathBuf),
    Portable(PathBuf),
}

#[derive(Debug, Eq, PartialEq)]
struct ParsedArgsV1 {
    authority: ShadowCheckpointAuthorityV1,
    corpus: PathBuf,
    tier: KernelNativeSearchTierV1,
    seed_block_id: usize,
    diagnostics_dir: PathBuf,
    output: PathBuf,
    limit_decisions: Option<u64>,
}

fn parse_args_v1(raw: Vec<OsString>) -> Result<ParsedArgsV1, ()> {
    if raw.len() < 12 || raw.len() > 16 || !raw.len().is_multiple_of(2) {
        return Err(());
    }
    let mut authority_root = None;
    let mut generation = None;
    let mut corpus = None;
    let mut tier = None;
    let mut seed_block_id = None;
    let mut diagnostics_dir = None;
    let mut output = None;
    let mut limit_decisions = None;
    for pair in raw.chunks_exact(2) {
        let flag = &pair[0];
        if flag == "--original-store-root" && authority_root.is_none() {
            authority_root = Some(AuthorityRootV1::Original(PathBuf::from(&pair[1])));
        } else if flag == "--population-store-root" && authority_root.is_none() {
            authority_root = Some(AuthorityRootV1::Population(PathBuf::from(&pair[1])));
        } else if flag == "--portable-derivative-root" && authority_root.is_none() {
            authority_root = Some(AuthorityRootV1::Portable(PathBuf::from(&pair[1])));
        } else if flag == "--generation" && generation.is_none() {
            generation = Some(pair[1].to_str().ok_or(())?.parse::<u64>().map_err(|_| ())?);
        } else if flag == "--corpus" && corpus.is_none() {
            corpus = Some(PathBuf::from(&pair[1]));
        } else if flag == "--tier" && tier.is_none() {
            tier = Some(parse_tts_s1_tier_v1(pair[1].to_str().ok_or(())?).ok_or(())?);
        } else if flag == "--seed-block" && seed_block_id.is_none() {
            seed_block_id = Some(
                pair[1]
                    .to_str()
                    .ok_or(())?
                    .parse::<usize>()
                    .map_err(|_| ())?,
            );
        } else if flag == "--diagnostics-dir" && diagnostics_dir.is_none() {
            diagnostics_dir = Some(PathBuf::from(&pair[1]));
        } else if flag == "--output" && output.is_none() {
            output = Some(PathBuf::from(&pair[1]));
        } else if flag == "--limit-decisions" && limit_decisions.is_none() {
            limit_decisions = Some(pair[1].to_str().ok_or(())?.parse::<u64>().map_err(|_| ())?);
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
        (Some(AuthorityRootV1::Portable(_)), Some(_))
        | (Some(AuthorityRootV1::Population(_)), None)
        | (None, _) => return Err(()),
    };
    Ok(ParsedArgsV1 {
        authority,
        corpus: corpus.ok_or(())?,
        tier: tier.ok_or(())?,
        seed_block_id: seed_block_id.ok_or(())?,
        diagnostics_dir: diagnostics_dir.ok_or(())?,
        output: output.ok_or(())?,
        limit_decisions,
    })
}

fn main() {
    let raw = std::env::args_os().skip(1).collect();
    let parsed = parse_args_v1(raw).unwrap_or_else(|()| usage_v1());
    let config = TtsS1ReplayConfigV1 {
        authority: parsed.authority,
        corpus_path: parsed.corpus,
        tier: parsed.tier,
        seed_block_id: parsed.seed_block_id,
        limit_decisions: parsed.limit_decisions,
        diagnostics_directory: parsed.diagnostics_dir,
    };
    let report = match run_tts_s1_replay_v1(&config) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("TTS_S1_REPLAY_FAILED {error}");
            std::process::exit(1);
        }
    };
    let bytes = match publish_tts_s1_replay_report_v1(&report, &parsed.output) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("TTS_S1_REPLAY_FAILED {error}");
            std::process::exit(1);
        }
    };
    let body = &report.body;
    println!(
        "TTS_S1_REPLAY_PUBLISHED path={} bytes={} report_sha256={} tier={} verdict={} decisions={} whole_corpus={} search_p50_micros={} search_p99_micros={} search_max_micros={} decision_p50_micros={} decision_p99_micros={} decision_max_micros={} protocol_p50_micros={} protocol_p99_micros={} protocol_max_micros={} decisions_per_second_milli={} projected_s2_worker_hours_milli={} compute_cap_worker_hours_milli={} within_compute_cap={} corpus_sha256={} authority_digest={}",
        parsed.output.display(),
        bytes.len(),
        report.report_sha256,
        body.tier,
        body.verdict.tag_v1(),
        body.decisions_replayed,
        body.replayed_whole_corpus,
        body.search_wall_time.p50_micros,
        body.search_wall_time.p99_micros,
        body.search_wall_time.max_micros,
        body.decision_wall_time.p50_micros,
        body.decision_wall_time.p99_micros,
        body.decision_wall_time.max_micros,
        body.protocol_wall_time.p50_micros,
        body.protocol_wall_time.p99_micros,
        body.protocol_wall_time.max_micros,
        body.decisions_per_second_milli,
        body.compute_cap.projected_worker_hours_milli,
        body.compute_cap.cap_worker_hours_milli,
        body.compute_cap.within_cap,
        body.corpus_sha256,
        body.search_authority_digest_sha256,
    );
    // Never silent: the verdict goes to stderr too, so a launcher that
    // only captures one stream still sees it.
    eprintln!(
        "TTS_S1_TIER_VERDICT tier={} verdict={} reason={}",
        body.tier,
        body.verdict.tag_v1(),
        body.verdict_reason,
    );
    if body.verdict == TtsS1TierVerdictV1::Infeasible {
        std::process::exit(4);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv_v1() -> Vec<OsString> {
        vec![
            "--population-store-root".into(),
            "store".into(),
            "--generation".into(),
            "1024".into(),
            "--corpus".into(),
            "corpus.json".into(),
            "--tier".into(),
            "t512".into(),
            "--seed-block".into(),
            "1".into(),
            "--diagnostics-dir".into(),
            "diagnostics".into(),
            "--output".into(),
            "report.json".into(),
        ]
    }

    #[test]
    fn every_tier_parses_and_the_flags_are_order_independent_v1() {
        for (text, expected) in [
            ("t512", KernelNativeSearchTierV1::T512),
            ("t2048", KernelNativeSearchTierV1::T2048),
            ("t8192", KernelNativeSearchTierV1::T8192),
            ("t32768", KernelNativeSearchTierV1::T32768),
        ] {
            let mut argv = argv_v1();
            argv[7] = text.into();
            let parsed = parse_args_v1(argv).unwrap();
            assert_eq!(parsed.tier, expected);
            assert_eq!(parsed.seed_block_id, 1);
            assert_eq!(parsed.limit_decisions, None);
            assert_eq!(parsed.diagnostics_dir, PathBuf::from("diagnostics"));
            assert_eq!(parsed.output, PathBuf::from("report.json"));
        }
        let mut reordered = argv_v1();
        reordered.rotate_left(6);
        assert_eq!(
            parse_args_v1(reordered).unwrap(),
            parse_args_v1(argv_v1()).unwrap()
        );
    }

    #[test]
    fn the_smoke_bound_is_optional_and_numeric_v1() {
        let mut argv = argv_v1();
        argv.push("--limit-decisions".into());
        argv.push("8".into());
        assert_eq!(parse_args_v1(argv).unwrap().limit_decisions, Some(8));

        let mut argv = argv_v1();
        argv.push("--limit-decisions".into());
        argv.push("all".into());
        assert!(parse_args_v1(argv).is_err());
    }

    #[test]
    fn the_flag_surface_is_strict_v1() {
        let full = argv_v1();
        for drop_pair in [0usize, 4, 6, 8, 10, 12] {
            let mut partial = full.clone();
            partial.drain(drop_pair..drop_pair + 2);
            assert!(
                parse_args_v1(partial).is_err(),
                "dropping the pair at {drop_pair} must be a usage error"
            );
        }
        for bad_tier in ["512", "T512", "t1024", "t512 ", ""] {
            let mut argv = full.clone();
            argv[7] = bad_tier.into();
            assert!(
                parse_args_v1(argv).is_err(),
                "tier {bad_tier:?} must be rejected"
            );
        }
        let mut repeated = full.clone();
        repeated.push("--tier".into());
        repeated.push("t2048".into());
        assert!(parse_args_v1(repeated).is_err());

        let mut unknown = full.clone();
        unknown.push("--episodes".into());
        unknown.push("64".into());
        assert!(parse_args_v1(unknown).is_err());

        // The diagnostics directory is REQUIRED, not optional: the
        // protocol latency the SLO is classified on is measured by the
        // production writer that publishes into it.
        let mut repeated_dir = full.clone();
        repeated_dir.push("--diagnostics-dir".into());
        repeated_dir.push("other".into());
        assert!(parse_args_v1(repeated_dir).is_err());
    }
}
