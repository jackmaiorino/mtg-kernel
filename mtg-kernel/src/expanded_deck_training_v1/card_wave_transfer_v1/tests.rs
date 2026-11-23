//! Card-wave transfer on small synthetic registries and the real net-8
//! train-state layout. Engineering verification only, never a card wave.

use super::*;
use crate::expanded_deck_training_v1::tests::checkpoint_fixture_v1;

const OLD_CARD_DB_HASH: &str = "0123456789abcdef";
const SEED: u64 = 20_261_123;

fn card(name: &str, mana_value: u64) -> Value {
    serde_json::json!({
        "name": name,
        "mana_cost": "{1}",
        "mana_value": mana_value,
        "types": ["Instant"],
        "is_land": false,
        "decks": ["Synthetic"],
        "complexity": "low",
    })
}

fn segment(cards: Vec<Value>) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({ "version": 1, "cards": cards })).unwrap()
}

fn named(names: &[&str]) -> Vec<u8> {
    segment(names.iter().map(|name| card(name, 1)).collect())
}

fn slices(files: &[Vec<u8>]) -> Vec<&[u8]> {
    files.iter().map(Vec::as_slice).collect()
}

fn current_hash() -> String {
    format!("{KERNEL_CARDDB_HASH:016x}")
}

/// A two-segment source registry (ids 0..3) and its wave: a third segment
/// appending ids 3 and 4, as `eternal-fixtures` appends after Standard.
fn wave_registries() -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let source = vec![named(&["Alpha", "Beta"]), named(&["Gamma"])];
    let mut destination = source.clone();
    destination.push(named(&["Delta", "Epsilon"]));
    (source, destination)
}

/// The trainer's checkpoint fixture with learned-looking optimizer state on
/// every tensor and on the source cards' rows, nothing on still-unassigned
/// rows, the padding row or the gauge, and a non-zero Adam step.
fn learned_checkpoint(card_db_hash: &str, source_cards: usize) -> ExpandedCheckpointV1 {
    let (_, _, mut saved) = checkpoint_fixture_v1();
    saved.card_db_hash = card_db_hash.into();
    saved.adam_step = 3;
    let anchor = saved
        .parameters
        .iter()
        .position(|tensor| tensor.name == "scorer.2.bias")
        .unwrap();
    let learned = (source_cards + 1) * CARD_EMBEDDING_DIM_V1;
    for bits in &mut saved.parameters[0].values[CARD_EMBEDDING_DIM_V1..learned] {
        *bits = (f32::from_bits(*bits) + 0.0625).to_bits();
    }
    let learnable = |ordinal: usize, index: usize| {
        if ordinal == 0 {
            (CARD_EMBEDDING_DIM_V1..learned).contains(&index)
        } else {
            ordinal != anchor || index != 0
        }
    };
    for (ordinal, tensor) in saved.first_moments.iter_mut().enumerate() {
        for (index, bits) in tensor.values.iter_mut().enumerate() {
            if learnable(ordinal, index) {
                *bits = (((index % 7) as f32 - 3.0) / 1024.0).to_bits();
            }
        }
    }
    for (ordinal, tensor) in saved.second_moments.iter_mut().enumerate() {
        for (index, bits) in tensor.values.iter_mut().enumerate() {
            if learnable(ordinal, index) {
                *bits = (((index % 5) as f32 + 1.0) / 65536.0).to_bits();
            }
        }
    }
    saved.state_sha256 = state_sha256_v1(&saved).unwrap();
    saved
}

fn bytes_of(saved: &ExpandedCheckpointV1) -> Vec<u8> {
    serde_json::to_vec(saved).unwrap()
}

fn request_for(
    checkpoint: &[u8],
    source: &[&[u8]],
    destination: &[&[u8]],
    source_card_db_hash: &str,
) -> CardWaveTransferRequestV1 {
    CardWaveTransferRequestV1 {
        source_checkpoint_sha256: sha(checkpoint),
        source_card_db_hash: source_card_db_hash.into(),
        source_registry_sha256: registry_identity_v1(source),
        destination_card_db_hash: current_hash(),
        destination_registry_sha256: registry_identity_v1(destination),
        feature_contract_digest: FEATURE_CONTRACT_DIGEST_V3.into(),
        feature_encoding_digest: FEATURE_ENCODING_DIGEST_V3.into(),
        initialization_seed: SEED,
    }
}

/// The transfer into a destination build whose compiled card names are the
/// destination registry's own and whose card database is this build's.
fn transfer(
    checkpoint: &[u8],
    source: &[&[u8]],
    destination: &[&[u8]],
    request: &CardWaveTransferRequestV1,
) -> Result<CardWaveTransferV1, String> {
    let cards = concatenated_cards_v1(destination)?;
    let names = cards
        .iter()
        .map(card_name_v1)
        .collect::<Result<Vec<_>, _>>()?;
    let binding = DestinationBindingV1 {
        card_db_hash: current_hash(),
        names,
    };
    transfer_to_destination_v1(checkpoint, source, destination, request, &binding)
}

/// The source checkpoint of the standard wave transferred into `destination`.
fn transfer_into(destination: &[Vec<u8>]) -> Result<CardWaveTransferV1, String> {
    let (source, _) = wave_registries();
    let checkpoint = bytes_of(&learned_checkpoint(OLD_CARD_DB_HASH, 3));
    let source = slices(&source);
    let destination = slices(destination);
    let request = request_for(&checkpoint, &source, &destination, OLD_CARD_DB_HASH);
    transfer(&checkpoint, &source, &destination, &request)
}

fn parsed(candidate: &CardWaveTransferV1) -> ExpandedCheckpointV1 {
    checkpoint_container_v1::parse_checkpoint_v1(candidate.checkpoint_bytes_v1()).unwrap()
}

fn sections(saved: &ExpandedCheckpointV1) -> [&[ParameterBitsV1]; 3] {
    [
        &saved.parameters,
        &saved.first_moments,
        &saved.second_moments,
    ]
}

/// The standard wave with its first source segment replaced.
fn edited_wave(first: Vec<u8>) -> Vec<Vec<u8>> {
    let (source, destination) = wave_registries();
    vec![first, source[1].clone(), destination[2].clone()]
}

fn appended_span(card_id: usize) -> std::ops::Range<usize> {
    (card_id + 1) * CARD_EMBEDDING_DIM_V1..(card_id + 2) * CARD_EMBEDDING_DIM_V1
}

#[test]
fn card_wave_transfer_keeps_every_existing_bit_and_field() {
    assert_ne!(OLD_CARD_DB_HASH, current_hash());
    let before = learned_checkpoint(OLD_CARD_DB_HASH, 3);
    let (_, destination) = wave_registries();
    let candidate = transfer_into(&destination).unwrap();
    let after = parsed(&candidate);
    let changed = appended_span(3).start..appended_span(4).end;
    for (old, new) in sections(&before).into_iter().zip(sections(&after)) {
        assert_eq!(old.len(), new.len());
        for (ordinal, (a, b)) in old.iter().zip(new).enumerate() {
            assert_eq!((&a.name, &a.shape), (&b.name, &b.shape));
            for (index, (x, y)) in a.values.iter().zip(&b.values).enumerate() {
                if ordinal == 0 && changed.contains(&index) {
                    continue;
                }
                assert_eq!(x, y, "{}[{index}] changed", a.name);
            }
        }
    }
    // Every other field, including the warm-start provenance, Adam step,
    // gauge anchor, loss and scalar settings, is unchanged.
    let mut old = serde_json::to_value(&before).unwrap();
    let mut new = serde_json::to_value(&after).unwrap();
    for key in [
        "parameters",
        "first_moments",
        "second_moments",
        "card_db_hash",
        "state_sha256",
    ] {
        old.as_object_mut().unwrap().remove(key);
        new.as_object_mut().unwrap().remove(key);
    }
    assert_eq!(old, new);
    assert_eq!(after.card_db_hash, current_hash());
    assert_eq!(after.state_sha256, state_sha256_v1(&after).unwrap());
    let record = candidate.record_v1();
    assert_eq!(record.request.source_card_db_hash, OLD_CARD_DB_HASH);
    assert_eq!(record.request.destination_card_db_hash, current_hash());
    assert_eq!(record.request.initialization_seed, SEED);
    assert_eq!(record.source_state_sha256, before.state_sha256);
    assert_eq!(record.destination_state_sha256, after.state_sha256);
    assert_eq!(
        record.destination_checkpoint_sha256,
        sha(candidate.checkpoint_bytes_v1())
    );
    assert_eq!(record.source_card_count, 3);
    assert_eq!(record.destination_card_count, 5);
    let appended: Vec<(u16, u32, &str)> = record
        .appended_cards
        .iter()
        .map(|card| (card.card_id, card.embedding_row, card.name.as_str()))
        .collect();
    assert_eq!(appended, [(3, 4, "Delta"), (4, 5, "Epsilon")]);
    assert_eq!(record.initialized_scalar_count_per_state_section, 32);
}

#[test]
fn card_wave_appended_rows_match_the_deterministic_initializer_with_zero_moments() {
    let (_, destination) = wave_registries();
    let after = parsed(&transfer_into(&destination).unwrap());
    for (card_id, name) in [(3, "Delta"), (4, "Epsilon")] {
        let span = appended_span(card_id);
        let expected: Vec<u32> = initial_card_row(name, SEED)
            .iter()
            .map(|value| value.to_bits())
            .collect();
        assert_eq!(after.parameters[0].values[span.clone()], expected[..]);
        let other_seed = initial_card_row(name, SEED + 1);
        assert_ne!(other_seed, initial_card_row(name, SEED));
        assert!(after.first_moments[0].values[span.clone()]
            .iter()
            .chain(&after.second_moments[0].values[span])
            .all(|bits| *bits == 0));
    }
    // Still-unassigned rows past the wave keep their bits.
    let before = learned_checkpoint(OLD_CARD_DB_HASH, 3);
    let beyond = appended_span(5).start..appended_span(9).end;
    assert_eq!(
        before.parameters[0].values[beyond.clone()],
        after.parameters[0].values[beyond]
    );
}

#[test]
fn card_wave_transferred_checkpoint_loads_where_the_source_refused() {
    let (mut policy, model, _) = checkpoint_fixture_v1();
    let source = learned_checkpoint(OLD_CARD_DB_HASH, 3);
    let refused = match restore_checkpoint_state_v1(&source, &mut policy, model.clone()) {
        Ok(_) => panic!("a checkpoint of another card database must not load"),
        Err(error) => error,
    };
    assert!(refused.contains("card database"), "{refused}");

    let (_, destination) = wave_registries();
    let candidate = transfer_into(&destination).unwrap();
    // Through the trainer's own reader and ordinary restore.
    let saved = parsed(&candidate);
    let state = restore_checkpoint_state_v1(&saved, &mut policy, model).unwrap();
    let snapshot = state.snapshot_v1().unwrap();
    assert_eq!(snapshot.adam_step, 3);
    let restored = [
        &snapshot.parameters,
        &snapshot.first_moments,
        &snapshot.second_moments,
    ];
    for (loaded, written) in restored.into_iter().zip(sections(&saved)) {
        for (a, b) in loaded.iter().zip(written) {
            let bits: Vec<u32> = a.values.iter().map(|value| value.to_bits()).collect();
            assert_eq!(bits, b.values, "{} differs after restore", a.name);
        }
    }
}

#[test]
fn card_wave_refuses_reordered_removed_renamed_and_changed_rows() {
    let reordered = edited_wave(named(&["Beta", "Alpha"]));
    let error = transfer_into(&reordered).unwrap_err();
    assert!(error.contains("reordered"), "{error}");

    let removed = [named(&["Alpha", "Beta"])];
    let error = transfer_into(&removed).unwrap_err();
    assert!(error.contains("is removed"), "{error}");

    let removed_inside = edited_wave(named(&["Alpha"]));
    let error = transfer_into(&removed_inside).unwrap_err();
    assert!(error.contains("removed or renamed"), "{error}");

    let renamed = edited_wave(named(&["Alpha", "Beta Prime"]));
    let error = transfer_into(&renamed).unwrap_err();
    assert!(error.contains("removed or renamed"), "{error}");

    let changed = edited_wave(segment(vec![card("Alpha", 1), card("Beta", 2)]));
    let error = transfer_into(&changed).unwrap_err();
    assert!(error.contains("\"mana_value\""), "{error}");

    let mut beta = card("Beta", 1);
    beta["decks"] = serde_json::json!(["Synthetic", "Other"]);
    let changed = edited_wave(segment(vec![card("Alpha", 1), beta]));
    let error = transfer_into(&changed).unwrap_err();
    assert!(error.contains("\"decks\""), "{error}");
}

#[test]
fn card_wave_admits_a_metadata_only_change_outside_the_card_database_hash() {
    let mut beta = card("Beta", 1);
    beta["complexity"] = serde_json::json!("high");
    let edited = edited_wave(segment(vec![card("Alpha", 1), beta]));
    let candidate = transfer_into(&edited).unwrap();
    assert_eq!(candidate.record_v1().appended_cards.len(), 2);
}

#[test]
fn card_wave_refuses_the_same_registry_and_a_second_transfer() {
    let (source, destination) = wave_registries();
    let source = slices(&source);
    let destination = slices(&destination);

    // Same card database: nothing to transfer.
    let checkpoint = bytes_of(&learned_checkpoint(&current_hash(), 3));
    let request = request_for(&checkpoint, &source, &source, &current_hash());
    let error = transfer(&checkpoint, &source, &source, &request).unwrap_err();
    assert!(error.contains("identical"), "{error}");

    // A transferred checkpoint is never transferred again into its build.
    let checkpoint = bytes_of(&learned_checkpoint(OLD_CARD_DB_HASH, 3));
    let request = request_for(&checkpoint, &source, &destination, OLD_CARD_DB_HASH);
    let first = transfer(&checkpoint, &source, &destination, &request).unwrap();
    let again = first.checkpoint_bytes_v1();
    let request = request_for(again, &destination, &destination, &current_hash());
    let error = transfer(again, &destination, &destination, &request).unwrap_err();
    assert!(error.contains("identical"), "{error}");
    // Transferring the original again is deterministic.
    let original = first.record_v1().request.clone();
    let repeat = transfer(&checkpoint, &source, &destination, &original);
    assert_eq!(repeat.unwrap().checkpoint_bytes_v1(), again);

    // A changed hash with no appended card is not a card wave.
    let request = request_for(&checkpoint, &source, &source, OLD_CARD_DB_HASH);
    let error = transfer(&checkpoint, &source, &source, &request).unwrap_err();
    assert!(error.contains("no card is appended"), "{error}");
}

#[test]
fn card_wave_refuses_feature_shape_pin_and_learned_row_mismatches() {
    let (source, destination) = wave_registries();
    let source = slices(&source);
    let destination = slices(&destination);
    let checkpoint = bytes_of(&learned_checkpoint(OLD_CARD_DB_HASH, 3));
    let request = request_for(&checkpoint, &source, &destination, OLD_CARD_DB_HASH);

    let mut changed = request.clone();
    changed.feature_contract_digest = "0".repeat(64);
    let error = transfer(&checkpoint, &source, &destination, &changed).unwrap_err();
    assert!(error.contains("feature contract"), "{error}");

    let mut changed = request.clone();
    changed.source_card_db_hash = "fedcba9876543210".into();
    let error = transfer(&checkpoint, &source, &destination, &changed).unwrap_err();
    assert!(error.contains("declared source card database"), "{error}");

    let mut changed = request;
    changed.source_checkpoint_sha256 = "0".repeat(64);
    let error = transfer(&checkpoint, &source, &destination, &changed).unwrap_err();
    assert!(error.contains("SHA256 differs"), "{error}");

    // A model of another shape: one tensor fewer in every section.
    let mut reshaped = learned_checkpoint(OLD_CARD_DB_HASH, 3);
    reshaped.parameters.pop();
    reshaped.first_moments.pop();
    reshaped.second_moments.pop();
    let bytes = bytes_of(&reshaped);
    let request = request_for(&bytes, &source, &destination, OLD_CARD_DB_HASH);
    let error = transfer(&bytes, &source, &destination, &request).unwrap_err();
    assert!(error.contains("model shape"), "{error}");

    // Learning in a row the declared registry calls unassigned.
    let mut learned = learned_checkpoint(OLD_CARD_DB_HASH, 3);
    learned.first_moments[0].values[appended_span(4).start] = 0.5_f32.to_bits();
    learned.state_sha256 = state_sha256_v1(&learned).unwrap();
    let bytes = bytes_of(&learned);
    let request = request_for(&bytes, &source, &destination, OLD_CARD_DB_HASH);
    let error = transfer(&bytes, &source, &destination, &request).unwrap_err();
    assert!(error.contains("learned optimizer state"), "{error}");
}

#[test]
fn card_wave_compiled_destination_refuses_a_foreign_registry() {
    let (source, destination) = wave_registries();
    let source = slices(&source);
    let destination = slices(&destination);
    let checkpoint = bytes_of(&learned_checkpoint(OLD_CARD_DB_HASH, 3));
    let request = request_for(&checkpoint, &source, &destination, OLD_CARD_DB_HASH);
    let result = transfer_card_wave_checkpoint_v1(&checkpoint, &source, &destination, &request);
    let error = result.unwrap_err();
    assert!(error.contains("compiled card registry"), "{error}");
}

#[test]
fn card_wave_record_replay_accepts_the_transfer_and_rejects_tampering() {
    let (source, destination) = wave_registries();
    let source = slices(&source);
    let destination = slices(&destination);
    let checkpoint = bytes_of(&learned_checkpoint(OLD_CARD_DB_HASH, 3));
    let request = request_for(&checkpoint, &source, &destination, OLD_CARD_DB_HASH);
    let candidate = transfer(&checkpoint, &source, &destination, &request).unwrap();
    let record = candidate.record_bytes_v1().unwrap();
    let written = candidate.checkpoint_bytes_v1();
    let reproduce = |pinned_request: &CardWaveTransferRequestV1| {
        transfer(&checkpoint, &source, &destination, pinned_request)
    };
    let verified = verify_reproduced_v1(&record, written, reproduce).unwrap();
    assert_eq!(verified.record_v1(), candidate.record_v1());

    let mut edited: CardWaveTransferRecordV1 = serde_json::from_slice(&record).unwrap();
    edited.appended_cards[0].name = "Zeta".into();
    let edited = serde_json::to_vec(&edited).unwrap();
    assert!(verify_reproduced_v1(&edited, written, reproduce).is_err());

    let mut tampered = parsed(&candidate);
    tampered.first_moments[1].values[0] ^= 1;
    let tampered = bytes_of(&tampered);
    assert!(verify_reproduced_v1(&record, &tampered, reproduce).is_err());
}
