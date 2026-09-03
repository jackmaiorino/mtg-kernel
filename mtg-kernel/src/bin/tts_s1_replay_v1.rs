//! Test-time-search wrapper, stage S1: the per-tier feasibility replay
//! (`LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md` Section 5, S1).
//!
//! Reconstructs every decision of a frozen corpus through the kernel, runs
//! the production `ModelGuidedSearch` selector over it at one ladder tier
//! with the stability halves off, and publishes an immutable, hash-chained
//! canonical JSON report carrying p50/p99/max of both timings, decisions
//! per second, and the tier's feasibility verdict.
//!
//! THREE MODES, one bin, and each publishes a different document:
//!
//! 1. UNSHARDED (no `--shard-*`): the whole corpus in one process,
//!    publishing the tier report. Unchanged, byte for byte, from before
//!    sharding existed.
//! 2. SHARD (`--shard-index N --shard-count K`, both or neither): the
//!    contributing episodes whose position in the corpus's episode order
//!    is `N` modulo `K`, publishing a SHARD report with a schema of its
//!    own and no verdict. K processes at one tier do the work of one.
//! 3. MERGE (`--merge-shards DIR --shard-count K --output PATH`): reads
//!    exactly those K shard reports and publishes the tier report the
//!    unsharded run would have published, recomputing every statistic over
//!    the union. It loads no checkpoint and searches nothing.
//! 4. PUBLISH BARRIER (`--publish-start-barrier PATH --barrier-dir DIR
//!    --shard-count K --readiness-timeout-seconds N`): waits for all K
//!    shards to announce themselves ready and then writes the start token.
//!    It exists because the token's instant is compared, EXACTLY and with
//!    no tolerance, against instants the shards recorded through this
//!    crate's own clock: a launcher that stamped the token from its own
//!    runtime would be comparing two clocks, and two clocks in two runtimes
//!    do not have to disagree by much to invert a comparison between
//!    instants a few microseconds apart. It loads no checkpoint and
//!    searches nothing, and exits 0 or 1.
//!
//! EXIT CODES. `0` the tier is FEASIBLE (or a shard finished), `4` the tier
//! is INFEASIBLE (the report is published either way, and the verdict line
//! is printed either way), `1` a real failure with no report, `2` a usage
//! error. An INFEASIBLE tier is therefore impossible to mistake for a pass
//! and equally impossible to mistake for a crash. A SHARD never exits 4,
//! because a fraction of the episodes carries no verdict to exit on.
//!
//! Every input is an explicit flag; no environment variable is read.

use mtg_kernel::kernel_native_search_opponent_v1::KernelNativeSearchTierV1;
use mtg_kernel::native_checkpoint_shadow_stdio_v1::ShadowCheckpointAuthorityV1;
use mtg_kernel::native_tts_s1_replay_v1::{
    merge_tts_s1_replay_shards_v1, parse_tts_s1_tier_v1, publish_start_barrier_v1,
    publish_tts_s1_replay_report_v1, publish_tts_s1_replay_shard_report_v1,
    run_tts_s1_replay_shard_v1, run_tts_s1_replay_v1, TtsS1ReplayConfigV1, TtsS1ShardSelectorV1,
    TtsS1StartBarrierConfigV1, TtsS1TierVerdictV1, TtsS1WallClockBaseV1, TTS_S1_MAX_SHARD_COUNT_V1,
};
use std::ffi::OsString;
use std::path::PathBuf;

fn usage_v1() -> ! {
    eprintln!(
        "usage: tts_s1_replay_v1 (--original-store-root PATH [--generation N] | --population-store-root PATH --generation N | --portable-derivative-root PATH) --corpus PATH --tier (t512|t2048|t8192|t32768) --seed-block N --diagnostics-dir PATH --max-episodes N --output PATH [--limit-episodes N] [--shard-index N --shard-count K [--start-barrier PATH --start-barrier-timeout-seconds N]]"
    );
    eprintln!("   or: tts_s1_replay_v1 --merge-shards DIR --shard-count K --output PATH");
    eprintln!(
        "   or: tts_s1_replay_v1 --publish-start-barrier PATH --barrier-dir DIR --shard-count K --readiness-timeout-seconds N"
    );
    std::process::exit(2);
}

enum AuthorityRootV1 {
    Original(PathBuf),
    Population(PathBuf),
    Portable(PathBuf),
}

#[derive(Debug, Eq, PartialEq)]
struct ReplayArgsV1 {
    authority: ShadowCheckpointAuthorityV1,
    corpus: PathBuf,
    tier: KernelNativeSearchTierV1,
    seed_block_id: usize,
    diagnostics_dir: PathBuf,
    max_episodes: u64,
    output: PathBuf,
    limit_episodes: Option<u64>,
    /// `None` is the unsharded run.
    shard: Option<TtsS1ShardSelectorV1>,
    /// Where this shard waits for the rest of the fan-out. Shard-only.
    start_barrier: Option<TtsS1StartBarrierConfigV1>,
}

#[derive(Debug, Eq, PartialEq)]
struct MergeArgsV1 {
    shard_directory: PathBuf,
    shard_count: u64,
    output: PathBuf,
}

#[derive(Debug, Eq, PartialEq)]
struct PublishBarrierArgsV1 {
    token_path: PathBuf,
    barrier_directory: PathBuf,
    shard_count: u64,
    readiness_timeout_seconds: u64,
}

#[derive(Debug, Eq, PartialEq)]
enum ParsedArgsV1 {
    Replay(ReplayArgsV1),
    Merge(MergeArgsV1),
    PublishBarrier(PublishBarrierArgsV1),
}

fn parse_u64_v1(value: &OsString) -> Result<u64, ()> {
    value.to_str().ok_or(())?.parse::<u64>().map_err(|_| ())
}

fn parse_args_v1(raw: Vec<OsString>) -> Result<ParsedArgsV1, ()> {
    // Length is bounded on both sides and always even, so a dangling flag
    // or a value with no flag is a usage error rather than a silent
    // mis-pairing. The upper bound is the whole replay surface: the
    // authority pair, the generation, the corpus, the tier, the seed
    // block, the diagnostics directory, the guard, the output, the smoke
    // bound, and the two shard flags.
    if raw.len() < 6 || raw.len() > 26 || !raw.len().is_multiple_of(2) {
        return Err(());
    }
    let mut authority_root = None;
    let mut generation = None;
    let mut corpus = None;
    let mut tier = None;
    let mut seed_block_id = None;
    let mut diagnostics_dir = None;
    let mut max_episodes = None;
    let mut output = None;
    let mut limit_episodes = None;
    let mut shard_index = None;
    let mut shard_count = None;
    let mut merge_shards = None;
    let mut start_barrier_path = None;
    let mut start_barrier_timeout = None;
    let mut publish_start_barrier = None;
    let mut barrier_directory = None;
    let mut readiness_timeout = None;
    for pair in raw.chunks_exact(2) {
        let flag = &pair[0];
        if flag == "--original-store-root" && authority_root.is_none() {
            authority_root = Some(AuthorityRootV1::Original(PathBuf::from(&pair[1])));
        } else if flag == "--population-store-root" && authority_root.is_none() {
            authority_root = Some(AuthorityRootV1::Population(PathBuf::from(&pair[1])));
        } else if flag == "--portable-derivative-root" && authority_root.is_none() {
            authority_root = Some(AuthorityRootV1::Portable(PathBuf::from(&pair[1])));
        } else if flag == "--generation" && generation.is_none() {
            generation = Some(parse_u64_v1(&pair[1])?);
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
        } else if flag == "--max-episodes" && max_episodes.is_none() {
            max_episodes = Some(parse_u64_v1(&pair[1])?);
        } else if flag == "--limit-episodes" && limit_episodes.is_none() {
            limit_episodes = Some(parse_u64_v1(&pair[1])?);
        } else if flag == "--shard-index" && shard_index.is_none() {
            shard_index = Some(parse_u64_v1(&pair[1])?);
        } else if flag == "--shard-count" && shard_count.is_none() {
            shard_count = Some(parse_u64_v1(&pair[1])?);
        } else if flag == "--start-barrier" && start_barrier_path.is_none() {
            start_barrier_path = Some(PathBuf::from(&pair[1]));
        } else if flag == "--start-barrier-timeout-seconds" && start_barrier_timeout.is_none() {
            start_barrier_timeout = Some(parse_u64_v1(&pair[1])?);
        } else if flag == "--publish-start-barrier" && publish_start_barrier.is_none() {
            publish_start_barrier = Some(PathBuf::from(&pair[1]));
        } else if flag == "--barrier-dir" && barrier_directory.is_none() {
            barrier_directory = Some(PathBuf::from(&pair[1]));
        } else if flag == "--readiness-timeout-seconds" && readiness_timeout.is_none() {
            readiness_timeout = Some(parse_u64_v1(&pair[1])?);
        } else if flag == "--merge-shards" && merge_shards.is_none() {
            merge_shards = Some(PathBuf::from(&pair[1]));
        } else {
            return Err(());
        }
    }

    // PUBLISH-BARRIER MODE. It loads no checkpoint, reads no corpus and
    // publishes no report, so every other flag is rejected rather than
    // ignored.
    if let Some(token_path) = publish_start_barrier {
        if authority_root.is_some()
            || generation.is_some()
            || corpus.is_some()
            || tier.is_some()
            || seed_block_id.is_some()
            || diagnostics_dir.is_some()
            || max_episodes.is_some()
            || limit_episodes.is_some()
            || shard_index.is_some()
            || start_barrier_path.is_some()
            || start_barrier_timeout.is_some()
            || merge_shards.is_some()
            || output.is_some()
        {
            return Err(());
        }
        let readiness_timeout_seconds = readiness_timeout.ok_or(())?;
        if readiness_timeout_seconds == 0 {
            return Err(());
        }
        // The fan-out size is range-checked HERE, at the flag surface, the
        // same way every other mode checks it: a publisher told to wait for
        // no shards, or for more than the ladder allows, is a usage error
        // and not something to discover once it is already running.
        let shard_count = shard_count.ok_or(())?;
        if shard_count == 0 || shard_count > TTS_S1_MAX_SHARD_COUNT_V1 {
            return Err(());
        }
        return Ok(ParsedArgsV1::PublishBarrier(PublishBarrierArgsV1 {
            token_path,
            barrier_directory: barrier_directory.ok_or(())?,
            shard_count,
            readiness_timeout_seconds,
        }));
    }
    if barrier_directory.is_some() || readiness_timeout.is_some() {
        // Only the publish mode has anything to do with either.
        return Err(());
    }

    // MERGE MODE. It loads no checkpoint and reads no corpus, so every
    // replay-only flag is rejected rather than ignored: a merge invoked
    // with a tier would look like it had measured one.
    if let Some(shard_directory) = merge_shards {
        if authority_root.is_some()
            || generation.is_some()
            || corpus.is_some()
            || tier.is_some()
            || seed_block_id.is_some()
            || diagnostics_dir.is_some()
            || max_episodes.is_some()
            || limit_episodes.is_some()
            || shard_index.is_some()
            || start_barrier_path.is_some()
            || start_barrier_timeout.is_some()
        {
            return Err(());
        }
        return Ok(ParsedArgsV1::Merge(MergeArgsV1 {
            shard_directory,
            shard_count: shard_count.ok_or(())?,
            output: output.ok_or(())?,
        }));
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
    // BOTH SHARD FLAGS OR NEITHER. One alone is a usage error and never a
    // defaulted shard: `--shard-count 8` with no index would otherwise run
    // shard 0 and look like a whole replay, and `--shard-index 3` with no
    // count has no meaning at all.
    let shard = match (shard_index, shard_count) {
        (None, None) => None,
        (Some(index), Some(count)) => Some(TtsS1ShardSelectorV1::new_v1(index, count).ok_or(())?),
        _ => return Err(()),
    };
    // BOTH BARRIER FLAGS OR NEITHER, and only for a shard. A path with no
    // deadline would be an unbounded wait, a deadline with no path would
    // wait for nothing, and an unsharded run has nobody to wait for; each
    // is a usage error rather than a silently ignored flag.
    let start_barrier = match (start_barrier_path, start_barrier_timeout) {
        (None, None) => None,
        (Some(path), Some(timeout_seconds)) => {
            if shard.is_none() || timeout_seconds == 0 {
                return Err(());
            }
            Some(TtsS1StartBarrierConfigV1 {
                path,
                timeout_seconds,
            })
        }
        _ => return Err(()),
    };
    Ok(ParsedArgsV1::Replay(ReplayArgsV1 {
        authority,
        corpus: corpus.ok_or(())?,
        tier: tier.ok_or(())?,
        seed_block_id: seed_block_id.ok_or(())?,
        diagnostics_dir: diagnostics_dir.ok_or(())?,
        max_episodes: max_episodes.ok_or(())?,
        output: output.ok_or(())?,
        limit_episodes,
        shard,
        start_barrier,
    }))
}

fn fail_v1(error: impl std::fmt::Display) -> ! {
    eprintln!("TTS_S1_REPLAY_FAILED {error}");
    std::process::exit(1);
}

fn run_publish_barrier_v1(parsed: PublishBarrierArgsV1) -> ! {
    // The base supplies diagnostic instants only. Formal ordering comes
    // from the ready-file digests committed by the canonical token.
    let clock = TtsS1WallClockBaseV1::now_v1();
    let published = match publish_start_barrier_v1(
        &parsed.token_path,
        &parsed.barrier_directory,
        parsed.shard_count,
        parsed.readiness_timeout_seconds,
        &clock,
    ) {
        Ok(published) => published,
        Err(error) => fail_v1(error),
    };
    println!(
        "TTS_S1_BARRIER_PUBLISHED path={} shard_count={} token_sha256={} observed_ready_count={} released_unix_micros={} latest_ready_unix_micros={} waited_micros={}",
        parsed.token_path.display(),
        parsed.shard_count,
        published.token_sha256,
        published.token.observed_shard_readiness.len(),
        published.token.released_unix_micros,
        published.latest_ready_unix_micros,
        published.waited_micros,
    );
    for announcement in &published.ready {
        println!(
            "TTS_S1_SHARD_READY shard_index={} process_id={} ready_unix_micros={}",
            announcement.shard_index, announcement.process_id, announcement.ready_unix_micros,
        );
    }
    std::process::exit(0);
}

fn run_merge_v1(parsed: MergeArgsV1) -> ! {
    let report = match merge_tts_s1_replay_shards_v1(&parsed.shard_directory, parsed.shard_count) {
        Ok(report) => report,
        Err(error) => fail_v1(error),
    };
    let bytes = match publish_tts_s1_replay_report_v1(&report, &parsed.output) {
        Ok(bytes) => bytes,
        Err(error) => fail_v1(error),
    };
    print_replay_report_v1(&report, &parsed.output, bytes.len());
    println!(
        "TTS_S1_MERGE_PUBLISHED path={} shard_dir={} shard_count={} tier={} episodes={} searched_decisions={}",
        parsed.output.display(),
        parsed.shard_directory.display(),
        parsed.shard_count,
        report.body.tier,
        report.body.episodes_replayed,
        report.body.searched_decisions,
    );
    exit_on_verdict_v1(&report);
}

fn print_replay_report_v1(
    report: &mtg_kernel::native_tts_s1_replay_v1::TtsS1ReplayReportV1,
    output: &std::path::Path,
    bytes: usize,
) {
    let body = &report.body;
    println!(
        "TTS_S1_REPLAY_PUBLISHED path={} bytes={} report_sha256={} tier={} verdict={} episodes={} searched_decisions={} corpus_targets={} whole_corpus={} protocol_p50_micros={} protocol_p99_micros={} protocol_max_micros={} mean_protocol_micros={} decisions_per_second_milli={} target_protocol_p99_micros={} projected_s2_worker_hours_milli={} projected_elapsed_hours_at_16_workers_milli={} compute_cap_worker_hours_milli={} within_compute_cap={} corpus_sha256={} authority_digest={}",
        output.display(),
        bytes,
        report.report_sha256,
        body.tier,
        body.verdict.tag_v1(),
        body.episodes_replayed,
        body.searched_decisions,
        body.corpus_targets_replayed,
        body.replayed_whole_corpus,
        body.whole_episode_view.protocol_wall_time.p50_micros,
        body.whole_episode_view.protocol_wall_time.p99_micros,
        body.whole_episode_view.protocol_wall_time.max_micros,
        body.whole_episode_view.mean_protocol_micros,
        body.whole_episode_view.decisions_per_second_milli,
        body.corpus_target_view.protocol_wall_time.p99_micros,
        body.compute_cap.projected_worker_hours_milli,
        body.compute_cap.projected_elapsed_hours_at_workers_milli,
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
}

fn exit_on_verdict_v1(report: &mtg_kernel::native_tts_s1_replay_v1::TtsS1ReplayReportV1) -> ! {
    if report.body.verdict == TtsS1TierVerdictV1::Infeasible {
        std::process::exit(4);
    }
    std::process::exit(0);
}

fn main() {
    let raw = std::env::args_os().skip(1).collect();
    let parsed = parse_args_v1(raw).unwrap_or_else(|()| usage_v1());
    let parsed = match parsed {
        ParsedArgsV1::Merge(merge) => run_merge_v1(merge),
        ParsedArgsV1::PublishBarrier(publish) => run_publish_barrier_v1(publish),
        ParsedArgsV1::Replay(replay) => replay,
    };
    let output = parsed.output.clone();
    let shard = parsed.shard;
    let config = TtsS1ReplayConfigV1 {
        authority: parsed.authority,
        corpus_path: parsed.corpus,
        tier: parsed.tier,
        seed_block_id: parsed.seed_block_id,
        max_episodes: parsed.max_episodes,
        limit_episodes: parsed.limit_episodes,
        shard,
        start_barrier: parsed.start_barrier,
        diagnostics_directory: parsed.diagnostics_dir,
    };

    // A SHARD publishes a shard report and stops. It has no verdict to
    // print and none to exit on; the merge is where a tier gets either.
    if shard.is_some() {
        let report = match run_tts_s1_replay_shard_v1(&config) {
            Ok(report) => report,
            Err(error) => fail_v1(error),
        };
        let bytes = match publish_tts_s1_replay_shard_report_v1(&report, &output) {
            Ok(bytes) => bytes,
            Err(error) => fail_v1(error),
        };
        let body = &report.body;
        println!(
            "TTS_S1_SHARD_PUBLISHED path={} bytes={} shard_report_sha256={} tier={} shard_index={} shard_count={} shard_episodes={} planned_episodes={} searched_decisions={} corpus_targets={} corpus_sha256={} authority_digest={}",
            output.display(),
            bytes.len(),
            report.shard_report_sha256,
            body.identity.tier,
            body.shard_index,
            body.shard_count,
            body.shard_episodes_replayed,
            body.identity.episodes_replayed,
            body.searched_decisions,
            body.corpus_targets_replayed,
            body.identity.corpus_sha256,
            body.identity.search_authority_digest_sha256,
        );
        eprintln!(
            "TTS_S1_SHARD_BARRIER used={} token_sha256={} observed_token_before_first_decision={} released_unix_micros={} wait_micros={} first_decision_unix_micros={}",
            body.start_barrier.used,
            body.start_barrier.token_sha256,
            body.start_barrier.observed_token_before_first_decision,
            body.start_barrier.token.released_unix_micros,
            body.start_barrier.wait_micros,
            body.first_work_started_unix_micros,
        );
        eprintln!(
            "TTS_S1_SHARD_DONE tier={} shard_index={} shard_count={} shard_episodes={}",
            body.identity.tier, body.shard_index, body.shard_count, body.shard_episodes_replayed,
        );
        return;
    }

    let report = match run_tts_s1_replay_v1(&config) {
        Ok(report) => report,
        Err(error) => fail_v1(error),
    };
    let bytes = match publish_tts_s1_replay_report_v1(&report, &output) {
        Ok(bytes) => bytes,
        Err(error) => fail_v1(error),
    };
    print_replay_report_v1(&report, &output, bytes.len());
    exit_on_verdict_v1(&report);
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
            "--max-episodes".into(),
            "64".into(),
            "--output".into(),
            "report.json".into(),
        ]
    }

    fn replay_v1(raw: Vec<OsString>) -> Result<ReplayArgsV1, ()> {
        match parse_args_v1(raw)? {
            ParsedArgsV1::Replay(replay) => Ok(replay),
            ParsedArgsV1::Merge(_) | ParsedArgsV1::PublishBarrier(_) => Err(()),
        }
    }

    fn publish_argv_v1() -> Vec<OsString> {
        vec![
            "--publish-start-barrier".into(),
            "shards/start-barrier.token".into(),
            "--barrier-dir".into(),
            "shards".into(),
            "--shard-count".into(),
            "8".into(),
            "--readiness-timeout-seconds".into(),
            "900".into(),
        ]
    }

    /// The publish mode is its own closed flag surface. It loads no
    /// checkpoint, reads no corpus and publishes no report, so a flag from
    /// any other mode is a usage error rather than something ignored.
    #[test]
    fn the_publish_barrier_mode_is_its_own_closed_flag_surface_v1() {
        assert_eq!(
            parse_args_v1(publish_argv_v1()).unwrap(),
            ParsedArgsV1::PublishBarrier(PublishBarrierArgsV1 {
                token_path: PathBuf::from("shards/start-barrier.token"),
                barrier_directory: PathBuf::from("shards"),
                shard_count: 8,
                readiness_timeout_seconds: 900,
            })
        );

        // Order independent, like every other invocation here.
        let mut reordered = publish_argv_v1();
        reordered.rotate_left(2);
        assert_eq!(
            parse_args_v1(reordered).unwrap(),
            parse_args_v1(publish_argv_v1()).unwrap()
        );

        // Every one of the four is required.
        for drop_pair in [0usize, 2, 4, 6] {
            let mut partial = publish_argv_v1();
            partial.drain(drop_pair..drop_pair + 2);
            assert!(
                parse_args_v1(partial).is_err(),
                "dropping the publish pair at {drop_pair} must be a usage error"
            );
        }

        // An unbounded wait is not a bounded one.
        let mut zero = publish_argv_v1();
        zero[7] = "0".into();
        assert!(parse_args_v1(zero).is_err());
        // And the shard count is range-checked like everywhere else.
        for bad in ["0", "65"] {
            let mut argv = publish_argv_v1();
            argv[5] = bad.into();
            assert!(
                parse_args_v1(argv).is_err(),
                "--shard-count {bad} must be rejected"
            );
        }

        for extra in [
            vec![OsString::from("--tier"), OsString::from("t512")],
            vec![OsString::from("--corpus"), OsString::from("corpus.json")],
            vec![OsString::from("--seed-block"), OsString::from("1")],
            vec![OsString::from("--max-episodes"), OsString::from("64")],
            vec![OsString::from("--output"), OsString::from("report.json")],
            vec![OsString::from("--shard-index"), OsString::from("0")],
            vec![OsString::from("--merge-shards"), OsString::from("shards")],
            vec![
                OsString::from("--start-barrier"),
                OsString::from("barrier.token"),
            ],
            vec![
                OsString::from("--diagnostics-dir"),
                OsString::from("diagnostics"),
            ],
            vec![
                OsString::from("--population-store-root"),
                OsString::from("store"),
            ],
        ] {
            let mut argv = publish_argv_v1();
            argv.extend(extra);
            assert!(
                parse_args_v1(argv).is_err(),
                "the publish mode must refuse a flag from another mode"
            );
        }

        // And the publish-only flags are refused everywhere else.
        for extra in [
            vec![OsString::from("--barrier-dir"), OsString::from("shards")],
            vec![
                OsString::from("--readiness-timeout-seconds"),
                OsString::from("900"),
            ],
        ] {
            let mut argv = argv_v1();
            argv.extend(extra.clone());
            assert!(replay_v1(argv).is_err(), "a replay refuses a publish flag");
            let mut argv = vec![
                OsString::from("--merge-shards"),
                OsString::from("shards"),
                OsString::from("--shard-count"),
                OsString::from("8"),
                OsString::from("--output"),
                OsString::from("report.json"),
            ];
            argv.extend(extra);
            assert!(
                parse_args_v1(argv).is_err(),
                "a merge refuses a publish flag"
            );
        }
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
            let parsed = replay_v1(argv).unwrap();
            assert_eq!(parsed.tier, expected);
            assert_eq!(parsed.seed_block_id, 1);
            assert_eq!(parsed.limit_episodes, None);
            assert_eq!(parsed.max_episodes, 64);
            assert_eq!(parsed.diagnostics_dir, PathBuf::from("diagnostics"));
            assert_eq!(parsed.output, PathBuf::from("report.json"));
            // The unsharded invocation is exactly what it was: no shard.
            assert_eq!(parsed.shard, None);
        }
        let mut reordered = argv_v1();
        reordered.rotate_left(6);
        assert_eq!(replay_v1(reordered).unwrap(), replay_v1(argv_v1()).unwrap());
    }

    #[test]
    fn the_smoke_bound_is_optional_and_numeric_v1() {
        let mut argv = argv_v1();
        argv.push("--limit-episodes".into());
        argv.push("8".into());
        assert_eq!(replay_v1(argv).unwrap().limit_episodes, Some(8));

        let mut argv = argv_v1();
        argv.push("--limit-episodes".into());
        argv.push("all".into());
        assert!(replay_v1(argv).is_err());

        // The GUARD, by contrast, is required: whole-episode replay is
        // orders of magnitude more work than the corpus size suggests, so
        // a run may not start without a stated bound.
        let mut argv = argv_v1();
        argv.push("--max-episodes".into());
        argv.push("2".into());
        assert!(replay_v1(argv).is_err(), "a repeated guard is rejected");
    }

    #[test]
    fn the_flag_surface_is_strict_v1() {
        let full = argv_v1();
        for drop_pair in [0usize, 4, 6, 8, 10, 12, 14] {
            let mut partial = full.clone();
            partial.drain(drop_pair..drop_pair + 2);
            assert!(
                replay_v1(partial).is_err(),
                "dropping the pair at {drop_pair} must be a usage error"
            );
        }
        for bad_tier in ["512", "T512", "t1024", "t512 ", ""] {
            let mut argv = full.clone();
            argv[7] = bad_tier.into();
            assert!(
                replay_v1(argv).is_err(),
                "tier {bad_tier:?} must be rejected"
            );
        }
        let mut repeated = full.clone();
        repeated.push("--tier".into());
        repeated.push("t2048".into());
        assert!(replay_v1(repeated).is_err());

        let mut unknown = full.clone();
        unknown.push("--episodes".into());
        unknown.push("64".into());
        assert!(replay_v1(unknown).is_err());

        // The diagnostics directory is REQUIRED, not optional: the
        // protocol latency the SLO is classified on is measured by the
        // production writer that publishes into it.
        let mut repeated_dir = full.clone();
        repeated_dir.push("--diagnostics-dir".into());
        repeated_dir.push("other".into());
        assert!(replay_v1(repeated_dir).is_err());
    }

    /// BOTH shard flags or neither, and the pair must name a real shard.
    #[test]
    fn the_shard_flags_come_in_a_pair_and_are_range_checked_v1() {
        let mut argv = argv_v1();
        argv.push("--shard-index".into());
        argv.push("3".into());
        argv.push("--shard-count".into());
        argv.push("8".into());
        let parsed = replay_v1(argv).unwrap();
        assert_eq!(
            parsed.shard,
            Some(TtsS1ShardSelectorV1::new_v1(3, 8).unwrap())
        );

        // One alone is a usage error, never a defaulted shard: a count
        // with no index would run shard 0 and look like a whole replay.
        for lone in [
            vec![OsString::from("--shard-index"), OsString::from("0")],
            vec![OsString::from("--shard-count"), OsString::from("8")],
        ] {
            let mut argv = argv_v1();
            argv.extend(lone);
            assert!(replay_v1(argv).is_err());
        }

        // Out of range in either direction, and a count of zero.
        for (index, count) in [("8", "8"), ("9", "8"), ("0", "0"), ("0", "65")] {
            let mut argv = argv_v1();
            argv.push("--shard-index".into());
            argv.push(index.into());
            argv.push("--shard-count".into());
            argv.push(count.into());
            assert!(
                replay_v1(argv).is_err(),
                "--shard-index {index} --shard-count {count} must be rejected"
            );
        }

        // The whole ladder of legal single-shard configurations parses.
        for count in [1u64, 8, 64] {
            let mut argv = argv_v1();
            argv.push("--shard-index".into());
            argv.push(format!("{}", count - 1).into());
            argv.push("--shard-count".into());
            argv.push(format!("{count}").into());
            assert_eq!(
                replay_v1(argv).unwrap().shard,
                TtsS1ShardSelectorV1::new_v1(count - 1, count)
            );
        }
    }

    /// The start barrier is a PAIR, and shard-only. A path with no deadline
    /// would be an unbounded wait, a deadline with no path would wait for
    /// nothing, and an unsharded run has nobody to wait for.
    #[test]
    fn the_start_barrier_flags_are_a_shard_only_pair_v1() {
        let mut argv = argv_v1();
        argv.push("--shard-index".into());
        argv.push("0".into());
        argv.push("--shard-count".into());
        argv.push("8".into());
        argv.push("--start-barrier".into());
        argv.push("barrier.token".into());
        argv.push("--start-barrier-timeout-seconds".into());
        argv.push("900".into());
        let parsed = replay_v1(argv).unwrap();
        assert_eq!(
            parsed.start_barrier,
            Some(TtsS1StartBarrierConfigV1 {
                path: PathBuf::from("barrier.token"),
                timeout_seconds: 900,
            })
        );

        // One alone is a usage error, in either direction.
        for lone in [
            vec![
                OsString::from("--start-barrier"),
                OsString::from("barrier.token"),
            ],
            vec![
                OsString::from("--start-barrier-timeout-seconds"),
                OsString::from("900"),
            ],
        ] {
            let mut argv = argv_v1();
            argv.push("--shard-index".into());
            argv.push("0".into());
            argv.push("--shard-count".into());
            argv.push("8".into());
            argv.extend(lone);
            assert!(replay_v1(argv).is_err());
        }

        // A zero deadline is not a bound; it is a wait that ends before it
        // begins, and a shard that never waited is a shard that ran alone.
        let mut argv = argv_v1();
        argv.push("--shard-index".into());
        argv.push("0".into());
        argv.push("--shard-count".into());
        argv.push("8".into());
        argv.push("--start-barrier".into());
        argv.push("barrier.token".into());
        argv.push("--start-barrier-timeout-seconds".into());
        argv.push("0".into());
        assert!(replay_v1(argv).is_err());

        // AND SHARD-ONLY: an unsharded run has nobody to wait for.
        let mut argv = argv_v1();
        argv.push("--start-barrier".into());
        argv.push("barrier.token".into());
        argv.push("--start-barrier-timeout-seconds".into());
        argv.push("900".into());
        assert!(replay_v1(argv).is_err());

        // A shard WITHOUT a barrier still parses: it is a legitimate smoke,
        // and the report refuses it formal standing rather than the parser.
        let mut argv = argv_v1();
        argv.push("--shard-index".into());
        argv.push("0".into());
        argv.push("--shard-count".into());
        argv.push("8".into());
        assert_eq!(replay_v1(argv).unwrap().start_barrier, None);
    }

    /// The merge takes three flags and refuses every replay-only one: a
    /// merge invoked with a tier would look like it had measured one.
    #[test]
    fn the_merge_mode_is_its_own_closed_flag_surface_v1() {
        let merge = vec![
            OsString::from("--merge-shards"),
            OsString::from("shards"),
            OsString::from("--shard-count"),
            OsString::from("8"),
            OsString::from("--output"),
            OsString::from("report.json"),
        ];
        assert_eq!(
            parse_args_v1(merge.clone()).unwrap(),
            ParsedArgsV1::Merge(MergeArgsV1 {
                shard_directory: PathBuf::from("shards"),
                shard_count: 8,
                output: PathBuf::from("report.json"),
            })
        );

        // Order independent, like every other invocation here.
        let mut reordered = merge.clone();
        reordered.rotate_left(2);
        assert_eq!(
            parse_args_v1(reordered).unwrap(),
            parse_args_v1(merge.clone()).unwrap()
        );

        for drop_pair in [0usize, 2, 4] {
            let mut partial = merge.clone();
            partial.drain(drop_pair..drop_pair + 2);
            assert!(
                parse_args_v1(partial).is_err(),
                "dropping the merge pair at {drop_pair} must be a usage error"
            );
        }

        for extra in [
            vec![OsString::from("--tier"), OsString::from("t512")],
            vec![OsString::from("--corpus"), OsString::from("corpus.json")],
            vec![OsString::from("--seed-block"), OsString::from("1")],
            vec![OsString::from("--max-episodes"), OsString::from("64")],
            vec![
                OsString::from("--diagnostics-dir"),
                OsString::from("diagnostics"),
            ],
            vec![OsString::from("--limit-episodes"), OsString::from("4")],
            vec![OsString::from("--shard-index"), OsString::from("0")],
            vec![
                OsString::from("--start-barrier"),
                OsString::from("barrier.token"),
            ],
            vec![
                OsString::from("--start-barrier-timeout-seconds"),
                OsString::from("900"),
            ],
            vec![
                OsString::from("--publish-start-barrier"),
                OsString::from("token"),
            ],
            vec![
                OsString::from("--population-store-root"),
                OsString::from("store"),
            ],
        ] {
            let mut argv = merge.clone();
            argv.extend(extra);
            assert!(
                parse_args_v1(argv).is_err(),
                "the merge must refuse a replay-only flag"
            );
        }
    }
}
