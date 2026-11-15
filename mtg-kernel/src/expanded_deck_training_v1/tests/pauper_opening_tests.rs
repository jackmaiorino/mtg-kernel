//! Pauper London mulligans v1 (docs/design/pauper_london_mulligans_v1.md,
//! sections 3 and 6): the episode's opt-in `opening`, its refusals, the
//! session it builds and the opening record's validation. Absent fields keep
//! every serialized byte.

use super::*;
use crate::rl_session::pauper_london_v1::PauperOpeningSeatRecordV1;
use PauperMulliganModeV1::{KeepSeven, Learned};

fn deck(id: &str) -> ExpandedDeckListV1 {
    let registered = crate::sideboard::checked_in_pauper_registered_deck_by_id_v1(id).unwrap();
    ExpandedDeckListV1 {
        label: id.into(),
        mainboard: registered.registered_configuration().mainboard().to_vec(),
        sideboard: registered.registered_configuration().sideboard().to_vec(),
    }
}

fn pin(name: &str) -> PinnedFileV1 {
    PinnedFileV1 {
        path: PathBuf::from(name),
        sha256: "ab".repeat(32),
    }
}

fn source() -> ExpandedModelSourceV1 {
    ExpandedModelSourceV1 {
        play_import: pin("import.json"),
        feature_transfer: FrozenPlayObservationTransferV3 {
            expected_feature_contract_digest: FEATURE_CONTRACT_DIGEST_V4.into(),
            expected_feature_encoding_digest: FEATURE_ENCODING_DIGEST_V4.into(),
        },
        checkpoint: None,
        s_learner: None,
    }
}

fn episode(opening: bool) -> ExpandedEpisodeV1 {
    ExpandedEpisodeV1 {
        id: "pauper-opening".into(),
        seed: 41,
        starting_player: 1,
        learner_seat: 0,
        opponent: None,
        opponent_kind: None,
        opponent_search: None,
        registered: [deck("Spy"), deck("Burn")],
        selected: [deck("Spy"), deck("Burn")],
        postboard: false,
        max_physical_decisions: 100_000,
        max_policy_steps: 1_000_000,
        runtime_rules: None,
        reset: None,
        opening: opening.then_some(PauperOpeningV1::LondonV1),
        mulligan_modes: opening.then_some([Learned, Learned]),
        london_keep_offset: None,
    }
}

#[test]
fn absent_opening_keeps_every_serialized_byte() {
    let plain = episode(false);
    let value = serde_json::to_value(&plain).unwrap();
    assert!(value.get("opening").is_none());
    assert!(value.get("mulligan_modes").is_none());
    let bytes = serde_json::to_vec(&plain).unwrap();
    let parsed: ExpandedEpisodeV1 = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(parsed, plain);
    assert_eq!(serde_json::to_vec(&parsed).unwrap(), bytes);
    let london = episode(true);
    let value = serde_json::to_value(&london).unwrap();
    assert_eq!(value["opening"], "london-v1");
    assert_eq!(
        value["mulligan_modes"],
        serde_json::json!(["learned", "learned"])
    );
    let parsed: ExpandedEpisodeV1 = serde_json::from_value(value).unwrap();
    assert_eq!(parsed, london);
    // An unknown protocol or mode is refused by the strict reader.
    let mut bad = serde_json::to_value(&london).unwrap();
    bad["opening"] = serde_json::json!("london-v2");
    assert!(serde_json::from_value::<ExpandedEpisodeV1>(bad).is_err());
    let mut bad = serde_json::to_value(&london).unwrap();
    bad["mulligan_modes"] = serde_json::json!(["learned", "vancouver"]);
    assert!(serde_json::from_value::<ExpandedEpisodeV1>(bad).is_err());
}

#[test]
fn opening_refusals_reset_search_legacy_modes_and_learner_keep_seven() {
    assert!(episode(true).configurations().is_ok());
    let refused = |edit: &dyn Fn(&mut ExpandedEpisodeV1), needle: &str| {
        let mut e = episode(true);
        edit(&mut e);
        let error = e.configurations_checked_v1().unwrap_err();
        assert!(error.contains(needle), "{error}");
        assert!(episode_session_v1(&e, &episode(false).configurations().unwrap()).is_err());
    };
    refused(
        &|e| {
            e.reset = Some(ExpandedResetStartV1 {
                guide: "g".into(),
                game_seed: 1,
                prefix: Vec::new(),
                cut_physical_decision_id: 0,
                cut_observation_sha256: String::new(),
                cut_menu_sha256: String::new(),
                cut_state_hash: String::new(),
            })
        },
        "reset start",
    );
    refused(
        &|e| {
            e.opponent = Some(source());
            e.opponent_search = Some(pin("search.json"));
        },
        "search opponent",
    );
    refused(
        &|e| {
            e.opponent_kind = Some(ExpandedOpponentKindV1::Legacy {
                source: source(),
                v3_forced_actions: false,
                v3_spell_target_reference_adapter: false,
                admission: pin("admission.json"),
            })
        },
        "Legacy opponent kind",
    );
    refused(&|e| e.mulligan_modes = None, "mulligan modes");
    refused(
        &|e| {
            e.opening = None;
        },
        "mulligan modes",
    );
    refused(
        &|e| e.mulligan_modes = Some([KeepSeven, Learned]),
        "learner seat",
    );
    // The opponent seat may keep seven.
    let mut frozen = episode(true);
    frozen.mulligan_modes = Some([Learned, KeepSeven]);
    assert!(frozen.configurations_checked_v1().is_ok());
}

#[test]
fn opening_episode_builds_the_session_owned_london_opening() {
    let london = episode(true);
    let configs = london.configurations().unwrap();
    let session = episode_session_v1(&london, &configs).unwrap();
    assert!(session.pauper_london_pending_v1());
    assert_eq!(session.pauper_london_modes_v1(), Some([Learned, Learned]));
    let FastActorResponseV1::Decision(d) = session.current_response() else {
        panic!("a London episode starts at a pregame decision");
    };
    // The starting player announces first.
    assert_eq!(seat(d.acting_player), london.starting_player);
    let plain = episode(false);
    let session = episode_session_v1(&plain, &plain.configurations().unwrap()).unwrap();
    assert!(!session.pauper_london_pending_v1());
    assert_eq!(session.pauper_london_modes_v1(), None);
}

#[test]
fn opening_requires_the_v7_identity_on_every_seat() {
    let v4 = FrozenPlayPolicyV1::training_fixture_v4();
    let v3 = FrozenPlayPolicyV1::training_fixture_v3();
    assert!(require_opening_feature_identity_v1(&episode(false), &[Some(&v4), Some(&v3)]).is_ok());
    for seats in [[Some(&v4), None], [Some(&v3), None], [Some(&v4), Some(&v3)]] {
        let error = require_opening_feature_identity_v1(&episode(true), &seats).unwrap_err();
        assert!(error.contains("FlatV7"), "{error}");
    }
    let v7 = FrozenPlayPolicyV1::training_fixture_v4()
        .into_flat_v7_transfer_v1()
        .unwrap();
    assert!(require_opening_feature_identity_v1(&episode(true), &[Some(&v7), Some(&v7)]).is_ok());
    assert!(require_opening_feature_identity_v1(&episode(true), &[Some(&v7), Some(&v4)]).is_err());
}

#[test]
fn flat_v7_transfer_requires_both_v7_digests() {
    use crate::native_flat_tensorizer_v7::{
        FEATURE_CONTRACT_DIGEST_V7, FEATURE_ENCODING_DIGEST_V7,
    };
    let mut declared = source();
    assert!(!flat_v7_transfer_requested_v1(&declared).unwrap());
    declared.feature_transfer.expected_feature_contract_digest = FEATURE_CONTRACT_DIGEST_V7.into();
    assert!(flat_v7_transfer_requested_v1(&declared).is_err());
    declared.feature_transfer.expected_feature_encoding_digest = FEATURE_ENCODING_DIGEST_V7.into();
    assert!(flat_v7_transfer_requested_v1(&declared).unwrap());
}

/// One complete London game collected by a V4 fixture transferred to V7:
/// pregame rows are ordinary rows with contiguous physical ids, the opening
/// record is written, collection validates the V7 trajectory and the update
/// readers resolve it to the V4 generation under the V7 identity. The
/// untransferred V4 policy refuses the episode.
#[test]
#[ignore = "root-owned native qualification: one real complete game"]
fn london_collection_with_the_v7_transfer_records_pregame_rows_and_the_opening() {
    let mut policy = FrozenPlayPolicyV1::training_fixture_v4()
        .into_flat_v7_transfer_v1()
        .unwrap();
    let learner = test_behavior(&policy, false);
    let london = episode(true);
    let trajectory = collect_episode(&mut policy, &learner, None, &london).unwrap();
    assert_eq!(
        trajectory.feature_contract_digest,
        crate::native_flat_tensorizer_v7::FEATURE_CONTRACT_DIGEST_V7
    );
    let record = trajectory.opening_record.clone().unwrap();
    let pregame: u32 = record
        .seats
        .iter()
        .map(|seat| seat.exposed_pregame_decisions)
        .sum();
    assert!(pregame >= 2, "{record:?}");
    for (index, row) in trajectory.decisions.iter().enumerate() {
        if index < pregame as usize {
            assert_eq!(row.physical_decision_id, index as u64);
            assert_eq!(row.tensor.state[89], 1.0f32.to_bits());
        }
    }
    validate_trajectory(&trajectory).unwrap();
    let contract = trajectory_contract_v1(&trajectory).unwrap();
    assert_eq!(contract.generation, FreshLineageGenerationV1::V4);
    assert_eq!(
        contract.base_contract_digest,
        crate::native_flat_tensorizer_v7::FEATURE_CONTRACT_DIGEST_V7
    );
    contract.require_policy_contract_v1(&policy).unwrap();
    assert!(contract
        .require_policy_contract_v1(&FrozenPlayPolicyV1::training_fixture_v4())
        .is_err());
    let value = serde_json::to_value(&trajectory).unwrap();
    assert_eq!(value["opening_record"]["opening"], "london-v1");
    let mut v4 = FrozenPlayPolicyV1::training_fixture_v4();
    let v4_learner = test_behavior(&v4, false);
    assert!(collect_episode(&mut v4, &v4_learner, None, &london).is_err());
}

#[test]
fn opening_record_must_match_the_episode_opening() {
    let seat = |mode, mulligans: u8, exposed| PauperOpeningSeatRecordV1 {
        mode,
        mulligans_taken: mulligans,
        kept_hand_size: 7 - mulligans,
        exposed_pregame_decisions: exposed,
        forced_zero_card_keep: mulligans == 7,
    };
    let record = PauperOpeningRecordV1 {
        opening: PauperOpeningV1::LondonV1,
        seats: [seat(Learned, 2, 5), seat(Learned, 0, 1)],
    };
    let london = episode(true);
    assert!(validate_opening_record_v1(&london, Some(&record)).is_ok());
    assert!(validate_opening_record_v1(&london, None).is_err());
    assert!(validate_opening_record_v1(&episode(false), Some(&record)).is_err());
    assert!(validate_opening_record_v1(&episode(false), None).is_ok());
    let mut wrong = record.clone();
    wrong.seats[0].kept_hand_size = 6;
    assert!(validate_opening_record_v1(&london, Some(&wrong)).is_err());
    let mut wrong = record.clone();
    wrong.seats[1].mode = KeepSeven;
    assert!(validate_opening_record_v1(&london, Some(&wrong)).is_err());
    let mut frozen = episode(true);
    frozen.mulligan_modes = Some([Learned, KeepSeven]);
    let mut kept = record.clone();
    kept.seats[1] = seat(KeepSeven, 0, 0);
    assert!(validate_opening_record_v1(&frozen, Some(&kept)).is_ok());
    kept.seats[1].exposed_pregame_decisions = 1;
    assert!(validate_opening_record_v1(&frozen, Some(&kept)).is_err());
}

/// FlatV7 records resolve to the V4 generation under their own identity and
/// bind only a FlatV7 policy; a V4 record binds only a policy that is not
/// one. A mixed pair is refused.
#[test]
fn flat_v7_records_resolve_to_the_v4_generation_and_bind_only_v7_policies() {
    use crate::native_flat_tensorizer_v7::{
        FEATURE_CONTRACT_DIGEST_V7, FEATURE_ENCODING_DIGEST_V7,
    };
    let cards = format!("{KERNEL_CARDDB_HASH:016x}");
    let v7_record = no_state_digest_v1::resolve_contract_v1(
        FEATURE_CONTRACT_DIGEST_V7,
        FEATURE_ENCODING_DIGEST_V7,
        &cards,
        None,
    )
    .unwrap();
    assert_eq!(v7_record.generation, FreshLineageGenerationV1::V4);
    assert_eq!(v7_record.base_contract_digest, FEATURE_CONTRACT_DIGEST_V7);
    assert!(no_state_digest_v1::resolve_contract_v1(
        FEATURE_CONTRACT_DIGEST_V7,
        FEATURE_ENCODING_DIGEST_V4,
        &cards,
        None
    )
    .is_err());
    assert!(no_state_digest_v1::resolve_contract_v1(
        FEATURE_CONTRACT_DIGEST_V7,
        FEATURE_ENCODING_DIGEST_V7,
        "0000000000000000",
        None
    )
    .is_err());
    let v4_record = no_state_digest_v1::resolve_contract_v1(
        FEATURE_CONTRACT_DIGEST_V4,
        FEATURE_ENCODING_DIGEST_V4,
        &cards,
        None,
    )
    .unwrap();
    let v4 = FrozenPlayPolicyV1::training_fixture_v4();
    let v7 = FrozenPlayPolicyV1::training_fixture_v4()
        .into_flat_v7_transfer_v1()
        .unwrap();
    assert!(v7_record.require_policy_contract_v1(&v7).is_ok());
    assert!(v7_record.require_policy_contract_v1(&v4).is_err());
    assert!(v4_record.require_policy_contract_v1(&v4).is_ok());
    assert!(v4_record.require_policy_contract_v1(&v7).is_err());
}

// London keep offset (`london_keep_offset_v1`).

fn scalar_offset(value: f64) -> london_keep_offset_v1::LondonKeepOffsetV1 {
    london_keep_offset_v1::LondonKeepOffsetV1::Scalar(value)
}

fn vector_offset(values: [f64; 7]) -> london_keep_offset_v1::LondonKeepOffsetV1 {
    london_keep_offset_v1::LondonKeepOffsetV1::PerCount(values)
}

#[test]
fn keep_offset_is_absent_by_default_and_refused_without_london() {
    let plain = episode(true);
    let value = serde_json::to_value(&plain).unwrap();
    assert!(value.get("london_keep_offset").is_none());
    let mut offset = episode(true);
    offset.london_keep_offset = Some(scalar_offset(6.5));
    let value = serde_json::to_value(&offset).unwrap();
    assert_eq!(value["london_keep_offset"], serde_json::json!(6.5));
    let parsed: ExpandedEpisodeV1 = serde_json::from_value(value).unwrap();
    assert_eq!(parsed, offset);
    assert!(offset.configurations_checked_v1().is_ok());
    // 15.3 as an f64 is not an f32 value: refused with the rest.
    for bad in [0.0, -2.0, f64::NAN, f64::INFINITY, 1.0e7, f64::MAX, 15.3] {
        let mut e = episode(true);
        e.london_keep_offset = Some(scalar_offset(bad));
        assert!(e.configurations_checked_v1().is_err());
    }
    let mut keep_seven = episode(false);
    keep_seven.london_keep_offset = Some(scalar_offset(6.5));
    let error = keep_seven.configurations_checked_v1().unwrap_err();
    assert!(error.contains("requires a London opening"), "{error}");
}

#[test]
fn keep_offset_json_round_trips_to_the_value_the_run_compares_against() {
    // A run resumed from saved trajectories compares each saved episode (parsed
    // JSON) with `serde_json::to_value` of the scheduled one. A non-dyadic
    // offset must survive that for every value a schedule writes.
    for b0 in [15.3003f32, 27.5723, 22.1944, 0.1, 1.0e-3, 123456.7] {
        for step in [1.0f32, 0.9999, 0.5, 0.123456] {
            let offset = f64::from(b0 * step);
            let mut e = episode(true);
            e.london_keep_offset = Some(scalar_offset(offset));
            e.configurations_checked_v1().unwrap();
            let text = serde_json::to_string(&e).unwrap();
            let reread: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(reread, serde_json::to_value(&e).unwrap(), "{text}");
            let parsed: ExpandedEpisodeV1 = serde_json::from_str(&text).unwrap();
            assert_eq!(parsed, e);
            assert_eq!(
                london_keep_offset_v1::offset_f32_v1(offset).unwrap(),
                b0 * step
            );
        }
    }
}

#[test]
fn keep_offset_batches_carry_one_value() {
    use london_keep_offset_v1::OffsetVectorV1;
    assert_eq!(batch_london_keep_offset_v1([None, None]).unwrap(), None);
    assert_eq!(batch_london_keep_offset_v1([]).unwrap(), None);
    assert_eq!(
        batch_london_keep_offset_v1([Some(scalar_offset(3.0)), Some(scalar_offset(3.0))]).unwrap(),
        Some(OffsetVectorV1::scalar(3.0))
    );
    assert!(batch_london_keep_offset_v1([Some(scalar_offset(3.0)), None]).is_err());
    assert!(batch_london_keep_offset_v1([None, Some(scalar_offset(3.0))]).is_err());
    assert!(
        batch_london_keep_offset_v1([Some(scalar_offset(3.0)), Some(scalar_offset(2.0))]).is_err()
    );
    // A scalar and seven equal entries are one value; the same vector twice
    // agrees; a vector differing in any one entry does not.
    assert_eq!(
        batch_london_keep_offset_v1([Some(scalar_offset(3.0)), Some(vector_offset([3.0; 7]))])
            .unwrap(),
        Some(OffsetVectorV1::scalar(3.0))
    );
    let base = [9.0, 8.0, 0.0, 6.0, 5.0, 4.0, 3.0];
    assert_eq!(
        batch_london_keep_offset_v1([Some(vector_offset(base)), Some(vector_offset(base))])
            .unwrap(),
        Some(OffsetVectorV1::per_count([
            9.0, 8.0, 0.0, 6.0, 5.0, 4.0, 3.0
        ]))
    );
    for index in 0..7 {
        let mut other = base;
        other[index] += 0.5;
        assert!(
            batch_london_keep_offset_v1([Some(vector_offset(base)), Some(vector_offset(other))])
                .is_err(),
            "entry {index}"
        );
        // A one-bit difference in one entry is a difference.
        let mut nudged = base;
        nudged[index] = f64::from(f32::from_bits((base[index] as f32).to_bits() + 1));
        assert!(
            batch_london_keep_offset_v1([Some(vector_offset(base)), Some(vector_offset(nudged))])
                .is_err(),
            "entry {index} one bit"
        );
    }
    assert!(batch_london_keep_offset_v1([Some(vector_offset(base)), None]).is_err());
    // A malformed member is an error whichever position it holds.
    assert!(batch_london_keep_offset_v1([None, Some(vector_offset([0.0; 7]))]).is_err());
    assert!(batch_london_keep_offset_v1([Some(vector_offset([0.0; 7])), None]).is_err());
}

#[test]
fn keep_offset_vector_is_written_read_and_refused_like_the_scalar() {
    let values = [15.25, 14.0, 0.0, 12.5, 11.0, 0.0, 9.75];
    let mut e = episode(true);
    e.london_keep_offset = Some(vector_offset(values));
    e.configurations_checked_v1().unwrap();
    let written = serde_json::to_value(&e).unwrap();
    assert_eq!(written["london_keep_offset"], serde_json::json!(values));
    let parsed: ExpandedEpisodeV1 = serde_json::from_value(written).unwrap();
    assert_eq!(parsed, e);
    // Without a London opening, the vector is refused as the scalar is.
    let mut keep_seven = episode(false);
    keep_seven.london_keep_offset = Some(vector_offset(values));
    let error = keep_seven.configurations_checked_v1().unwrap_err();
    assert!(error.contains("requires a London opening"), "{error}");
    // Every entry is checked, zero entries included and all-zero refused.
    for bad in [
        [0.0; 7],
        [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, -1.0],
        [1.0, 1.0, f64::NAN, 1.0, 1.0, 1.0, 1.0],
        [1.0, 1.0, 1.0, 1.0e7, 1.0, 1.0, 1.0],
        [1.0, 1.0, 1.0, 1.0, 15.3, 1.0, 1.0],
    ] {
        let mut e = episode(true);
        e.london_keep_offset = Some(vector_offset(bad));
        assert!(e.configurations_checked_v1().is_err(), "{bad:?}");
    }
    // A vector of any other length is not a field value.
    let mut value = serde_json::to_value(episode(true)).unwrap();
    for length in [0usize, 1, 6, 8] {
        value["london_keep_offset"] = serde_json::json!(vec![1.0f64; length]);
        assert!(serde_json::from_value::<ExpandedEpisodeV1>(value.clone()).is_err());
    }
}

#[test]
fn keep_offset_vector_json_round_trips_entry_by_entry() {
    // The resume comparison, per entry: every entry a schedule writes
    // (float32(float64(b_k) * (T - u) / T)) is an f64 holding exactly an f32.
    let bases = [15.3003f32, 27.5723, 0.0, 22.1944, 0.1, 1.0e-3, 123456.7];
    for step in [1.0f64, 0.99996948, 0.5, 0.123456, 3.0e-5] {
        let mut entries = [0.0f64; 7];
        for (entry, base) in entries.iter_mut().zip(bases) {
            *entry = f64::from((f64::from(base) * step) as f32);
        }
        let mut e = episode(true);
        e.london_keep_offset = Some(vector_offset(entries));
        e.configurations_checked_v1().unwrap();
        let text = serde_json::to_string(&e).unwrap();
        let reread: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(reread, serde_json::to_value(&e).unwrap(), "{text}");
        let parsed: ExpandedEpisodeV1 = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed, e);
        let resolved = parsed.london_keep_offset.unwrap().resolve_v1().unwrap();
        for (bits, entry) in resolved.bits().into_iter().zip(entries) {
            assert_eq!(f32::from_bits(bits), entry as f32);
        }
    }
}

/// The episode value a linear schedule stamps at fork update `u` (the chains
/// tools' rule: `float32(float64(b_k) * (T - u) / T)` per entry, no field from
/// `T` on), written here so the trainer is tested on every value the tools
/// can produce.
fn stamped_vector(
    base: [f64; 7],
    u: u64,
    end: u64,
) -> Option<london_keep_offset_v1::LondonKeepOffsetV1> {
    (u < end)
        .then(|| vector_offset(base.map(|b| f64::from((b * (end - u) as f64 / end as f64) as f32))))
}

#[test]
fn keep_offset_vector_schedule_values_are_accepted_until_the_end_update_and_absent_after() {
    let end = 32_768u64;
    let base = [15.3003, 14.0, 0.0, 12.4375, 11.0, 0.0, 9.1875];
    let mut previous_first_entry = f32::INFINITY;
    for u in [
        0u64,
        1,
        2,
        100,
        16_384,
        32_766,
        end - 1,
        end,
        end + 1,
        40_000,
    ] {
        let mut e = episode(true);
        e.london_keep_offset = stamped_vector(base, u, end);
        e.configurations_checked_v1().unwrap();
        let text = serde_json::to_string(&e).unwrap();
        let reread: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(reread, serde_json::to_value(&e).unwrap(), "u {u}: {text}");
        let parsed: ExpandedEpisodeV1 = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed, e);
        if u < end {
            let resolved = parsed.london_keep_offset.unwrap().resolve_v1().unwrap();
            let bits = resolved.bits();
            // The last update before the end still carries a positive entry
            // everywhere the base is positive, and zero entries stay zero.
            for (index, base) in base.iter().enumerate() {
                assert_eq!(
                    f32::from_bits(bits[index]) > 0.0,
                    *base > 0.0,
                    "u {u} entry {index}"
                );
            }
            let first = f32::from_bits(bits[0]);
            assert!(first < previous_first_entry || u == 0, "u {u}");
            previous_first_entry = first;
        } else {
            // From the end update on the field is absent: not written, so the
            // episode bytes are those of an episode that never had it.
            assert!(e.london_keep_offset.is_none());
            assert!(!text.contains("london_keep_offset"), "u {u}");
        }
    }
    // One value per update: episodes of one update agree, those of two
    // updates do not.
    let a = stamped_vector(base, 5, end);
    let b = stamped_vector(base, 6, end);
    assert!(batch_london_keep_offset_v1([a, a]).is_ok());
    assert!(batch_london_keep_offset_v1([a, b]).is_err());
}

#[test]
#[ignore = "root-owned native qualification: real complete games"]
fn keep_offset_steers_announcements_records_raw_logits_and_validates() {
    let mut policy = FrozenPlayPolicyV1::training_fixture_v4()
        .into_flat_v7_transfer_v1()
        .unwrap();
    let learner = test_behavior(&policy, false);
    let mut steered = episode(true);
    // Far beyond any logit gap: every learned announcement keeps.
    steered.london_keep_offset = Some(scalar_offset(1.0e4));
    let trajectory = collect_episode(&mut policy, &learner, None, &steered).unwrap();
    let record = trajectory.opening_record.clone().unwrap();
    for seat in &record.seats {
        assert_eq!(seat.mulligans_taken, 0, "{record:?}");
        assert_eq!(seat.exposed_pregame_decisions, 1, "{record:?}");
    }
    // Stored logits are the raw scorer output: the replay check reproduces
    // them, and the announcement rows' mulligan logit is not offset.
    validate_trajectory(&trajectory).unwrap();
    for row in &trajectory.decisions[..2] {
        let tensor = row.tensor.tensor();
        let raw: Vec<f32> = row.logits.iter().map(|b| f32::from_bits(*b)).collect();
        let rows = london_keep_offset_v1::mulligan_rows_v1(
            &tensor.state,
            &tensor.action_features,
            raw.len(),
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert!(
            raw[rows[0]] > -1.0e3,
            "raw logit recorded, not the offset one"
        );
    }
    // A moderate offset with the policy's legacy sampler: the announcement
    // rows are drawn from the unclamped sampler, every other row from the
    // legacy one, and the trajectory validates (the re-draw agrees) with the
    // legacy sampler identity unchanged on every row.
    for seed in [41u64, 42, 43, 44] {
        let mut moderate = episode(true);
        moderate.seed = seed;
        moderate.london_keep_offset = Some(scalar_offset(12.0));
        let mut policy = FrozenPlayPolicyV1::training_fixture_v4()
            .into_flat_v7_transfer_v1()
            .unwrap();
        let collected = collect_episode(&mut policy, &learner, None, &moderate).unwrap();
        validate_trajectory(&collected).unwrap();
        assert!(collected.learner_sampler.is_none());
        assert!(collected.decisions.iter().all(
            |row| row.sampler_identity.as_deref() != Some(UNCLAMPED_SOFTMAX_SAMPLER_VERSION_V1)
        ));
    }
    // An episode without the offset collects exactly as before: same
    // trajectory bytes as a collection that never knew the field.
    let plain = episode(true);
    let mut again = FrozenPlayPolicyV1::training_fixture_v4()
        .into_flat_v7_transfer_v1()
        .unwrap();
    let first = collect_episode(&mut again, &learner, None, &plain).unwrap();
    let mut fresh = FrozenPlayPolicyV1::training_fixture_v4()
        .into_flat_v7_transfer_v1()
        .unwrap();
    let second = collect_episode(&mut fresh, &learner, None, &plain).unwrap();
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
    assert!(serde_json::to_value(&first).unwrap()["episode"]
        .get("london_keep_offset")
        .is_none());
}
