use mtg_kernel::fast_sampler::{
    splitmix64_first, FastCategoricalError, FastCategoricalScratch, FAST_CATEGORICAL_EXP_TABLE_Q63,
    FAST_CATEGORICAL_EXP_TABLE_SHA256, FAST_CATEGORICAL_MASS_TOTAL, FAST_CATEGORICAL_MAX_ACTIONS,
    FAST_CATEGORICAL_SAMPLER_CONTRACT_JSON, FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256,
    FAST_CATEGORICAL_SAMPLER_VERSION,
};
use sha2::{Digest, Sha256};
use std::sync::{mpsc, Arc};
use std::thread;

const ORACLE_BYTES: &[u8] = include_bytes!("../../data/fast_sampler_decimal_oracle_v1.json");
const ORACLE_SHA256: &str = "bb42f0cacae9902d67851941678cf2fb34a90cb8459403126a8026085dcae033";
const CANDIDATE_VECTOR_BYTES: &[u8] =
    include_bytes!("../../data/fast_sampler_candidate_vectors_v1.json");
const CANDIDATE_VECTOR_SHA256: &str =
    "bb928feeb7e34221ebccad38d15b0e24fe4ea5ff6ea61fa54da8055269c249d5";

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn decode_lower_hex(encoded: &str) -> Vec<u8> {
    assert!(encoded.len().is_multiple_of(2));
    assert!(encoded
        .bytes()
        .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value)));
    encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[test]
fn contract_table_and_oracle_digests_are_exact() {
    assert_eq!(
        sha256_hex(FAST_CATEGORICAL_SAMPLER_CONTRACT_JSON.as_bytes()),
        FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256
    );
    let mut table_digest = Sha256::new();
    for value in FAST_CATEGORICAL_EXP_TABLE_Q63 {
        table_digest.update(value.to_le_bytes());
    }
    assert_eq!(
        format!("{:x}", table_digest.finalize()),
        FAST_CATEGORICAL_EXP_TABLE_SHA256
    );
    assert_eq!(sha256_hex(ORACLE_BYTES), ORACLE_SHA256);
}

#[test]
fn schema_two_fixture_binds_observed_workload_only_provenance_and_independent_rng_goldens() {
    let fixture: serde_json::Value = serde_json::from_slice(ORACLE_BYTES).unwrap();
    assert_eq!(fixture["schema_version"], 2);
    assert_eq!(
        fixture["workload_width_profile"]["status"],
        "observed_provenance_bound"
    );
    assert_eq!(fixture["workload_width_profile"]["claim_eligible"], true);
    assert_eq!(
        fixture["workload_width_profile"]["scope"],
        "all_sampled_policy_decisions_in_rally_vs_rally_not_learner_only"
    );
    assert_eq!(
        fixture["workload_width_profile"]["source_performance_gate_valid"],
        false
    );
    assert_eq!(
        fixture["workload_width_profile"]["source_performance_rates_included"],
        false
    );
    assert_eq!(
        fixture["workload_width_profile"]["source_coverage_scope"],
        "rally_vs_rally_only_not_nine_deck_coverage"
    );
    assert_eq!(
        fixture["workload_width_profile"]["final_all_nine_deck_gate"],
        "deferred"
    );
    assert!(fixture.get("predeclared_candidate_bounds").is_some());

    let goldens = &fixture["independent_rng_and_selection_goldens"];
    assert_eq!(goldens["seed_range"]["inclusive_start"], 0);
    assert_eq!(goldens["seed_range"]["exclusive_end"], 4096);
    let encoded = goldens["splitmix_first_draws"]["bytes_hex"]
        .as_str()
        .unwrap();
    let bytes = decode_lower_hex(encoded);
    assert_eq!(bytes.len(), 4096 * 8);
    assert_eq!(
        sha256_hex(&bytes),
        goldens["splitmix_first_draws"]["sha256"].as_str().unwrap()
    );
    for (seed, chunk) in bytes.chunks_exact(8).enumerate() {
        assert_eq!(
            splitmix64_first(seed as u64),
            u64::from_le_bytes(chunk.try_into().unwrap())
        );
    }
}

#[test]
fn exact_candidate_mass_and_selection_goldens_are_pinned() {
    let mut scratch = FastCategoricalScratch::default();
    assert_eq!(
        scratch.apportion(&[0.0, 1.0, 2.0]).unwrap(),
        &[
            1_660_770_942_083_389_844,
            4_514_443_473_098_088_106,
            12_271_529_658_528_073_666,
        ]
    );
    assert_eq!(
        scratch.apportion(&[100.0, 101.0, 102.0]).unwrap(),
        &[
            1_660_770_942_083_389_844,
            4_514_443_473_098_088_106,
            12_271_529_658_528_073_666,
        ]
    );
    assert_eq!(
        scratch.apportion(&[0.0, 0.0, 0.0]).unwrap(),
        &[
            6_148_914_691_236_517_206,
            6_148_914_691_236_517_205,
            6_148_914_691_236_517_205,
        ]
    );
    let expected = [2, 2, 2, 1, 2, 2, 2, 0];
    let seeds = [0, 1, 2, 3, 4, 5, u64::MAX, 0x0123_4567_89ab_cdef];
    for (seed, expected_index) in seeds.into_iter().zip(expected) {
        assert_eq!(
            scratch.sample(&[0.0, 1.0, 2.0], seed).unwrap(),
            expected_index
        );
    }
}

#[test]
fn cross_language_candidate_vectors_match_production_sampler_exactly() {
    assert_eq!(sha256_hex(CANDIDATE_VECTOR_BYTES), CANDIDATE_VECTOR_SHA256);
    let fixture: serde_json::Value = serde_json::from_slice(CANDIDATE_VECTOR_BYTES).unwrap();
    assert_eq!(
        fixture["schema"],
        "mtg-kernel-fast-sampler-cross-language-vectors/v1"
    );
    assert_eq!(
        fixture["sampler_identity"],
        FAST_CATEGORICAL_SAMPLER_VERSION
    );
    assert_eq!(
        fixture["sampler_contract_sha256"],
        FAST_CATEGORICAL_SAMPLER_CONTRACT_SHA256
    );
    assert_eq!(
        fixture["exp_table_sha256"],
        FAST_CATEGORICAL_EXP_TABLE_SHA256
    );
    assert_eq!(fixture["case_count"], 11);
    assert_eq!(fixture["seed_count_per_case"], 7);
    assert_eq!(fixture["rejection_count"], 5);

    let cases = fixture["cases"].as_array().unwrap();
    let expected_names = [
        "width-one",
        "width-two-ordered",
        "hamilton-exact-remainder-tie",
        "equal-tie-order",
        "repeated-weight-legal-order",
        "q8-halfway-neighbors",
        "clamp-neighborhood",
        "finite-extremes",
        "signed-zero-and-subnormal",
        "large-nearby-finite",
        "maximum-admitted-width",
    ];
    let mut stream = Vec::new();
    let domain = b"mtg-kernel-fast-sampler-cross-language-vectors-v1";
    stream.extend_from_slice(&u32::try_from(domain.len()).unwrap().to_be_bytes());
    stream.extend_from_slice(domain);
    stream.extend_from_slice(&u32::try_from(cases.len()).unwrap().to_be_bytes());
    let mut scratch = FastCategoricalScratch::default();
    for (case, expected_name) in cases.iter().zip(expected_names) {
        let name = case["name"].as_str().unwrap();
        assert_eq!(name, expected_name);
        stream.extend_from_slice(&u32::try_from(name.len()).unwrap().to_be_bytes());
        stream.extend_from_slice(name.as_bytes());

        let bits = case["logit_bits_hex"]
            .as_array()
            .unwrap()
            .iter()
            .map(|encoded| u32::from_str_radix(encoded.as_str().unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        stream.extend_from_slice(&u32::try_from(bits.len()).unwrap().to_be_bytes());
        for value in &bits {
            stream.extend_from_slice(&value.to_be_bytes());
        }
        let logits = bits.iter().copied().map(f32::from_bits).collect::<Vec<_>>();
        let expected_masses = case["mass_u128"]
            .as_array()
            .unwrap()
            .iter()
            .map(|encoded| encoded.as_str().unwrap().parse::<u128>().unwrap())
            .collect::<Vec<_>>();
        for mass in &expected_masses {
            stream.extend_from_slice(&mass.to_be_bytes());
        }
        assert_eq!(scratch.apportion(&logits).unwrap(), expected_masses);

        let draws = case["draws"].as_array().unwrap();
        stream.extend_from_slice(&u32::try_from(draws.len()).unwrap().to_be_bytes());
        for draw in draws {
            let seed = draw["seed_u64"].as_str().unwrap().parse::<u64>().unwrap();
            let expected_draw =
                u64::from_str_radix(draw["splitmix_draw_hex"].as_str().unwrap(), 16).unwrap();
            let expected_index = u32::try_from(draw["selected_index"].as_u64().unwrap()).unwrap();
            stream.extend_from_slice(&seed.to_be_bytes());
            stream.extend_from_slice(&expected_draw.to_be_bytes());
            stream.extend_from_slice(&expected_index.to_be_bytes());
            assert_eq!(splitmix64_first(seed), expected_draw);
            assert_eq!(
                scratch.sample(&logits, seed).unwrap(),
                expected_index as usize
            );
        }
    }
    let rejections = fixture["rejections"].as_array().unwrap();
    stream.extend_from_slice(&u32::try_from(rejections.len()).unwrap().to_be_bytes());
    for rejection in rejections {
        let name = rejection["name"].as_str().unwrap();
        stream.extend_from_slice(&u32::try_from(name.len()).unwrap().to_be_bytes());
        stream.extend_from_slice(name.as_bytes());
        let bits = rejection["logit_bits_hex"]
            .as_array()
            .unwrap()
            .iter()
            .map(|encoded| u32::from_str_radix(encoded.as_str().unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        stream.extend_from_slice(&u32::try_from(bits.len()).unwrap().to_be_bytes());
        for value in &bits {
            stream.extend_from_slice(&value.to_be_bytes());
        }
        let error = &rejection["expected_error"];
        let code = error["code"].as_str().unwrap();
        stream.extend_from_slice(&u32::try_from(code.len()).unwrap().to_be_bytes());
        stream.extend_from_slice(code.as_bytes());
        let index = error
            .get("index")
            .and_then(serde_json::Value::as_u64)
            .map(|value| u32::try_from(value).unwrap())
            .unwrap_or(u32::MAX);
        let error_bits = error
            .get("bits_hex")
            .and_then(serde_json::Value::as_str)
            .map(|encoded| u32::from_str_radix(encoded, 16).unwrap())
            .unwrap_or(0);
        let maximum = error
            .get("maximum")
            .and_then(serde_json::Value::as_u64)
            .map(|value| u32::try_from(value).unwrap())
            .unwrap_or(0);
        stream.extend_from_slice(&index.to_be_bytes());
        stream.extend_from_slice(&error_bits.to_be_bytes());
        stream.extend_from_slice(&maximum.to_be_bytes());

        let logits = bits.iter().copied().map(f32::from_bits).collect::<Vec<_>>();
        let actual = scratch.apportion(&logits).unwrap_err();
        let expected = match code {
            "empty" => FastCategoricalError::Empty,
            "width_exceeded" => FastCategoricalError::WidthExceeded {
                width: logits.len(),
                maximum: usize::try_from(maximum).unwrap(),
            },
            "nonfinite" => FastCategoricalError::NonFinite {
                index: usize::try_from(index).unwrap(),
                bits: error_bits,
            },
            other => panic!("unexpected negative-vector error code {other}"),
        };
        assert_eq!(actual, expected, "negative vector {name}");
    }
    assert_eq!(
        sha256_hex(&stream),
        fixture["vector_stream_sha256"].as_str().unwrap()
    );
}

#[test]
fn width_one_ties_extremes_and_boundaries_preserve_exact_mass() {
    let mut scratch = FastCategoricalScratch::default();
    assert_eq!(
        scratch.apportion(&[f32::MAX]).unwrap(),
        &[FAST_CATEGORICAL_MASS_TOTAL]
    );
    assert_eq!(scratch.sample(&[f32::MAX], u64::MAX).unwrap(), 0);

    assert_eq!(
        scratch.apportion(&[0.0, -16.0, -17.0]).unwrap(),
        &[
            18_446_739_921_895_351_402,
            2_075_907_100_107,
            2_075_907_100_107,
        ]
    );
    assert_eq!(
        scratch
            .apportion(&[f32::MAX, -f32::MAX, 0.0, -1.0])
            .unwrap(),
        &[
            18_446_737_845_988_952_133,
            2_075_906_866_495,
            2_075_906_866_494,
            2_075_906_866_494,
        ]
    );

    let maximum_width_logits =
        core::array::from_fn::<_, FAST_CATEGORICAL_MAX_ACTIONS, _>(|index| {
            -(((index * 37) % 4_097) as f32 / 256.0)
        });
    let masses = scratch.apportion(&maximum_width_logits).unwrap();
    assert_eq!(masses.len(), FAST_CATEGORICAL_MAX_ACTIONS);
    assert_eq!(
        masses.iter().copied().sum::<u128>(),
        FAST_CATEGORICAL_MASS_TOTAL
    );
    assert_eq!(masses[0], 2_482_656_302_468_097_058);
    assert_eq!(masses[63], 275_715_871_570_928);
}

#[test]
fn invalid_inputs_fail_closed() {
    let mut scratch = FastCategoricalScratch::default();
    assert_eq!(scratch.apportion(&[]), Err(FastCategoricalError::Empty));
    let too_wide = [0.0_f32; FAST_CATEGORICAL_MAX_ACTIONS + 1];
    assert_eq!(
        scratch.apportion(&too_wide),
        Err(FastCategoricalError::WidthExceeded {
            width: FAST_CATEGORICAL_MAX_ACTIONS + 1,
            maximum: FAST_CATEGORICAL_MAX_ACTIONS,
        })
    );
    for nonfinite in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            scratch.apportion(&[0.0, nonfinite]),
            Err(FastCategoricalError::NonFinite {
                index: 1,
                bits: nonfinite.to_bits(),
            })
        );
    }
}

fn task_logits(task: usize) -> Vec<f32> {
    let width = 1 + (task * 37 % 15);
    (0..width)
        .map(|index| {
            let numerator = ((task * 17 + index * 29) % 127) as i32 - 63;
            numerator as f32 / 23.0
        })
        .collect()
}

fn scheduled_results(logits: Arc<Vec<Vec<f32>>>, stride: usize) -> Vec<usize> {
    const THREADS: usize = 16;
    let task_count = logits.len();
    let (sender, receiver) = mpsc::channel();
    let handles = (0..THREADS)
        .map(|worker| {
            let sender = sender.clone();
            let logits = Arc::clone(&logits);
            thread::spawn(move || {
                let mut scratch = FastCategoricalScratch::default();
                for task in 0..task_count {
                    if (task * stride) % THREADS != worker {
                        continue;
                    }
                    if (task + worker) % 3 == 0 {
                        thread::yield_now();
                    }
                    let seed = (task as u64)
                        .wrapping_mul(0xd134_2543_de82_ef95)
                        .wrapping_add(0x9e37_79b9_7f4a_7c15);
                    sender
                        .send((task, scratch.sample(&logits[task], seed).unwrap()))
                        .unwrap();
                }
            })
        })
        .collect::<Vec<_>>();
    drop(sender);
    let mut results = vec![usize::MAX; task_count];
    for (task, selected) in receiver {
        results[task] = selected;
    }
    for handle in handles {
        handle.join().unwrap();
    }
    assert!(results.iter().all(|result| *result != usize::MAX));
    results
}

#[test]
fn cross_thread_and_schedule_repeatability_is_exact() {
    let logits = Arc::new((0..2_048).map(task_logits).collect::<Vec<_>>());
    let mut scratch = FastCategoricalScratch::default();
    let baseline = logits
        .iter()
        .enumerate()
        .map(|(task, values)| {
            let seed = (task as u64)
                .wrapping_mul(0xd134_2543_de82_ef95)
                .wrapping_add(0x9e37_79b9_7f4a_7c15);
            scratch.sample(values, seed).unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(scheduled_results(Arc::clone(&logits), 1), baseline);
    assert_eq!(scheduled_results(logits, 7), baseline);
}

#[test]
fn splitmix_first_outputs_are_exact() {
    assert_eq!(splitmix64_first(0), 0xe220_a839_7b1d_cdaf);
    assert_eq!(splitmix64_first(1), 0x910a_2dec_8902_5cc1);
    assert_eq!(splitmix64_first(u64::MAX), 0xe4d9_7177_1b65_2c20);
}
