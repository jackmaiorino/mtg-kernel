//! The measurement-only S learner profile (`s-learner-profile-v1`) counts
//! what the learner actually does and changes no result: its replay
//! multiply-adds equal the analytic weight-gradient census, its byte counters
//! equal the recorded term values, every span and drain is accounted for,
//! and the serial and parallel full Adam states stay identical.

use mtg_kernel::s_transformer_v1::profile::SProfileAllocatorV1;
use mtg_kernel::s_transformer_v1::tools::gae_bench_v1;
use serde_json::Value;

#[global_allocator]
static GLOBAL_ALLOCATOR: SProfileAllocatorV1 = SProfileAllocatorV1;

fn number(value: &Value, path: &[&str]) -> u64 {
    let mut cursor = value;
    for key in path {
        cursor = &cursor[*key];
    }
    cursor
        .as_u64()
        .unwrap_or_else(|| panic!("{path:?} is not a count: {cursor}"))
}

fn spans(profile: &Value, stage: &str) -> u64 {
    number(profile, &["stages", stage, "spans"])
}

fn sum(values: &Value) -> u64 {
    values
        .as_array()
        .expect("array")
        .iter()
        .map(|v| v.as_u64().expect("count"))
        .sum()
}

#[test]
fn profile_counts_match_the_recorded_work_and_change_no_result() {
    let seed = 7;
    let groups = 12;
    let serial = gae_bench_v1(seed, groups, 1, 1).unwrap();
    let parallel = gae_bench_v1(seed, groups, 2, 2).unwrap();
    assert_eq!(serial["after_state_sha256"], parallel["after_state_sha256"]);
    assert_eq!(
        serial["before_state_sha256"],
        parallel["before_state_sha256"]
    );
    let substeps = serial["substeps"].as_u64().unwrap();
    assert!(
        substeps > groups as u64,
        "some groups have several substeps"
    );

    let profile = &serial["profile"][0];
    assert_eq!(serial["profile"].as_array().unwrap().len(), 1);
    assert_eq!(profile["schema"], "s-learner-profile-v1");
    assert_eq!(profile["allocator_counted"], true);
    assert_eq!(number(profile, &["substeps"]), substeps);
    for stage in ["gradient", "adam", "state_hash"] {
        assert_eq!(spans(profile, stage), 1, "{stage}");
    }
    for stage in [
        "forward",
        "policy_row",
        "backward",
        "back_heads",
        "back_inputs",
    ] {
        assert_eq!(spans(profile, stage), substeps, "{stage}");
    }
    assert!(spans(profile, "sink_linear") >= 20 * substeps);
    for stage in [
        "record_drain",
        "score_merge",
        "replay_additions",
        "replay_drain",
    ] {
        assert_eq!(spans(profile, stage), 0, "serial {stage}");
    }
    assert_eq!(number(profile, &["terms", "linear_terms"]), 0);
    assert_eq!(number(profile, &["drains", "record", "drains"]), 0);
    assert!(profile["shards"].as_array().unwrap().is_empty());
    assert!(number(profile, &["stages", "gradient", "allocs"]) > 0);
    let sections = profile["backward_sections"].as_array().unwrap();
    assert_eq!(sections.len(), 3);
    assert_eq!(sections[2]["final_block"], true);
    for layer in sections {
        for section in ["ffn", "ln2", "proj", "attention", "qkv", "ln1"] {
            assert_eq!(number(layer, &[section, "spans"]), substeps, "{section}");
        }
    }

    let reports = parallel["profile"].as_array().unwrap();
    assert_eq!(reports.len(), 2);
    for profile in reports {
        assert_eq!(number(profile, &["substeps"]), substeps);
        for stage in ["gradient", "adam", "state_hash"] {
            assert_eq!(spans(profile, stage), 1, "{stage}");
        }
        for stage in ["forward", "policy_row", "backward"] {
            assert_eq!(spans(profile, stage), substeps, "{stage}");
        }
        let terms = &profile["terms"];
        let blocks = number(terms, &["blocks"]);
        assert_eq!(
            blocks,
            substeps.div_ceil(4),
            "blocks of two substeps per worker"
        );
        // A last block of one substep runs on the caller alone.
        let helpers = blocks - u64::from(substeps % 4 == 1);
        assert_eq!(number(terms, &["block_substeps"]), substeps);
        for stage in [
            "record_drain",
            "score_merge",
            "replay_additions",
            "replay_drain",
        ] {
            assert_eq!(spans(profile, stage), blocks, "{stage}");
        }
        // Every recorded Linear call is one sink span; every stored value is
        // one counted byte group.
        let linear_terms = number(terms, &["linear_terms"]);
        assert_eq!(spans(profile, "sink_linear"), linear_terms);
        assert_eq!(
            number(terms, &["block_value_bytes"]),
            number(terms, &["linear_input_bytes"])
                + number(terms, &["linear_output_gradient_bytes"])
                + number(terms, &["layer_norm_gain_bytes"])
                + number(terms, &["layer_norm_bias_bytes"])
                + number(terms, &["addition_bytes"])
        );
        assert!(number(terms, &["peak_block_live_bytes"]) > 0);
        assert!(
            number(terms, &["peak_block_reserved_bytes"])
                >= number(terms, &["peak_block_live_bytes"])
        );
        assert!(
            number(terms, &["peak_retained_term_capacity_bytes"])
                >= number(terms, &["peak_block_reserved_bytes"])
        );
        // Replay runs exactly the analytic weight-gradient multiply-adds.
        let shards = profile["shards"].as_array().unwrap();
        let replayed: u64 = shards.iter().map(|s| number(s, &["macs"])).sum();
        assert_eq!(replayed, number(profile, &["macs", "total", "dw"]));
        assert!(shards.iter().all(|s| number(s, &["visits"]) == blocks));
        let scanned: u64 = shards.iter().map(|s| number(s, &["terms_scanned"])).sum();
        assert_eq!(scanned, shards.len() as u64 * linear_terms);
        // Both drains per block, with one helper each and every helper joined.
        for kind in ["record", "replay"] {
            let drain = &profile["drains"][kind];
            assert_eq!(number(drain, &["drains"]), blocks, "{kind}");
            assert_eq!(number(drain, &["helper_spawns"]), helpers, "{kind}");
            assert_eq!(number(drain, &["spawn_failures"]), 0, "{kind}");
            assert_eq!(number(drain, &["joins"]), helpers, "{kind}");
            assert_eq!(drain["busy_ns_by_worker"].as_array().unwrap().len(), 2);
        }
        assert_eq!(
            sum(&profile["drains"]["record"]["items_by_worker"]),
            substeps
        );
        assert_eq!(
            sum(&profile["drains"]["replay"]["items_by_worker"]),
            shards.len() as u64 * blocks
        );
        let heap = &profile["heap"];
        assert!(number(heap, &["peak_live_bytes"]) >= number(heap, &["end_live_bytes"]));
        assert!(number(heap, &["peak_live_bytes"]) >= number(heap, &["start_live_bytes"]));
    }
}
