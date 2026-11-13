//! The decoders' in-place canonical check ([`is_frame_of_v1`]) against the
//! re-encoding each made before (milestone 3 of #596): every other check is
//! shared, so the two decodes differ only where that one check does.

use super::*;
use std::fmt::Debug;

/// The smallest S config `SConfigV1::validate_v1` admits: the file is mostly
/// frame (header, JSON and 256-byte alignment), so every byte can be changed.
const SMALLEST_V1: SConfigV1 = SConfigV1 {
    d_model: 1,
    layers: 1,
    heads: 1,
    ffn: 1,
    card_dim: 1,
    card_vocab: 2,
};

/// A train state with nonzero moments everywhere but the gauge ranges.
fn state_v1() -> STrainStateV1 {
    let model = SModelV1::init_parity_v1(SMALLEST_V1, 8).unwrap();
    let total = model.layout_v1().total;
    let mut first: Vec<f32> = (0..total).map(|i| (i as f32 * 0.25 - 1.0) * 1e-3).collect();
    let mut second: Vec<f32> = (0..total).map(|i| (i as f32 + 1.0) * 1e-6).collect();
    for range in model.layout_v1().gauge_ranges_v1() {
        first[range.clone()].fill(0.0);
        second[range].fill(0.0);
    }
    STrainStateV1::from_parts_v1(model, first, second, 7).unwrap()
}

/// [`decode_checkpoint_v1`] as it was: every other check, then the state
/// re-encoded and compared with the bytes.
fn decode_with_copy_v1(bytes: &[u8]) -> Result<STrainStateV1, String> {
    let (state, _) = decode_state_v1(bytes)?;
    if encode_checkpoint_v1(&state) != bytes {
        return Err("S checkpoint is not the canonical encoding of its state".into());
    }
    Ok(state)
}

/// [`decode_weights_snapshot_v1`] as it was: every other check, then the
/// snapshot re-framed and compared with the bytes.
fn decode_snapshot_with_copy_v1(bytes: &[u8]) -> Result<SWeightsSnapshotV1, String> {
    let (document, table, model) = decode_snapshot_parts_v1(bytes)?;
    let layout = LayoutV1::of_v1(document.config)?;
    let canonical = frame_v1(
        &layout,
        &snapshot_document_v1(
            document.config,
            document.adam_step,
            &document.cut_from_state_sha256,
            &document.parameters_sha256,
            &document.record,
        ),
        &table,
        &SNAPSHOT_SECTIONS_V1,
        &[model.values_v1()],
    );
    if canonical != bytes {
        return Err("S weights snapshot is not the canonical encoding of its parameters".into());
    }
    Ok(SWeightsSnapshotV1 {
        model,
        adam_step: document.adam_step,
        cut_from_state_sha256: document.cut_from_state_sha256,
        record: document.record,
    })
}

/// Decodes `bytes` both ways and asserts the same outcome: the same value, or
/// the same refusal. Returns whether it was accepted.
fn same_outcome_v1<T: Debug + PartialEq>(
    bytes: &[u8],
    decode: fn(&[u8]) -> Result<T, String>,
    reference: fn(&[u8]) -> Result<T, String>,
    what: &str,
) -> bool {
    let got = decode(bytes);
    assert_eq!(got, reference(bytes), "{what}");
    got.is_ok()
}

/// Where `container`'s tensor values lie.
fn value_ranges_v1(container: &[u8]) -> Vec<std::ops::Range<usize>> {
    let (_, table, data_start) = split_v1(container).unwrap();
    let table: SectionTableV1 = serde_json::from_slice(table).unwrap();
    table
        .sections
        .iter()
        .map(|entry| {
            let start = data_start + entry.offset as usize;
            start..start + entry.byte_length as usize
        })
        .collect()
}

/// `container` and single-byte changes of it (three per byte), every
/// truncation and four trailing tails, decoded both ways with the same
/// outcome. Every byte of the header, document, table and padding is
/// changed. A value byte is data, read where the encoder writes it: a change
/// to one changes the state, which both decodes refuse before their
/// canonical checks (its hash, finiteness or gauge), so only each tensor's
/// first and last bytes are changed. Returns how many were accepted and how
/// many refused.
fn same_refusals_v1<T: Debug + PartialEq>(
    container: &[u8],
    decode: fn(&[u8]) -> Result<T, String>,
    reference: fn(&[u8]) -> Result<T, String>,
) -> (usize, usize) {
    let values = value_ranges_v1(container);
    let sampled = |at: usize| {
        values
            .iter()
            .all(|range| !range.contains(&at) || at == range.start || at + 1 == range.end)
    };
    let mut accepted = 0usize;
    let mut refused = 0usize;
    let mut check = |bytes: &[u8], what: &str| {
        if same_outcome_v1(bytes, decode, reference, what) {
            accepted += 1;
        } else {
            refused += 1;
        }
    };
    check(container, "the container itself");
    let changes: [fn(u8) -> u8; 3] = [|b| b ^ 0x01, |b| b ^ 0x80, |_| 0x20];
    for at in (0..container.len()).filter(|&at| sampled(at)) {
        for change in changes {
            let mut bytes = container.to_vec();
            bytes[at] = change(bytes[at]);
            if bytes[at] != container[at] {
                check(&bytes, &format!("byte {at} changed"));
            }
        }
    }
    for length in 0..container.len() {
        check(&container[..length], &format!("truncated to {length}"));
    }
    let tails: [&[u8]; 4] = [&[0], &[1], &[0; 4], &[0; 256]];
    for tail in tails {
        check(&[container, tail].concat(), "trailing bytes");
    }
    (accepted, refused)
}

/// The container with `document` and `table` in place of its own, the
/// header's lengths to match and its data at the new data start.
fn reframed_v1(container: &[u8], document: &[u8], table: &[u8]) -> Vec<u8> {
    let (_, _, data_start) = split_v1(container).unwrap();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC_V1);
    bytes.extend_from_slice(&CONTAINER_VERSION_V1.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&(document.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&(table.len() as u64).to_le_bytes());
    bytes.extend_from_slice(document);
    bytes.extend_from_slice(table);
    bytes.resize(align_up_v1(bytes.len()), 0);
    bytes.extend_from_slice(&container[data_start..]);
    bytes
}

/// Rewrites that reach the canonical check, which no single-byte change
/// makes: the same document and table reformatted, so they parse to what the
/// decoder expects but are not the bytes the encoder writes.
fn rewrites_v1(container: &[u8]) -> Vec<(&'static str, Vec<u8>)> {
    let (document, table, _) = split_v1(container).unwrap();
    let value = |bytes: &[u8]| serde_json::from_slice::<Value>(bytes).unwrap();
    let mut reversed = serde_json::Map::new();
    for (name, member) in value(document).as_object().unwrap().iter().rev() {
        reversed.insert(name.clone(), member.clone());
    }
    let reordered = serde_json::to_vec(&reversed).unwrap();
    assert_ne!(reordered, document);
    vec![
        (
            "the container itself",
            reframed_v1(container, document, table),
        ),
        (
            "a pretty document",
            reframed_v1(
                container,
                &serde_json::to_vec_pretty(&value(document)).unwrap(),
                table,
            ),
        ),
        (
            "a document with its members in another order",
            reframed_v1(container, &reordered, table),
        ),
        (
            "a document with a trailing space",
            reframed_v1(container, &[document, b" "].concat(), table),
        ),
        (
            "a pretty table",
            reframed_v1(
                container,
                document,
                &serde_json::to_vec_pretty(&value(table)).unwrap(),
            ),
        ),
        (
            "a table with a leading space",
            reframed_v1(container, document, &[b" ", table].concat()),
        ),
    ]
}

#[test]
fn the_in_place_check_refuses_exactly_what_a_reencoded_checkpoint_refuses() {
    let state = state_v1();
    let container = encode_checkpoint_v1(&state);
    assert_eq!(decode_checkpoint_v1(&container).unwrap(), state);
    let (accepted, refused) =
        same_refusals_v1(&container, decode_checkpoint_v1, decode_with_copy_v1);
    // A changed value changes the state's hash, so only the container itself
    // decodes.
    assert!(accepted == 1 && refused > container.len());
    for (what, bytes) in rewrites_v1(&container) {
        let accepted = same_outcome_v1(&bytes, decode_checkpoint_v1, decode_with_copy_v1, what);
        assert_eq!(accepted, what == "the container itself", "{what}");
    }
    // A change to a padding byte reaches the canonical check itself.
    let (_, _, data_start) = split_v1(&container).unwrap();
    let mut padded = container.clone();
    padded[data_start - 1] = 1;
    assert_eq!(
        decode_checkpoint_v1(&padded).unwrap_err(),
        "S checkpoint is not the canonical encoding of its state"
    );
}

#[test]
fn the_in_place_check_refuses_exactly_what_a_reframed_snapshot_refuses() {
    let state = state_v1();
    let record = serde_json::json!({"path": "state.bin", "sha256": "00"});
    let snapshot = encode_weights_snapshot_v1(&state, &record);
    let decoded = decode_weights_snapshot_v1(&snapshot).unwrap();
    assert_eq!(decoded.model, *state.model_v1());
    let (accepted, refused) = same_refusals_v1(
        &snapshot,
        decode_weights_snapshot_v1,
        decode_snapshot_with_copy_v1,
    );
    // The document's Adam step, cut-from hash and record are data the
    // snapshot carries, unchecked, so a change to one of them that leaves
    // canonical JSON decodes either way. A changed value changes the
    // parameters' hash.
    assert!(accepted > 1 && refused > snapshot.len());
    for (what, bytes) in rewrites_v1(&snapshot) {
        let accepted = same_outcome_v1(
            &bytes,
            decode_weights_snapshot_v1,
            decode_snapshot_with_copy_v1,
            what,
        );
        assert_eq!(accepted, what == "the container itself", "{what}");
    }
}
