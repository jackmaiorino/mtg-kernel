use super::*;
use crate::expanded_deck_training_v1::tests::checkpoint_fixture_v1;

fn fixture_v1() -> ExpandedCheckpointV1 {
    let (_, _, mut saved) = checkpoint_fixture_v1();
    // Exercise optional members the exporter must reproduce.
    saved.gamma_bits = Some(0.99_f32.to_bits());
    saved.gae_lambda_bits = Some(0.95_f32.to_bits());
    saved.entropy_coefficient_bits = Some(0.0_f32.to_bits());
    saved
}

fn json_v1(saved: &ExpandedCheckpointV1) -> Vec<u8> {
    serde_json::to_vec(saved).unwrap()
}

fn with_lengths(document: &[u8], table: &[u8], data: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC_V1);
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&(document.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&(table.len() as u64).to_le_bytes());
    bytes.extend_from_slice(document);
    bytes.extend_from_slice(table);
    let start = bytes.len().div_ceil(256) * 256;
    bytes.resize(start, 0);
    bytes.extend_from_slice(data);
    bytes
}

fn parts(bytes: &[u8]) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let header = header_v1(bytes).unwrap();
    (
        bytes[header.document].to_vec(),
        bytes[header.table].to_vec(),
        bytes[header.data_start..].to_vec(),
    )
}

#[test]
fn a_container_decodes_to_its_checkpoint_and_exports_its_v1_bytes() {
    let saved = fixture_v1();
    let v1 = json_v1(&saved);
    let container = encode_v1(&saved).unwrap();
    assert!(is_container_v1(&container) && !is_container_v1(&v1));
    assert_eq!(json_v1(&decode_v1(&container).unwrap()), v1);
    assert_eq!(export_v1(&container).unwrap(), v1);
    assert_eq!(convert_v1_bytes(&v1).unwrap(), container);
    assert_eq!(&*json_document_v1(&container).unwrap(), v1.as_slice());
    assert_eq!(&*json_document_v1(&v1).unwrap(), v1.as_slice());
    assert_eq!(json_v1(&parse_checkpoint_v1(&container).unwrap()), v1);
    assert_eq!(json_v1(&parse_checkpoint_v1(&v1).unwrap()), v1);
    // Four bytes per value, padding and a small document: the size is set by
    // the shapes alone, whatever the values.
    let values: usize = sections_of_v1(&saved)
        .iter()
        .flat_map(|tensors| tensors.iter())
        .map(|t| t.values.len())
        .sum();
    assert!(container.len() >= 4 * values);
}

#[test]
fn every_tensor_section_is_aligned_and_in_train_step_order() {
    let saved = fixture_v1();
    let container = encode_v1(&saved).unwrap();
    let header = header_v1(&container).unwrap();
    assert_eq!(header.data_start % ALIGN_V1, 0);
    let table: SectionTableV1 = serde_json::from_slice(&container[header.table]).unwrap();
    assert_eq!(table.state_format, STATE_FORMAT_V1);
    let expected: Vec<_> = SECTION_NAMES_V1
        .iter()
        .zip(sections_of_v1(&saved))
        .flat_map(|(section, tensors)| {
            tensors
                .iter()
                .enumerate()
                .map(move |(ordinal, t)| (section.to_string(), ordinal, t.name.clone()))
        })
        .collect();
    let actual: Vec<_> = table
        .sections
        .iter()
        .map(|e| (e.section.clone(), e.ordinal, e.name.clone()))
        .collect();
    assert_eq!(actual, expected);
    for entry in &table.sections {
        assert_eq!(entry.offset % ALIGN_V1 as u64, 0);
        let start = header.data_start + entry.offset as usize;
        let tensor = &sections_of_v1(&saved)[SECTION_NAMES_V1
            .iter()
            .position(|s| *s == entry.section)
            .unwrap()][entry.ordinal];
        assert_eq!(
            u32::from_le_bytes(container[start..start + 4].try_into().unwrap()),
            tensor.values[0]
        );
    }
    let document: serde_json::Value =
        serde_json::from_slice(document_v1(&container).unwrap()).unwrap();
    assert_eq!(document["parameters"], serde_json::json!([]));
    assert_eq!(
        document["state_sha256"],
        serde_json::json!(saved.state_sha256)
    );
    assert_eq!(
        metadata_view_v1(&container).unwrap(),
        document_v1(&container).unwrap()
    );
}

#[test]
fn every_byte_outside_the_tensor_values_is_bound_by_the_canonical_check() {
    let container = encode_v1(&fixture_v1()).unwrap();
    let header = header_v1(&container).unwrap();
    let refused = |bytes: &[u8]| decode_v1(bytes).is_err();
    // Version, reserved word, lengths.
    for at in [8usize, 12, 16, 24] {
        let mut bytes = container.clone();
        bytes[at] ^= 1;
        assert!(refused(&bytes), "header byte {at}");
    }
    // Padding after the table and between tensors.
    let mut bytes = container.clone();
    bytes[header.data_start - 1] = 1;
    assert!(refused(&bytes), "padding before the data");
    let table: SectionTableV1 = serde_json::from_slice(&container[header.table.clone()]).unwrap();
    let gap = table
        .sections
        .windows(2)
        .find(|w| w[0].offset + w[0].byte_length < w[1].offset)
        .map(|w| header.data_start + (w[0].offset + w[0].byte_length) as usize)
        .expect("some tensor is followed by padding");
    let mut bytes = container.clone();
    bytes[gap] = 1;
    assert!(refused(&bytes), "padding between tensors");
    // Truncated or extended files.
    assert!(refused(&container[..container.len() - 1]));
    let mut longer = container.clone();
    longer.push(0);
    assert!(refused(&longer));
    assert!(refused(&container[..20]));
    // A value byte is data: it decodes to a different state, which the pin
    // and the state hash refuse elsewhere.
    let mut bytes = container.clone();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    assert!(decode_v1(&bytes).is_ok());
}

#[test]
fn a_non_canonical_document_or_table_is_refused() {
    let saved = fixture_v1();
    let container = encode_v1(&saved).unwrap();
    let (document, table, data) = parts(&container);
    assert_eq!(
        decode_v1(&with_lengths(&document, &table, &data))
            .map(|s| json_v1(&s))
            .unwrap(),
        json_v1(&saved)
    );
    let pretty: serde_json::Value = serde_json::from_slice(&document).unwrap();
    let spaced = serde_json::to_vec_pretty(&pretty).unwrap();
    assert!(decode_v1(&with_lengths(&spaced, &table, &data))
        .unwrap_err()
        .contains("canonical"));
    let spaced_table = [b" ".as_slice(), &table].concat();
    assert!(decode_v1(&with_lengths(&document, &spaced_table, &data)).is_err());
    // A document that carries tensor values itself.
    let full = json_v1(&saved);
    assert!(decode_v1(&with_lengths(&full, &table, &data))
        .unwrap_err()
        .contains("carries tensor values"));
    // Another state format.
    let mut other: serde_json::Value = serde_json::from_slice(&table).unwrap();
    other["state_format"] = serde_json::json!("mtgts-sections-v2");
    let other = serde_json::to_vec(&other).unwrap();
    assert!(decode_v1(&with_lengths(&document, &other, &data))
        .unwrap_err()
        .contains("state format"));
    // A section table out of order.
    let mut swapped: SectionTableV1 = serde_json::from_slice(&table).unwrap();
    swapped.sections.swap(0, 1);
    let swapped = serde_json::to_vec(&swapped).unwrap();
    assert!(decode_v1(&with_lengths(&document, &swapped, &data)).is_err());
}

#[test]
fn the_converter_admits_only_canonical_v1_checkpoints() {
    let saved = fixture_v1();
    let v1 = json_v1(&saved);
    let pretty = serde_json::to_vec_pretty(&saved).unwrap();
    assert!(convert_v1_bytes(&pretty)
        .unwrap_err()
        .contains("not in canonical form"));
    let container = convert_v1_bytes(&v1).unwrap();
    assert!(convert_v1_bytes(&container)
        .unwrap_err()
        .contains("already a checkpoint container"));
}

#[test]
fn a_tensor_whose_values_do_not_fill_its_shape_is_not_encoded() {
    let mut saved = fixture_v1();
    saved.first_moments[0].values.pop();
    assert!(encode_v1(&saved)
        .unwrap_err()
        .contains("differs from its shape"));
}
