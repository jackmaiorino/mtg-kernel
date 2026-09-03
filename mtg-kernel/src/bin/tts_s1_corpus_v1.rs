//! Test-time-search wrapper, stage S1: the frozen corpus builder
//! (`LEAD_TEST_TIME_SEARCH_DESIGN_SKETCH_V2.md` Section 5, S1).
//!
//! Plays seeded CPU self-play from one checkpoint with both seats on that
//! same checkpoint, labels every decision, selects the pre-registered
//! stratified corpus, and publishes it as an immutable canonical JSON
//! manifest. It never searches and it never touches CP7.
//!
//! Every input is an explicit flag. No environment variable is read for
//! configuration, and there is no default for the seed block, the episode
//! count, or the output path: each is a pre-registered choice, and a
//! silently defaulted one would be a choice made by accident.

use mtg_kernel::native_checkpoint_shadow_stdio_v1::ShadowCheckpointAuthorityV1;
use mtg_kernel::native_tts_s1_corpus_v1::{
    build_tts_s1_corpus_v1, publish_tts_s1_corpus_v1, TtsS1CorpusConfigV1,
};
use std::ffi::OsString;
use std::path::PathBuf;

fn usage_v1() -> ! {
    eprintln!(
        "usage: tts_s1_corpus_v1 (--original-store-root PATH [--generation N] | --population-store-root PATH --generation N | --portable-derivative-root PATH) --seed-block N --episodes N --output PATH"
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
    seed_block_id: usize,
    episode_count: u64,
    output: PathBuf,
}

fn parse_args_v1(raw: Vec<OsString>) -> Result<ParsedArgsV1, ()> {
    if raw.len() < 8 || raw.len() > 10 || !raw.len().is_multiple_of(2) {
        return Err(());
    }
    let mut authority_root = None;
    let mut generation = None;
    let mut seed_block_id = None;
    let mut episode_count = None;
    let mut output = None;
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
        } else if flag == "--seed-block" && seed_block_id.is_none() {
            seed_block_id = Some(
                pair[1]
                    .to_str()
                    .ok_or(())?
                    .parse::<usize>()
                    .map_err(|_| ())?,
            );
        } else if flag == "--episodes" && episode_count.is_none() {
            episode_count = Some(pair[1].to_str().ok_or(())?.parse::<u64>().map_err(|_| ())?);
        } else if flag == "--output" && output.is_none() {
            output = Some(PathBuf::from(&pair[1]));
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
        seed_block_id: seed_block_id.ok_or(())?,
        episode_count: episode_count.ok_or(())?,
        output: output.ok_or(())?,
    })
}

fn main() {
    let raw = std::env::args_os().skip(1).collect();
    let parsed = parse_args_v1(raw).unwrap_or_else(|()| usage_v1());
    let config = TtsS1CorpusConfigV1 {
        authority: parsed.authority,
        seed_block_id: parsed.seed_block_id,
        episode_count: parsed.episode_count,
    };
    let manifest = match build_tts_s1_corpus_v1(&config) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("TTS_S1_CORPUS_FAILED {error}");
            std::process::exit(1);
        }
    };
    let bytes = match publish_tts_s1_corpus_v1(&manifest, &parsed.output) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("TTS_S1_CORPUS_FAILED {error}");
            std::process::exit(1);
        }
    };
    println!(
        "TTS_S1_CORPUS_PUBLISHED path={} bytes={} corpus_sha256={} decisions={} candidates={} episodes={} natural_terminal_episodes={} contributing_episodes={} contributing_episode_decisions={} seed_block_id={} seed_block_seed={}",
        parsed.output.display(),
        bytes.len(),
        manifest.corpus_sha256,
        manifest.body.decisions.len(),
        manifest.body.candidate_count,
        manifest.body.episode_count,
        manifest.body.natural_terminal_episode_count,
        // THE REPLAY'S OWN SIZE. A per-tier replay runs the contributing
        // episodes WHOLE, so this decision count, not the 512 stratified
        // targets, is what a tier costs and what the launcher shards over.
        manifest.body.contributing_episode_count,
        manifest.body.contributing_episode_decisions,
        manifest.body.seed_block_id,
        manifest.body.seed_block_seed,
    );
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
            "--seed-block".into(),
            "0".into(),
            "--episodes".into(),
            "64".into(),
            "--output".into(),
            "corpus.json".into(),
        ]
    }

    #[test]
    fn every_authority_shape_parses_and_is_order_independent_v1() {
        let parsed = parse_args_v1(argv_v1()).unwrap();
        assert!(matches!(
            parsed.authority,
            ShadowCheckpointAuthorityV1::PopulationStoreGeneration {
                generation: 1024,
                ..
            }
        ));
        assert_eq!(parsed.seed_block_id, 0);
        assert_eq!(parsed.episode_count, 64);
        assert_eq!(parsed.output, PathBuf::from("corpus.json"));

        let mut reordered = argv_v1();
        reordered.rotate_left(4);
        assert_eq!(parse_args_v1(reordered).unwrap(), parsed);

        let parsed = parse_args_v1(vec![
            "--original-store-root".into(),
            "store".into(),
            "--seed-block".into(),
            "1".into(),
            "--episodes".into(),
            "8".into(),
            "--output".into(),
            "corpus.json".into(),
        ])
        .unwrap();
        assert!(matches!(
            parsed.authority,
            ShadowCheckpointAuthorityV1::OriginalPromoted2Generation384Store { .. }
        ));

        let parsed = parse_args_v1(vec![
            "--portable-derivative-root".into(),
            "portable".into(),
            "--seed-block".into(),
            "3".into(),
            "--episodes".into(),
            "8".into(),
            "--output".into(),
            "corpus.json".into(),
        ])
        .unwrap();
        assert!(matches!(
            parsed.authority,
            ShadowCheckpointAuthorityV1::PortablePromoted2WeightsGenesis { .. }
        ));
    }

    #[test]
    fn the_flag_surface_is_strict_v1() {
        // Every required flag is required.
        let full = argv_v1();
        for drop_pair in [0usize, 4, 6, 8] {
            let mut partial = full.clone();
            partial.drain(drop_pair..drop_pair + 2);
            assert!(
                parse_args_v1(partial).is_err(),
                "dropping the pair at {drop_pair} must be a usage error"
            );
        }
        // A repeated flag is a usage error, never a last-one-wins.
        let mut repeated = full.clone();
        repeated.push("--seed-block".into());
        repeated.push("1".into());
        assert!(parse_args_v1(repeated).is_err());
        // Unknown flags are rejected.
        let mut unknown = full.clone();
        unknown.push("--tier".into());
        unknown.push("t512".into());
        assert!(parse_args_v1(unknown).is_err());
        // Non-numeric numbers are rejected rather than defaulted.
        for index in [5usize, 7] {
            let mut bad = full.clone();
            bad[index] = "many".into();
            assert!(parse_args_v1(bad).is_err());
        }
        // A generation is meaningless for the portable genesis authority.
        assert!(parse_args_v1(vec![
            "--portable-derivative-root".into(),
            "portable".into(),
            "--generation".into(),
            "3".into(),
            "--seed-block".into(),
            "0".into(),
            "--episodes".into(),
            "8".into(),
            "--output".into(),
            "corpus.json".into(),
        ])
        .is_err());
        // A population Store always needs one.
        assert!(parse_args_v1(vec![
            "--population-store-root".into(),
            "store".into(),
            "--seed-block".into(),
            "0".into(),
            "--episodes".into(),
            "8".into(),
            "--output".into(),
            "corpus.json".into(),
        ])
        .is_err());
        // Nothing turns this bin on without an authority root.
        assert!(parse_args_v1(vec![
            "--seed-block".into(),
            "0".into(),
            "--episodes".into(),
            "8".into(),
            "--output".into(),
            "corpus.json".into(),
        ])
        .is_err());
    }
}
