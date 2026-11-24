//! Card-wave transfer on small synthetic registries and the trainer's own
//! checkpoint fixture (the net-8 layout and the V8 inclusion layout), plus
//! one transfer into this build's compiled registry through the public entry
//! points and the command. Engineering verification only, never a card wave.

use super::*;
use crate::expanded_deck_training_v1::archive_distillation_v1::{
    ArchiveDistillationArmV1, ArchiveDistillationProvenanceV1,
};
use crate::expanded_deck_training_v1::registry_transfer_source::ExpandedRegistryContinuationV1;
use crate::expanded_deck_training_v1::tests::checkpoint_fixture_v1;
use crate::native_flat_tensorizer_v7::{FEATURE_CONTRACT_DIGEST_V7, FEATURE_ENCODING_DIGEST_V7};
use crate::native_flat_tensorizer_v8::{FEATURE_CONTRACT_DIGEST_V8, FEATURE_ENCODING_DIGEST_V8};
use crate::phase1_registry_transfer_v1::{registry_cards, RegistryTransferScalarsV1};
use crate::sideboard_play_policy_v1::{
    FreshFeatureIdentityV1, FreshPlayPolicyIdentityV1, TransferredFreshPlayPolicyIdentityV1,
    FRESH_FEATURE_IDENTITY_V3, FRESH_FEATURE_IDENTITY_V4, FRESH_PLAY_INITIALIZATION_SCHEMA_V1,
    TRANSFERRED_FRESH_PLAY_SCHEMA_V1,
};

const OLD_CARD_DB_HASH: &str = "0123456789abcdef";
const SEED: u64 = 20_261_123;
/// A row far past every synthetic registry, still unassigned.
const FAR_ROW: usize = 100;

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

fn segment_of_version(version: u64, cards: Vec<Value>) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({ "version": version, "cards": cards })).unwrap()
}

fn segment(cards: Vec<Value>) -> Vec<u8> {
    segment_of_version(REGISTRY_SCHEMA_VERSION, cards)
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

/// A two-file source registry (ids 0..3) and its wave: a third file
/// appending ids 3 and 4, as `eternal-fixtures` appends after Standard.
fn wave_registries() -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let source = vec![named(&["Alpha", "Beta"]), named(&["Gamma"])];
    let mut destination = source.clone();
    destination.push(named(&["Delta", "Epsilon"]));
    (source, destination)
}

fn appended_span(card_id: usize) -> std::ops::Range<usize> {
    (card_id + 1) * CARD_EMBEDDING_DIM_V1..(card_id + 2) * CARD_EMBEDDING_DIM_V1
}

/// The imported origin `origin` as a build with card database
/// `card_db_hash` and first registry file `first_segment` stamps it: that
/// hash, that file's SHA-256 and its card count.
fn imported_origin(
    origin: &PlayPolicyOriginV1,
    card_db_hash: &str,
    first_segment: &[u8],
) -> PlayPolicyOriginV1 {
    let PlayPolicyOriginV1::Imported(mut identity) = origin.clone() else {
        panic!("the trainer's checkpoint fixture has an imported origin");
    };
    identity.source_card_db_hash = card_db_hash.into();
    identity.destination_card_db_hash = card_db_hash.into();
    identity.destination_registry_sha256 = sha(first_segment);
    identity.destination_card_count = registry_cards(first_segment).unwrap().len();
    identity.into()
}

/// A fresh initialization identity of the feature generation `features`
/// that passes the origin's own metadata validation, bound to the given card
/// database, first file and count.
fn fresh_identity(
    card_db_hash: &str,
    first_segment: &[u8],
    card_count: usize,
    features: FreshFeatureIdentityV1,
) -> FreshPlayPolicyIdentityV1 {
    FreshPlayPolicyIdentityV1 {
        schema: FRESH_PLAY_INITIALIZATION_SCHEMA_V1.into(),
        initialization_manifest_sha256: "a".repeat(64),
        lineage_id: "synthetic-card-wave-test".into(),
        initializer: "trainer-seeded-v1".into(),
        base_seed: 0,
        model_init_seed: 6_443_515_232_517_447_393,
        seed_derivation: "kernel-python-rl-trainer-sha256-v2".into(),
        producer_git_commit: "1".repeat(40),
        initial_weights_sha256: "b".repeat(64),
        initial_model_parameter_sha256: "c".repeat(64),
        parameter_layout_sha256: "d".repeat(64),
        destination_registry_sha256: sha(first_segment),
        destination_card_db_hash: card_db_hash.into(),
        destination_card_count: card_count,
        feature_contract_digest: features.feature_contract_digest.into(),
        feature_encoding_digest: features.feature_encoding_digest.into(),
        features_source_sha256: features.features_source_sha256.into(),
        feature_descriptor_sha256: features.feature_descriptor_sha256.into(),
        sampler_identity: crate::fast_sampler::WIDE_CATEGORICAL_SAMPLER_VERSION_V1.into(),
    }
}

/// Learned-looking optimizer state on every tensor and on the first
/// `trained_cards` card rows (their parameters perturbed too), nothing on
/// later rows, the padding row or the gauge.
fn add_learning(saved: &mut ExpandedCheckpointV1, trained_cards: usize) {
    let anchor = saved
        .parameters
        .iter()
        .position(|tensor| tensor.name == "scorer.2.bias")
        .unwrap();
    let learned = (trained_cards + 1) * CARD_EMBEDDING_DIM_V1;
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
}

/// Re-records the checkpoint's state digest by the trainer's restore, for a
/// model of the V8 inclusion layout or net-8's.
fn reseal_as(saved: &mut ExpandedCheckpointV1, flat_v8: bool) {
    saved.state_sha256 = restored_state_sha256_v1(saved, flat_v8).unwrap();
}

fn reseal(saved: &mut ExpandedCheckpointV1) {
    let flat_v8 = saved.feature_contract_digest == FEATURE_CONTRACT_DIGEST_V8;
    reseal_as(saved, flat_v8);
}

/// `saved` as a checkpoint written before the wave in a build whose registry
/// is `source`: that build's card database and imported origin, an Adam step
/// and learning on the first `trained_cards` cards.
fn pre_wave(
    mut saved: ExpandedCheckpointV1,
    source: &[Vec<u8>],
    trained_cards: usize,
) -> ExpandedCheckpointV1 {
    saved.card_db_hash = OLD_CARD_DB_HASH.into();
    saved.adam_step = 3;
    saved.source_import = imported_origin(&saved.source_import, OLD_CARD_DB_HASH, &source[0]);
    add_learning(&mut saved, trained_cards);
    reseal(&mut saved);
    saved
}

/// The fresh origin of the feature generation `features` as the source
/// build of the standard wave stamps it: its card database, first file and
/// whole card count.
fn fresh_origin(features: FreshFeatureIdentityV1) -> PlayPolicyOriginV1 {
    let source = wave_registries().0;
    PlayPolicyOriginV1::FreshInitialization(fresh_identity(
        OLD_CARD_DB_HASH,
        &source[0],
        3,
        features,
    ))
}

/// `saved` (of a V4-generation feature contract: V4, FlatV7 or FlatV8) as
/// the fresh lineage writes it before the standard wave: the fresh checkpoint
/// schema and a fresh origin of the V4 generation, which FlatV7 and FlatV8
/// keep from the V4 policy they convert.
fn fresh_v4_pre_wave(mut saved: ExpandedCheckpointV1) -> ExpandedCheckpointV1 {
    saved.schema = FRESH_CHECKPOINT_SCHEMA.into();
    let mut saved = pre_wave(saved, &wave_registries().0, 3);
    saved.source_import = fresh_origin(FRESH_FEATURE_IDENTITY_V4);
    saved
}

/// The trainer's fixture (V3, terminal loss, net-8) before the standard wave.
fn learned_checkpoint() -> ExpandedCheckpointV1 {
    let (_, _, saved) = checkpoint_fixture_v1();
    pre_wave(saved, &wave_registries().0, 3)
}

fn bits_of(section: &[NativeNamedParameterV1]) -> Vec<ParameterBitsV1> {
    section.iter().map(ParameterBitsV1::from_native).collect()
}

/// A FlatV8 checkpoint: the V8 inclusion net's 37-tensor train state.
fn flat_v8_checkpoint() -> ExpandedCheckpointV1 {
    let (_, _, mut saved) = checkpoint_fixture_v1();
    let model = NativePolicyValueNetV1::v8_inclusion_template_v1().unwrap();
    let snapshot = NativePolicyValueTrainStateV1::new_v1(model)
        .unwrap()
        .snapshot_v1()
        .unwrap();
    saved.parameters = bits_of(&snapshot.parameters);
    saved.first_moments = bits_of(&snapshot.first_moments);
    saved.second_moments = bits_of(&snapshot.second_moments);
    saved.scorer_bias_anchor_bits = snapshot.scorer_bias_anchor_bits;
    saved.feature_contract_digest = FEATURE_CONTRACT_DIGEST_V8.into();
    saved.feature_encoding_digest = FEATURE_ENCODING_DIGEST_V8.into();
    fresh_v4_pre_wave(saved)
}

/// Sets the state-digest input columns of the state encoder's weight and
/// both its moments to +0.0, as a no-state-digest conversion stores them.
fn zero_state_digest_columns(saved: &mut ExpandedCheckpointV1) {
    let width = no_state_digest_v1::STATE_ENCODER_INPUT_V1;
    let columns = no_state_digest_v1::STATE_DIGEST_COLUMNS_V1;
    for section in [
        &mut saved.parameters,
        &mut saved.first_moments,
        &mut saved.second_moments,
    ] {
        let weight = section
            .iter_mut()
            .find(|tensor| tensor.name == "state_encoder.0.weight")
            .unwrap();
        for row in weight.values.chunks_mut(width) {
            row[columns.clone()].fill(0);
        }
    }
}

/// A no-state-digest checkpoint of the V3 generation.
fn no_state_digest_checkpoint() -> ExpandedCheckpointV1 {
    let mut saved = learned_checkpoint();
    saved.feature_contract_digest =
        no_state_digest_v1::converted_contract_digest_v1(FEATURE_CONTRACT_DIGEST_V3);
    saved.input_contract = Some(no_state_digest_v1::InputContractRecordV1 {
        contract: no_state_digest_v1::CONTRACT_V1.into(),
        state_digest_columns: [
            no_state_digest_v1::STATE_DIGEST_COLUMNS_V1.start,
            no_state_digest_v1::STATE_DIGEST_COLUMNS_V1.end,
        ],
        action_digest_columns: None,
        conversion: PinnedFileV1 {
            path: PathBuf::from("/synthetic/conversion.json"),
            sha256: "9".repeat(64),
        },
    });
    zero_state_digest_columns(&mut saved);
    reseal(&mut saved);
    saved
}

fn bytes_of(saved: &ExpandedCheckpointV1) -> Vec<u8> {
    serde_json::to_vec(saved).unwrap()
}

/// The request a caller pins for `saved` (whose bytes are `checkpoint`).
fn request_for(
    saved: &ExpandedCheckpointV1,
    checkpoint: &[u8],
    source: &[&[u8]],
    destination: &[&[u8]],
) -> CardWaveTransferRequestV1 {
    CardWaveTransferRequestV1 {
        source_checkpoint_sha256: sha(checkpoint),
        source_card_db_hash: saved.card_db_hash.clone(),
        source_registry_sha256: registry_identity_v1(source),
        destination_card_db_hash: current_hash(),
        destination_registry_sha256: registry_identity_v1(destination),
        feature_contract_digest: saved.feature_contract_digest.clone(),
        feature_encoding_digest: saved.feature_encoding_digest.clone(),
        initialization_seed: SEED,
    }
}

/// The transfer into a build whose compiled registry files are `compiled`
/// (their card names its compiled names) and whose card database is this
/// build's.
fn transfer_compiled(
    checkpoint: &[u8],
    source: &[&[u8]],
    destination: &[&[u8]],
    request: &CardWaveTransferRequestV1,
    compiled: &[&[u8]],
) -> Result<CardWaveTransferV1, String> {
    let cards = registry_v1(compiled)?.cards;
    let names = cards
        .iter()
        .map(card_name_v1)
        .collect::<Result<Vec<_>, _>>()?;
    let binding = DestinationBindingV1 {
        segments: compiled.to_vec(),
        names,
    };
    transfer_to_destination_v1(checkpoint, source, destination, request, &binding)
}

/// The transfer into a build whose compiled registry is `destination`.
fn transfer(
    checkpoint: &[u8],
    source: &[&[u8]],
    destination: &[&[u8]],
    request: &CardWaveTransferRequestV1,
) -> Result<CardWaveTransferV1, String> {
    transfer_compiled(checkpoint, source, destination, request, destination)
}

/// `saved` across the given registries, pinned as a caller pins it.
fn run(
    saved: &ExpandedCheckpointV1,
    source: &[Vec<u8>],
    destination: &[Vec<u8>],
) -> Result<CardWaveTransferV1, String> {
    let checkpoint = bytes_of(saved);
    let source = slices(source);
    let destination = slices(destination);
    let request = request_for(saved, &checkpoint, &source, &destination);
    transfer(&checkpoint, &source, &destination, &request)
}

/// `saved` across the standard wave.
fn transfer_wave(saved: &ExpandedCheckpointV1) -> Result<CardWaveTransferV1, String> {
    let (source, destination) = wave_registries();
    run(saved, &source, &destination)
}

fn refusal(saved: &ExpandedCheckpointV1) -> String {
    match transfer_wave(saved) {
        Ok(_) => panic!("the transfer must refuse this checkpoint"),
        Err(error) => error,
    }
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

/// The transfer of `before` across the standard wave (`appended` card ids)
/// changed nothing but the appended rows of the card embedding and its
/// moments, `card_db_hash` and `state_sha256`; the appended rows hold the
/// deterministic initializer and zero moments.
fn assert_carried(
    before: &ExpandedCheckpointV1,
    candidate: &CardWaveTransferV1,
    appended: &[&str],
) {
    let after = parsed(candidate);
    let first = appended_span(3).start;
    let changed = first..first + appended.len() * CARD_EMBEDDING_DIM_V1;
    for (old, new) in sections(before).into_iter().zip(sections(&after)) {
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
    for (offset, name) in appended.iter().enumerate() {
        let span = appended_span(3 + offset);
        let expected: Vec<u32> = initial_card_row(name, SEED)
            .iter()
            .map(|value| value.to_bits())
            .collect();
        assert_eq!(after.parameters[0].values[span.clone()], expected[..]);
        assert!(after.first_moments[0].values[span.clone()]
            .iter()
            .chain(&after.second_moments[0].values[span])
            .all(|bits| *bits == 0));
    }
    let mut old = serde_json::to_value(before).unwrap();
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
    let flat_v8 = after.feature_contract_digest == FEATURE_CONTRACT_DIGEST_V8;
    assert_eq!(
        after.state_sha256,
        restored_state_sha256_v1(&after, flat_v8).unwrap()
    );
}

/// The checkpoint's v1 JSON bytes with `member` inserted first.
fn with_member(saved: &ExpandedCheckpointV1, member: &str) -> Vec<u8> {
    let bytes = bytes_of(saved);
    assert_eq!(bytes[0], b'{');
    let mut edited = b"{".to_vec();
    edited.extend_from_slice(member.as_bytes());
    edited.push(b',');
    edited.extend_from_slice(&bytes[1..]);
    edited
}

/// `checkpoint` (bytes of `saved`) across the standard wave.
fn transfer_bytes(
    saved: &ExpandedCheckpointV1,
    checkpoint: &[u8],
) -> Result<CardWaveTransferV1, String> {
    let (source, destination) = wave_registries();
    let source = slices(&source);
    let destination = slices(&destination);
    let request = request_for(saved, checkpoint, &source, &destination);
    transfer(checkpoint, &source, &destination, &request)
}

#[test]
fn card_wave_transfer_keeps_every_existing_bit_and_field() {
    assert_ne!(OLD_CARD_DB_HASH, current_hash());
    let before = learned_checkpoint();
    let candidate = transfer_wave(&before).unwrap();
    assert_carried(&before, &candidate, &["Delta", "Epsilon"]);
    let after = parsed(&candidate);
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
    let before = learned_checkpoint();
    let after = parsed(&transfer_wave(&before).unwrap());
    for (card_id, name) in [(3, "Delta"), (4, "Epsilon")] {
        let span = appended_span(card_id);
        let expected: Vec<u32> = initial_card_row(name, SEED)
            .iter()
            .map(|value| value.to_bits())
            .collect();
        assert_eq!(after.parameters[0].values[span.clone()], expected[..]);
        assert_ne!(
            initial_card_row(name, SEED + 1),
            initial_card_row(name, SEED)
        );
        assert!(after.first_moments[0].values[span.clone()]
            .iter()
            .chain(&after.second_moments[0].values[span])
            .all(|bits| *bits == 0));
    }
    // Still-unassigned rows past the wave keep their bits.
    let beyond = appended_span(5).start..appended_span(9).end;
    assert_eq!(
        before.parameters[0].values[beyond.clone()],
        after.parameters[0].values[beyond]
    );
}

/// The trainer's ordinary restore refuses the pre-wave checkpoint at its
/// card database. The transferred one passes that gate and every check
/// before the warm-start provenance, where it stops: its origin still names
/// the source build, which this transfer deliberately does not rewrite
/// (docs/CARD-WAVE-TRANSFER.md, Limits). With the origin re-bound to the
/// destination policy, the step a design decision must still add, every
/// remaining restore check passes and the restored state is the written one.
#[test]
fn card_wave_transferred_checkpoint_passes_the_card_database_gate_and_stops_only_at_its_origin() {
    let (mut policy, model, _) = checkpoint_fixture_v1();
    let restore = |saved: &ExpandedCheckpointV1, policy: &mut FrozenPlayPolicyV1| {
        restore_checkpoint_state_v1(saved, policy, model.clone())
    };
    let source = learned_checkpoint();
    let refused = match restore(&source, &mut policy) {
        Ok(_) => panic!("a checkpoint of another card database must not load"),
        Err(error) => error,
    };
    assert!(refused.contains("card database"), "{refused}");
    // The origin alone does not make the source load: its card database does
    // not.
    let mut stale = source.clone();
    stale.source_import = policy.identity_v1().clone();
    let refused = match restore(&stale, &mut policy) {
        Ok(_) => panic!("a checkpoint of another card database must not load"),
        Err(error) => error,
    };
    assert!(refused.contains("card database"), "{refused}");

    let saved = parsed(&transfer_wave(&source).unwrap());
    let refused = match restore(&saved, &mut policy) {
        Ok(_) => panic!("the transfer does not re-bind the origin"),
        Err(error) => error,
    };
    assert!(
        refused.contains("warm-start provenance differs"),
        "{refused}"
    );

    let mut rebound = saved.clone();
    rebound.source_import = policy.identity_v1().clone();
    let state = restore(&rebound, &mut policy).unwrap();
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
    let saved = learned_checkpoint();
    let (source, destination) = wave_registries();
    let into = |first: Vec<u8>| {
        let edited = vec![first, source[1].clone(), destination[2].clone()];
        run(&saved, &source, &edited).unwrap_err()
    };
    let error = into(named(&["Beta", "Alpha"]));
    assert!(error.contains("reordered"), "{error}");
    let error = run(&saved, &source, &[named(&["Alpha", "Beta"])]).unwrap_err();
    assert!(error.contains("is removed"), "{error}");
    let error = into(named(&["Alpha"]));
    assert!(error.contains("removed or renamed"), "{error}");
    let error = into(named(&["Alpha", "Beta Prime"]));
    assert!(error.contains("removed or renamed"), "{error}");
    let error = into(segment(vec![card("Alpha", 1), card("Beta", 2)]));
    assert!(error.contains("\"mana_value\""), "{error}");
    let mut beta = card("Beta", 1);
    beta["decks"] = serde_json::json!(["Synthetic", "Other"]);
    let error = into(segment(vec![card("Alpha", 1), beta]));
    assert!(error.contains("\"decks\""), "{error}");
    // A field `CardJson` does not declare is still compared.
    let mut beta = card("Beta", 1);
    beta["oracle_id"] = serde_json::json!("x");
    let error = into(segment(vec![card("Alpha", 1), beta]));
    assert!(error.contains("\"oracle_id\""), "{error}");
}

#[test]
fn card_wave_admits_a_metadata_only_change_outside_the_card_database_hash() {
    let saved = learned_checkpoint();
    let (source, destination) = wave_registries();
    let mut beta = card("Beta", 1);
    beta["complexity"] = serde_json::json!("high");
    let edited = vec![
        segment(vec![card("Alpha", 1), beta]),
        source[1].clone(),
        destination[2].clone(),
    ];
    // The origin binds the source build's first file, so the edited file is
    // the destination's alone.
    let candidate = run(&saved, &source, &edited).unwrap();
    assert_eq!(candidate.record_v1().appended_cards.len(), 2);
}

#[test]
fn card_wave_refuses_the_same_registry_and_a_second_transfer() {
    let (source_files, destination_files) = wave_registries();
    let source = slices(&source_files);
    let destination = slices(&destination_files);

    // Same card database: nothing to transfer.
    let mut current = learned_checkpoint();
    current.card_db_hash = current_hash();
    let checkpoint = bytes_of(&current);
    let request = request_for(&current, &checkpoint, &source, &source);
    let error = transfer(&checkpoint, &source, &source, &request).unwrap_err();
    assert!(error.contains("identical"), "{error}");

    // A transferred checkpoint is never transferred again into its build.
    let saved = learned_checkpoint();
    let checkpoint = bytes_of(&saved);
    let request = request_for(&saved, &checkpoint, &source, &destination);
    let first = transfer(&checkpoint, &source, &destination, &request).unwrap();
    let again = first.checkpoint_bytes_v1();
    let request = request_for(&parsed(&first), again, &destination, &destination);
    let error = transfer(again, &destination, &destination, &request).unwrap_err();
    assert!(error.contains("identical"), "{error}");
    // Transferring the original again is deterministic.
    let repeat = transfer(
        &checkpoint,
        &source,
        &destination,
        &first.record_v1().request,
    );
    assert_eq!(repeat.unwrap().checkpoint_bytes_v1(), again);

    // A changed hash with no appended card is not a card wave.
    let request = request_for(&saved, &checkpoint, &source, &source);
    let error = transfer(&checkpoint, &source, &source, &request).unwrap_err();
    assert!(error.contains("no card is appended"), "{error}");
}

/// An edit of a transfer request, and the refusal it must meet.
type RequestEditV1 = (fn(&mut CardWaveTransferRequestV1), &'static str);

#[test]
fn card_wave_refuses_every_request_pin_mismatch() {
    let saved = learned_checkpoint();
    let (source, destination) = wave_registries();
    let source = slices(&source);
    let destination = slices(&destination);
    let checkpoint = bytes_of(&saved);
    let request = request_for(&saved, &checkpoint, &source, &destination);
    transfer(&checkpoint, &source, &destination, &request).unwrap();
    let edits: [RequestEditV1; 7] = [
        (
            |r| r.source_checkpoint_sha256 = "0".repeat(64),
            "source checkpoint SHA256 differs",
        ),
        (
            |r| r.source_registry_sha256 = "0".repeat(64),
            "source registry identity differs",
        ),
        (
            |r| r.destination_registry_sha256 = "0".repeat(64),
            "destination registry identity differs",
        ),
        (
            |r| r.destination_card_db_hash = "fedcba9876543210".into(),
            "not this build's KERNEL_CARDDB_HASH",
        ),
        (
            |r| r.source_card_db_hash = "fedcba9876543210".into(),
            "declared source card database",
        ),
        (
            |r| r.feature_contract_digest = FEATURE_CONTRACT_DIGEST_V4.into(),
            "never migrates features",
        ),
        (
            |r| r.feature_encoding_digest = FEATURE_ENCODING_DIGEST_V4.into(),
            "never migrates features",
        ),
    ];
    for (edit, expected) in edits {
        let mut changed = request.clone();
        edit(&mut changed);
        let error = transfer(&checkpoint, &source, &destination, &changed).unwrap_err();
        assert!(error.contains(expected), "{expected}: {error}");
    }
}

/// Review finding 1: the destination is the build's own registry bytes, not
/// files that only carry its card names.
#[test]
fn card_wave_destination_must_be_the_compiled_registry_bytes() {
    let saved = learned_checkpoint();
    let checkpoint = bytes_of(&saved);
    let (source_files, destination_files) = wave_registries();
    let source = slices(&source_files);
    let destination = slices(&destination_files);
    let request = request_for(&saved, &checkpoint, &source, &destination);

    // The build changed Beta's mana value while it appended; files that keep
    // the old Beta and carry every compiled name are still refused.
    let mut compiled_files = destination_files.clone();
    compiled_files[0] = segment(vec![card("Alpha", 1), card("Beta", 2)]);
    let compiled = slices(&compiled_files);
    let error =
        transfer_compiled(&checkpoint, &source, &destination, &request, &compiled).unwrap_err();
    assert!(error.contains("compiled registry"), "{error}");
    // Given the build's own files, the change itself is what refuses.
    let request = request_for(&saved, &checkpoint, &source, &compiled);
    let error = transfer(&checkpoint, &source, &compiled, &request).unwrap_err();
    assert!(error.contains("\"mana_value\""), "{error}");

    // The same rows in one file are another identity, refused.
    let merged_files = [named(&["Alpha", "Beta", "Gamma", "Delta", "Epsilon"])];
    let merged = slices(&merged_files);
    let request = request_for(&saved, &checkpoint, &source, &merged);
    let error =
        transfer_compiled(&checkpoint, &source, &merged, &request, &destination).unwrap_err();
    assert!(error.contains("compiled registry"), "{error}");

    // Compiled names that are not the compiled files' own.
    let request = request_for(&saved, &checkpoint, &source, &destination);
    let binding = DestinationBindingV1 {
        segments: destination.clone(),
        names: vec!["Alpha", "Beta", "Gamma", "Delta", "Zeta"],
    };
    let error = transfer_to_destination_v1(&checkpoint, &source, &destination, &request, &binding)
        .unwrap_err();
    assert!(error.contains("compiled card names differ"), "{error}");

    // The public entry point binds the running build, which these synthetic
    // files are not.
    let error =
        transfer_card_wave_checkpoint_v1(&checkpoint, &source, &destination, &request).unwrap_err();
    assert!(error.contains("compiled registry"), "{error}");
}

#[test]
fn card_wave_reads_registry_files_as_build_rs_does() {
    let saved = learned_checkpoint();
    let (source, destination) = wave_registries();

    // Another schema version.
    let old_version = [
        segment_of_version(1, vec![card("Alpha", 1), card("Beta", 1)]),
        source[1].clone(),
    ];
    let versioned = pre_wave(checkpoint_fixture_v1().2, &old_version, 3);
    let error = run(&versioned, &old_version, &destination).unwrap_err();
    assert!(error.contains("schema version"), "{error}");

    // No source file at all.
    let error = run(&saved, &[], &destination).unwrap_err();
    assert!(error.contains("at least one registry segment"), "{error}");

    // One name in two files.
    let twice = [named(&["Alpha", "Beta"]), named(&["Alpha"])];
    let error = run(&saved, &twice, &destination).unwrap_err();
    assert!(error.contains("duplicate card name across"), "{error}");

    // A repeated object key in a file. The repeated file reads, by a reader
    // that keeps the last of two keys, as the source's own first file (same
    // version, same cards), and the checkpoint's origin binds its bytes, so
    // only the strict reader refuses it.
    let repeated = [b"{\"version\":2,".as_slice(), &source[0][1..]].concat();
    let mut bound = saved.clone();
    let PlayPolicyOriginV1::Imported(identity) = &mut bound.source_import else {
        panic!("the trainer's checkpoint fixture has an imported origin");
    };
    identity.destination_registry_sha256 = sha(&repeated);
    let error = run(&bound, &[repeated, source[1].clone()], &destination).unwrap_err();
    assert!(error.contains("duplicate registry object key"), "{error}");
}

/// Review finding 2: the feature and input contract are ones the trainer's
/// own resolver reads, and the tensors are that contract's model.
#[test]
fn card_wave_refuses_unsupported_feature_contracts_and_model_shapes() {
    let unread = "checkpoint feature contract is not one the trainer reads";
    let mut saved = learned_checkpoint();
    saved.feature_contract_digest = "0".repeat(64);
    saved.feature_encoding_digest = "0".repeat(64);
    assert!(refusal(&saved).contains(unread));

    let mut saved = learned_checkpoint();
    saved.feature_encoding_digest = FEATURE_ENCODING_DIGEST_V4.into();
    assert!(refusal(&saved).contains(unread));

    // An input contract under unconverted digests.
    let mut saved = no_state_digest_checkpoint();
    saved.feature_contract_digest = FEATURE_CONTRACT_DIGEST_V3.into();
    assert!(refusal(&saved).contains(unread));
    // Converted digests without their input contract.
    let mut saved = no_state_digest_checkpoint();
    saved.input_contract = None;
    assert!(refusal(&saved).contains(unread));
    // A stored state-digest column that is not +0.0.
    let mut saved = no_state_digest_checkpoint();
    let weight = saved
        .first_moments
        .iter_mut()
        .find(|tensor| tensor.name == "state_encoder.0.weight")
        .unwrap();
    weight.values[no_state_digest_v1::STATE_DIGEST_COLUMNS_V1.start] = 0.5_f32.to_bits();
    // Resealed, so the state is otherwise valid and only the column refuses.
    reseal(&mut saved);
    assert!(refusal(&saved).contains("state-digest"));

    // FlatV8 digests on net-8's 33 tensors, and V3 digests on the V8
    // inclusion net's 37, each with an origin of its digests' generation.
    let mut saved = fresh_v4_pre_wave(checkpoint_fixture_v1().2);
    saved.feature_contract_digest = FEATURE_CONTRACT_DIGEST_V8.into();
    saved.feature_encoding_digest = FEATURE_ENCODING_DIGEST_V8.into();
    assert!(refusal(&saved).contains("model shape"));
    let mut saved = flat_v8_checkpoint();
    saved.feature_contract_digest = FEATURE_CONTRACT_DIGEST_V3.into();
    saved.feature_encoding_digest = FEATURE_ENCODING_DIGEST_V3.into();
    saved.source_import = fresh_origin(FRESH_FEATURE_IDENTITY_V3);
    assert!(refusal(&saved).contains("model shape"));

    // One tensor fewer, a renamed tensor, a reshaped tensor.
    let mut saved = learned_checkpoint();
    saved.parameters.pop();
    saved.first_moments.pop();
    saved.second_moments.pop();
    assert!(refusal(&saved).contains("model shape"));
    let mut saved = learned_checkpoint();
    for section in [
        &mut saved.parameters,
        &mut saved.first_moments,
        &mut saved.second_moments,
    ] {
        section[1].name = "renamed.weight".into();
    }
    assert!(refusal(&saved).contains("model shape"));
    let mut saved = learned_checkpoint();
    for section in [
        &mut saved.parameters,
        &mut saved.first_moments,
        &mut saved.second_moments,
    ] {
        section[0].shape = vec![CARD_EMBEDDING_DIM_V1, CARD_VOCAB_SIZE_V1];
    }
    assert!(refusal(&saved).contains("model shape"));

    // A recorded state digest that is not the state's: every other check
    // passes.
    let mut saved = learned_checkpoint();
    saved.state_sha256 = "0".repeat(64);
    let error = refusal(&saved);
    assert!(
        error.contains("source checkpoint state digest differs"),
        "{error}"
    );

    // A gauge anchor that is not the scorer bias the parameters hold.
    let mut saved = learned_checkpoint();
    saved.scorer_bias_anchor_bits = (f32::from_bits(saved.scorer_bias_anchor_bits) + 1.0).to_bits();
    let error = refusal(&saved);
    assert!(!error.contains("state digest differs"), "{error}");

    // State the native validation refuses before any digest comparison: a
    // NaN first moment, a negative second moment, a moment on the padding
    // row.
    for (first, tensor, bits) in [
        (true, 3, f32::NAN.to_bits()),
        (false, 3, (-1.0_f32).to_bits()),
        (true, 0, 0.5_f32.to_bits()),
    ] {
        let mut saved = learned_checkpoint();
        let moments = if first {
            &mut saved.first_moments
        } else {
            &mut saved.second_moments
        };
        moments[tensor].values[0] = bits;
        let error = refusal(&saved);
        assert!(!error.contains("state digest differs"), "{error}");
    }
}

#[test]
fn card_wave_refuses_an_unknown_loss_and_inconsistent_scalars() {
    let mut saved = learned_checkpoint();
    saved.loss_identity = "unknown_loss/v1".into();
    assert!(refusal(&saved).contains("checkpoint loss differs"));

    let mut saved = learned_checkpoint();
    saved.loss_identity = GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1.into();
    assert!(refusal(&saved).contains("gae scalar presence"));

    let mut saved = learned_checkpoint();
    saved.gamma_bits = Some(1.0_f32.to_bits());
    saved.gae_lambda_bits = Some(0.95_f32.to_bits());
    saved.entropy_coefficient_bits = Some(0);
    assert!(refusal(&saved).contains("gae scalar presence"));

    // One or two stray GAE scalars on the terminal loss, which the presence
    // rule's conjunction alone admits.
    type ScalarEditV1 = fn(&mut ExpandedCheckpointV1);
    let strays: [ScalarEditV1; 4] = [
        |saved| saved.gamma_bits = Some(f32::NAN.to_bits()),
        |saved| saved.gae_lambda_bits = Some(0.95_f32.to_bits()),
        |saved| saved.entropy_coefficient_bits = Some(0),
        |saved| {
            saved.gamma_bits = Some(1.0_f32.to_bits());
            saved.gae_lambda_bits = Some(0.95_f32.to_bits());
        },
    ];
    for stray in strays {
        let mut saved = learned_checkpoint();
        stray(&mut saved);
        let error = refusal(&saved);
        assert!(
            error.contains("terminal loss carries a GAE scalar"),
            "{error}"
        );
    }

    // GAE scalars no update writes: gamma or lambda NaN, infinite or out of
    // range, a non-zero entropy coefficient.
    for (gamma, lambda, entropy_coefficient) in [
        (f32::NAN, 0.95_f32, 0.0_f32),
        (1.0, f32::INFINITY, 0.0),
        (1.5, 0.95, 0.0),
        (1.0, -0.25, 0.0),
        (1.0, 0.95, 0.01),
        (1.0, 0.95, f32::NAN),
    ] {
        let mut saved = learned_checkpoint();
        saved.loss_identity = GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1.into();
        saved.gamma_bits = Some(gamma.to_bits());
        saved.gae_lambda_bits = Some(lambda.to_bits());
        saved.entropy_coefficient_bits = Some(entropy_coefficient.to_bits());
        let error = refusal(&saved);
        assert!(error.contains("not ones an update writes"), "{error}");
    }

    let mut saved = learned_checkpoint();
    saved.loss_identity = "gae_uniform_kl_recovery/v1".into();
    saved.gamma_bits = Some(1.0_f32.to_bits());
    saved.gae_lambda_bits = Some(0.9_f32.to_bits());
    saved.entropy_coefficient_bits = Some(0);
    assert!(refusal(&saved).contains("exploration identity"));

    let mut saved = learned_checkpoint();
    saved.loss_identity = archive_distillation_v1::PHASE1_LOSS_IDENTITY_V1.into();
    assert!(transfer_wave(&saved).is_err());

    for (learning_rate, value_coefficient) in [
        (0.0_f32, 0.5_f32),
        (-0.001, 0.5),
        (f32::INFINITY, 0.5),
        (0.001, f32::NAN),
        (0.001, 0.0),
    ] {
        let mut saved = learned_checkpoint();
        saved.learning_rate_bits = learning_rate.to_bits();
        saved.value_coefficient_bits = value_coefficient.to_bits();
        assert!(refusal(&saved).contains("positive and finite"));
    }
}

#[test]
fn card_wave_carries_every_feature_contract_layout_and_loss_the_trainer_writes() {
    // V4 and FlatV7 as the fresh lineage writes them, with the V4 origin.
    let (_, _, mut v4) = checkpoint_fixture_v1();
    v4.feature_contract_digest = FEATURE_CONTRACT_DIGEST_V4.into();
    v4.feature_encoding_digest = FEATURE_ENCODING_DIGEST_V4.into();
    let (_, _, mut flat_v7) = checkpoint_fixture_v1();
    flat_v7.feature_contract_digest = FEATURE_CONTRACT_DIGEST_V7.into();
    flat_v7.feature_encoding_digest = FEATURE_ENCODING_DIGEST_V7.into();
    let (_, _, mut gae) = checkpoint_fixture_v1();
    gae.loss_identity = GAE_ADVANTAGE_VALUE_LOSS_IDENTITY_V1.into();
    gae.gamma_bits = Some(1.0_f32.to_bits());
    gae.gae_lambda_bits = Some(0.95_f32.to_bits());
    gae.entropy_coefficient_bits = Some(0);
    // An entropy coefficient of -0.0 passes the update's own validation
    // (`-0.0 == 0.0`), which then writes its bits as they are.
    let mut gae_negative_zero = gae.clone();
    gae_negative_zero.entropy_coefficient_bits = Some((-0.0_f32).to_bits());
    let source = wave_registries().0;
    let mut recovery = pre_wave(checkpoint_fixture_v1().2, &source, 3);
    recovery.loss_identity = "gae_uniform_kl_recovery/v1".into();
    recovery.exploration = Some(serde_json::from_str(r#"{"kind":"gae_uniform_kl_recovery_v1","gamma":1,"lambda":0.9,"initial_adam_step":32400,"recovery":true,"diagnostics":true}"#).unwrap());
    recovery.adam_step = 32_401;
    recovery.gamma_bits = Some(1.0_f32.to_bits());
    recovery.gae_lambda_bits = Some(0.9_f32.to_bits());
    recovery.entropy_coefficient_bits = Some(0);
    recovery.learning_rate_bits = 0.0001_f32.to_bits();
    recovery.value_coefficient_bits = 0.5_f32.to_bits();
    reseal(&mut recovery);

    let checkpoints = [
        fresh_v4_pre_wave(v4),
        fresh_v4_pre_wave(flat_v7),
        flat_v8_checkpoint(),
        no_state_digest_checkpoint(),
        pre_wave(gae, &source, 3),
        pre_wave(gae_negative_zero, &source, 3),
        recovery,
    ];
    for before in &checkpoints {
        let candidate = transfer_wave(before).unwrap();
        assert_carried(before, &candidate, &["Delta", "Epsilon"]);
    }
    assert_eq!(
        checkpoints[2].parameters.len(),
        learned_checkpoint().parameters.len() + 4
    );
}

/// Review finding 3: the source checkpoint's own origin and optimizer state
/// bind it to the declared source registry.
#[test]
fn card_wave_binds_the_source_checkpoint_to_its_registry() {
    let (source, destination) = wave_registries();

    // A fresh origin, under its own schema, counts the whole id space.
    let (_, _, mut fresh) = checkpoint_fixture_v1();
    fresh.schema = FRESH_CHECKPOINT_SCHEMA.into();
    let mut fresh = pre_wave(fresh, &source, 3);
    let identity = fresh_identity(OLD_CARD_DB_HASH, &source[0], 3, FRESH_FEATURE_IDENTITY_V3);
    fresh.source_import = PlayPolicyOriginV1::FreshInitialization(identity.clone());
    let candidate = transfer_wave(&fresh).unwrap();
    assert_carried(&fresh, &candidate, &["Delta", "Epsilon"]);
    assert_eq!(
        candidate.record_v1().checkpoint_schema,
        FRESH_CHECKPOINT_SCHEMA
    );
    // ... and is refused under the imported origin's schema.
    let mut wrong_schema = fresh.clone();
    wrong_schema.schema = CHECKPOINT_SCHEMA.into();
    assert!(refusal(&wrong_schema).contains("under that origin's checkpoint schema"));
    // A fresh origin of another card count: a truncated source registry.
    let mut short = fresh.clone();
    short.source_import = PlayPolicyOriginV1::FreshInitialization(fresh_identity(
        OLD_CARD_DB_HASH,
        &source[0],
        4,
        FRESH_FEATURE_IDENTITY_V3,
    ));
    assert!(refusal(&short).contains("another card count"));
    // An origin that already went through a registry transfer.
    let mut transferred = fresh.clone();
    transferred.source_import =
        PlayPolicyOriginV1::TransferredFreshInitialization(TransferredFreshPlayPolicyIdentityV1 {
            schema: TRANSFERRED_FRESH_PLAY_SCHEMA_V1.into(),
            original: identity,
            destination_registry_sha256: sha(&source[0]),
            destination_card_db_hash: OLD_CARD_DB_HASH.into(),
            destination_card_count: 3,
            transfer_envelope_sha256: "8".repeat(64),
        });
    assert!(refusal(&transferred).contains("imported or fresh origin"));
    // An import the phase-1 registry transfer re-bound: still `Imported`,
    // under its own schema.
    let mut transferred_import = learned_checkpoint();
    let PlayPolicyOriginV1::Imported(imported) = &mut transferred_import.source_import else {
        panic!("the trainer's checkpoint fixture has an imported origin");
    };
    imported.schema = REGISTRY_TRANSFERRED_IMPORT_SCHEMA.into();
    let error = refusal(&transferred_import);
    assert!(
        error.contains("never went through a registry transfer"),
        "{error}"
    );

    // An origin of another card database than the checkpoint's own.
    let mut saved = learned_checkpoint();
    saved.source_import = imported_origin(&saved.source_import, "fedcba9876543210", &source[0]);
    assert!(refusal(&saved).contains("another card database"));

    // A declared source whose first file is not the origin's.
    let saved = learned_checkpoint();
    let merged = [named(&["Alpha", "Beta", "Gamma"])];
    let error = run(&saved, &merged, &destination).unwrap_err();
    assert!(error.contains("another first registry file"), "{error}");

    // A declared source shorter than the checkpoint's own: Gamma's trained
    // row would be replaced.
    let truncated = [source[0].clone()];
    let error = run(&saved, &truncated, &destination).unwrap_err();
    assert!(error.contains("learned optimizer state"), "{error}");
    // ... and so for a fresh origin, refused by its count first.
    let error = run(&fresh, &truncated, &destination).unwrap_err();
    assert!(error.contains("another card count"), "{error}");

    // Optimizer state in an appended row or far past every registry, in
    // either moment, including a -0.0.
    for (first, row, bits) in [
        (true, 4, 0.5_f32.to_bits()),
        (true, FAR_ROW, 0.5_f32.to_bits()),
        (false, FAR_ROW, 0.5_f32.to_bits()),
        (true, FAR_ROW, 0x8000_0000),
        (false, FAR_ROW, 0x8000_0000),
    ] {
        let mut saved = learned_checkpoint();
        let moments = if first {
            &mut saved.first_moments
        } else {
            &mut saved.second_moments
        };
        moments[0].values[row * CARD_EMBEDDING_DIM_V1 + 5] = bits;
        reseal(&mut saved);
        assert!(refusal(&saved).contains("learned optimizer state"));
    }
}

/// Review round 2, finding 2: the origin's feature identity is of the
/// checkpoint's own feature generation, as every trainer path that writes
/// the checkpoint requires.
#[test]
fn card_wave_binds_the_origin_feature_identity_to_the_checkpoint_generation() {
    let mismatch = "not of the checkpoint's own feature generation";
    let v4_generation = |contract: &str, encoding: &str| {
        let mut saved = fresh_v4_pre_wave(checkpoint_fixture_v1().2);
        saved.feature_contract_digest = contract.into();
        saved.feature_encoding_digest = encoding.into();
        saved
    };
    // The V4 generation with a V4 origin transfers; with a V3 origin
    // (imported, or fresh) it is refused, for V4, FlatV7 and FlatV8 alike.
    let v4 = v4_generation(FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V4);
    transfer_wave(&v4).unwrap();
    for contract in [
        (FEATURE_CONTRACT_DIGEST_V4, FEATURE_ENCODING_DIGEST_V4),
        (FEATURE_CONTRACT_DIGEST_V7, FEATURE_ENCODING_DIGEST_V7),
    ] {
        let mut saved = v4_generation(contract.0, contract.1);
        saved.source_import = fresh_origin(FRESH_FEATURE_IDENTITY_V3);
        assert!(refusal(&saved).contains(mismatch));
        let mut imported = v4_generation(contract.0, contract.1);
        imported.schema = CHECKPOINT_SCHEMA.into();
        imported.source_import = learned_checkpoint().source_import;
        assert!(refusal(&imported).contains(mismatch));
    }
    let mut flat_v8 = flat_v8_checkpoint();
    flat_v8.source_import = fresh_origin(FRESH_FEATURE_IDENTITY_V3);
    assert!(refusal(&flat_v8).contains(mismatch));

    // The V3 generation with a V4 origin: fresh, or an import of V4 digests.
    let mut saved = learned_checkpoint();
    saved.schema = FRESH_CHECKPOINT_SCHEMA.into();
    saved.source_import = fresh_origin(FRESH_FEATURE_IDENTITY_V4);
    assert!(refusal(&saved).contains(mismatch));
    let mut saved = learned_checkpoint();
    let PlayPolicyOriginV1::Imported(identity) = &mut saved.source_import else {
        panic!("the trainer's checkpoint fixture has an imported origin");
    };
    identity.feature_contract_digest = FEATURE_CONTRACT_DIGEST_V4.into();
    identity.feature_encoding_digest = FEATURE_ENCODING_DIGEST_V4.into();
    assert!(refusal(&saved).contains(mismatch));
    // The no-state-digest conversion keeps its base generation's origin.
    let mut converted = no_state_digest_checkpoint();
    converted.schema = FRESH_CHECKPOINT_SCHEMA.into();
    converted.source_import = fresh_origin(FRESH_FEATURE_IDENTITY_V4);
    assert!(refusal(&converted).contains(mismatch));

    // A fresh origin whose four feature fields are no compiled generation,
    // though each is well-formed hex.
    let mut saved = learned_checkpoint();
    saved.schema = FRESH_CHECKPOINT_SCHEMA.into();
    let PlayPolicyOriginV1::FreshInitialization(mut identity) =
        fresh_origin(FRESH_FEATURE_IDENTITY_V3)
    else {
        unreachable!()
    };
    identity.features_source_sha256 = "e".repeat(64);
    saved.source_import = PlayPolicyOriginV1::FreshInitialization(identity);
    assert!(refusal(&saved).contains(mismatch));
}

/// Review finding 4: a continuation member refuses even as an explicit
/// `null`, and only the canonical serialization of a v1 JSON checkpoint is
/// read, so the destination drops nothing the source held.
#[test]
fn card_wave_refuses_continuation_members_and_non_canonical_json() {
    let saved = learned_checkpoint();
    for member in ["e_supported", "soft_q", "search_targets", "ex1"] {
        for value in ["null", "{}"] {
            let bytes = with_member(&saved, &format!("\"{member}\":{value}"));
            let error = transfer_bytes(&saved, &bytes).unwrap_err();
            assert!(error.contains("continuation member"), "{member}: {error}");
        }
    }
    let mut with_e = saved.clone();
    with_e.e_supported = Some(serde_json::json!({"pin": "x"}));
    assert!(refusal(&with_e).contains("continuation member"));

    let mut distilled = saved.clone();
    let pin = |name: &str, fill: char| PinnedFileV1 {
        path: PathBuf::from("/synthetic").join(name),
        sha256: fill.to_string().repeat(64),
    };
    distilled.archive_distillation = Some(ArchiveDistillationProvenanceV1 {
        method: "archive-distillation".into(),
        lineage: "r1".into(),
        arm: ArchiveDistillationArmV1::Uniform,
        parent: pin("parent.json", 'c'),
        parent_state_sha256: saved.state_sha256.clone(),
        games: pin("games.jsonl", 'd'),
        archive_sources: vec![pin("ARCHIVE.jsonl", '5')],
        bank: pin("bank.json", 'e'),
        bank_rows_sha256: "f".repeat(64),
        anchor_sha256: "1".repeat(64),
        steps: 179,
        learning_rate_bits: saved.learning_rate_bits,
        phase1_state_sha256: "2".repeat(64),
        runtime_commit: "test".into(),
        cell: None,
        update_scope: Default::default(),
        rules_pathway: None,
    });
    assert!(refusal(&distilled).contains("archive-distillation provenance"));

    // Registry-transfer continuation provenance, typed, and as an explicit
    // `null`, which the trainer's reader refuses as no continuation.
    let mut continued = saved.clone();
    continued.registry_transfer = Some(ExpandedRegistryContinuationV1 {
        schema: "mtg-kernel-expanded-registry-continuation/v1".into(),
        source_descriptor_sha256: "1".repeat(64),
        source_checkpoint_sha256: "2".repeat(64),
        source_registry_sha256: "3".repeat(64),
        transfer_envelope_sha256: "4".repeat(64),
        continuation_schedule_sha256: "5".repeat(64),
        transfer_state_sha256: "6".repeat(64),
        transfer_adam_step: 2,
        completed_updates: 1,
        consumed_batch_sha256: "7".repeat(64),
        preserved_scalars: RegistryTransferScalarsV1 {
            optimizer_identity: "adam".into(),
            adam_beta1_bits: 0.9_f32.to_bits(),
            adam_beta2_bits: 0.999_f32.to_bits(),
            adam_epsilon_bits: 1e-8_f32.to_bits(),
            adam_weight_decay_bits: 0,
            adam_step: 2,
            scorer_bias_anchor_bits: saved.scorer_bias_anchor_bits,
            loss_identity: saved.loss_identity.clone(),
            learning_rate_bits: saved.learning_rate_bits,
            value_coefficient_bits: saved.value_coefficient_bits,
        },
    });
    let error = refusal(&continued);
    assert!(error.contains("registry-transfer"), "{error}");
    let bytes = with_member(&saved, "\"registry_transfer\":null");
    let error = transfer_bytes(&saved, &bytes).unwrap_err();
    assert!(error.contains("null"), "{error}");

    // Other spacing, and a repeated key.
    let pretty = serde_json::to_vec_pretty(&saved).unwrap();
    let error = transfer_bytes(&saved, &pretty).unwrap_err();
    assert!(error.contains("canonical serialization"), "{error}");
    let repeated = with_member(
        &saved,
        "\"schema\":\"mtg-kernel-expanded-deck-checkpoint/v1\"",
    );
    assert!(transfer_bytes(&saved, &repeated).is_err());
}

#[test]
fn card_wave_reads_container_and_sealed_checkpoints_into_the_same_destination() {
    let saved = learned_checkpoint();
    let json = bytes_of(&saved);
    let expected = transfer_bytes(&saved, &json).unwrap();
    let container = checkpoint_container_v1::encode_v1(&saved).unwrap();
    let sealed_json = record_envelope_v1::RecordEnvelopeV1::Zstd1
        .seal_v1(&json)
        .unwrap()
        .into_owned();
    let sealed_container = record_envelope_v1::RecordEnvelopeV1::Zstd1
        .seal_v1(&container)
        .unwrap()
        .into_owned();
    for bytes in [container, sealed_json, sealed_container] {
        let candidate = transfer_bytes(&saved, &bytes).unwrap();
        assert_eq!(
            candidate.checkpoint_bytes_v1(),
            expected.checkpoint_bytes_v1()
        );
        assert_eq!(
            candidate.record_v1().request.source_checkpoint_sha256,
            sha(&bytes)
        );
    }
}

/// Review finding 5: the record is accepted only as the reproduced
/// transfer's own bytes.
#[test]
fn card_wave_record_replay_accepts_the_transfer_and_rejects_tampering() {
    let saved = learned_checkpoint();
    let (source, destination) = wave_registries();
    let source = slices(&source);
    let destination = slices(&destination);
    let checkpoint = bytes_of(&saved);
    let request = request_for(&saved, &checkpoint, &source, &destination);
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
    let edited = serde_json::to_vec_pretty(&edited).unwrap();
    assert!(verify_reproduced_v1(&edited, written, reproduce).is_err());
    // The same document from other bytes.
    let compact = serde_json::to_vec(candidate.record_v1()).unwrap();
    assert!(verify_reproduced_v1(&compact, written, reproduce).is_err());
    // A forged member ahead of the real one.
    let forged = [
        b"{\"destination_checkpoint_sha256\":\"".as_slice(),
        "0".repeat(64).as_bytes(),
        b"\",",
        &record[1..],
    ]
    .concat();
    assert!(verify_reproduced_v1(&forged, written, reproduce).is_err());
    let mut other_schema = candidate.record_v1().clone();
    other_schema.schema = "other".into();
    let other_schema = serde_json::to_vec_pretty(&other_schema).unwrap();
    let error = verify_reproduced_v1(&other_schema, written, reproduce).unwrap_err();
    assert!(error.contains("record schema differs"), "{error}");

    let mut tampered = parsed(&candidate);
    tampered.first_moments[1].values[0] ^= 1;
    let tampered = bytes_of(&tampered);
    assert!(verify_reproduced_v1(&record, &tampered, reproduce).is_err());
    // Another valid transfer of the same source.
    let mut reseeded = request.clone();
    reseeded.initialization_seed = SEED + 1;
    let other = transfer(&checkpoint, &source, &destination, &reseeded).unwrap();
    assert!(verify_reproduced_v1(&record, other.checkpoint_bytes_v1(), reproduce).is_err());
}

/// This build's compiled registry with the last two cards of its last file
/// removed (the source of a synthetic wave into this build), and the
/// compiled registry itself.
fn compiled_wave() -> (Vec<Vec<u8>>, Vec<&'static [u8]>) {
    let destination = COMPILED_REGISTRY.to_vec();
    let mut source: Vec<Vec<u8>> = destination.iter().map(|file| file.to_vec()).collect();
    let last = source.last_mut().unwrap();
    let mut document = strict_registry_document(last).unwrap();
    let cards = document["cards"].as_array_mut().unwrap();
    cards.truncate(cards.len() - 2);
    *last = serde_json::to_vec(&document).unwrap();
    (source, destination)
}

#[test]
fn card_wave_public_transfer_and_verification_into_this_build() {
    let (source_files, destination) = compiled_wave();
    let source = slices(&source_files);
    let saved = pre_wave(checkpoint_fixture_v1().2, &source_files, 3);
    let checkpoint = bytes_of(&saved);
    let request = request_for(&saved, &checkpoint, &source, &destination);
    let candidate =
        transfer_card_wave_checkpoint_v1(&checkpoint, &source, &destination, &request).unwrap();
    let record = candidate.record_v1();
    let compiled_names: Vec<&str> = CARD_DEFS.iter().map(|card| card.name).collect();
    let appended: Vec<&str> = record
        .appended_cards
        .iter()
        .map(|card| card.name.as_str())
        .collect();
    assert_eq!(appended, compiled_names[compiled_names.len() - 2..]);
    assert_eq!(record.destination_card_count, CARD_DEFS.len());

    let record_bytes = candidate.record_bytes_v1().unwrap();
    let written = candidate.checkpoint_bytes_v1();
    let verify = |record: &[u8], written: &[u8], source_checkpoint: &[u8]| {
        verify_card_wave_transfer_v1(record, written, source_checkpoint, &source, &destination)
    };
    verify(&record_bytes[..], written, &checkpoint[..]).unwrap();

    // Another source checkpoint, another destination checkpoint, another
    // record schema, other destination files.
    let mut other_source = saved.clone();
    other_source.adam_step = 4;
    reseal(&mut other_source);
    let other_source = bytes_of(&other_source);
    assert!(verify(&record_bytes[..], written, &other_source[..]).is_err());
    let mut reseeded = request.clone();
    reseeded.initialization_seed = SEED + 1;
    let other =
        transfer_card_wave_checkpoint_v1(&checkpoint, &source, &destination, &reseeded).unwrap();
    assert!(verify(
        &record_bytes[..],
        other.checkpoint_bytes_v1(),
        &checkpoint[..]
    )
    .is_err());
    let mut other_schema = record.clone();
    other_schema.schema = "other".into();
    let other_schema = serde_json::to_vec_pretty(&other_schema).unwrap();
    assert!(verify(&other_schema[..], written, &checkpoint[..]).is_err());
    let foreign =
        verify_card_wave_transfer_v1(&record_bytes, written, &checkpoint, &source, &source);
    assert!(foreign.is_err());
}

/// A fresh, absolute directory for one command test.
fn scratch_directory(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "card-wave-transfer-{name}-{}-{nanos}",
        std::process::id()
    ))
}

#[test]
fn card_wave_command_writes_verified_files_and_requires_its_pins() {
    let root = scratch_directory("command");
    fs::create_dir_all(&root).unwrap();
    let (source_files, destination) = compiled_wave();
    let saved = pre_wave(checkpoint_fixture_v1().2, &source_files, 3);
    let checkpoint = bytes_of(&saved);
    let checkpoint_path = root.join("source-checkpoint.json");
    fs::write(&checkpoint_path, &checkpoint).unwrap();
    let write_all = |prefix: &str, files: &[&[u8]]| -> Vec<PathBuf> {
        files
            .iter()
            .enumerate()
            .map(|(ordinal, bytes)| {
                let path = root.join(format!("{prefix}-{ordinal}.json"));
                fs::write(&path, bytes).unwrap();
                path
            })
            .collect()
    };
    let source_paths = write_all("source", &slices(&source_files));
    let destination_paths = write_all("destination", &destination);
    let request = |output: &Path, contract: &str| {
        serde_json::to_vec(&serde_json::json!({
            "schema": CLI_REQUEST_SCHEMA,
            "source_checkpoint": checkpoint_path,
            "source_card_db_hash": OLD_CARD_DB_HASH,
            "feature_contract_digest": contract,
            "feature_encoding_digest": FEATURE_ENCODING_DIGEST_V3,
            "source_registry": source_paths,
            "destination_registry": destination_paths,
            "initialization_seed": SEED,
            "output_directory": output,
        }))
        .unwrap()
    };

    let output = root.join("out");
    let result =
        run_card_wave_transfer_cli_json_v1(&request(output.as_path(), FEATURE_CONTRACT_DIGEST_V3))
            .unwrap();
    assert_eq!(result["verified"], true);
    let source = slices(&source_files);
    let pinned = request_for(&saved, &checkpoint, &source, &destination);
    let expected =
        transfer_card_wave_checkpoint_v1(&checkpoint, &source, &destination, &pinned).unwrap();
    assert_eq!(
        fs::read(output.join("checkpoint.json")).unwrap(),
        expected.checkpoint_bytes_v1()
    );
    assert_eq!(
        fs::read(output.join("transfer-record.json")).unwrap(),
        expected.record_bytes_v1().unwrap()
    );

    // An existing output directory.
    let error =
        run_card_wave_transfer_cli_json_v1(&request(output.as_path(), FEATURE_CONTRACT_DIGEST_V3))
            .unwrap_err();
    assert!(error.contains("already exists"), "{error}");
    // The caller's feature pin is compared, never copied from the checkpoint.
    let other = root.join("other");
    let error =
        run_card_wave_transfer_cli_json_v1(&request(other.as_path(), FEATURE_CONTRACT_DIGEST_V4))
            .unwrap_err();
    assert!(error.contains("never migrates features"), "{error}");
    assert!(!other.exists());
    // A repeated key, an unknown member, an oversized request.
    let valid = request(other.as_path(), FEATURE_CONTRACT_DIGEST_V3);
    let repeated = [b"{\"schema\":\"x\",".as_slice(), &valid[1..]].concat();
    assert!(run_card_wave_transfer_cli_json_v1(&repeated).is_err());
    let unknown = [b"{\"unknown\":1,".as_slice(), &valid[1..]].concat();
    assert!(run_card_wave_transfer_cli_json_v1(&unknown).is_err());
    let oversized = vec![b' '; MAX_CLI_REQUEST_BYTES + 1];
    let error = run_card_wave_transfer_cli_json_v1(&oversized).unwrap_err();
    assert!(error.contains("exceeds 1 MiB"), "{error}");
    assert!(!other.exists());
    // An input that is not a regular file.
    let mut not_a_file: Value = serde_json::from_slice(&valid).unwrap();
    not_a_file["source_checkpoint"] = serde_json::json!(root);
    let error =
        run_card_wave_transfer_cli_json_v1(&serde_json::to_vec(&not_a_file).unwrap()).unwrap_err();
    assert!(error.contains("not a regular file"), "{error}");
    assert!(!other.exists());
    fs::remove_dir_all(&root).unwrap();
}

/// Review round 2, nit 6: a failed publication removes what it published,
/// whichever step fails: the second file's publication after the first
/// succeeded, or the verification after both.
#[test]
fn card_wave_publication_removes_its_files_on_any_failure() {
    let root = scratch_directory("publication");
    let directory = root.join("out");
    fs::create_dir_all(&directory).unwrap();
    // The second publication fails: its name is taken by the first.
    let taken = [
        ("checkpoint.json", b"{}".as_slice()),
        ("checkpoint.json", b"{}".as_slice()),
    ];
    let error = publish_verified_outputs_v1(&directory, &taken, |_| Ok(())).unwrap_err();
    assert!(!directory.exists(), "{error}");

    let outputs = [
        ("checkpoint.json", b"{}".as_slice()),
        ("transfer-record.json", b"[]".as_slice()),
    ];
    fs::create_dir(&directory).unwrap();
    let error = publish_verified_outputs_v1(&directory, &outputs, |pins| {
        assert_eq!(pins.len(), 2);
        Err("refused".into())
    })
    .unwrap_err();
    assert_eq!(error, "refused");
    assert!(!directory.exists());

    fs::create_dir(&directory).unwrap();
    let pins = publish_verified_outputs_v1(&directory, &outputs, |_| Ok(())).unwrap();
    assert_eq!(fs::read(&pins[0].path).unwrap(), b"{}");
    assert_eq!(fs::read(&pins[1].path).unwrap(), b"[]");
    fs::remove_dir_all(&root).unwrap();
}
