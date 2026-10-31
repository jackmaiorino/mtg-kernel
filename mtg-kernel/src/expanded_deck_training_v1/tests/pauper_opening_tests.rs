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
