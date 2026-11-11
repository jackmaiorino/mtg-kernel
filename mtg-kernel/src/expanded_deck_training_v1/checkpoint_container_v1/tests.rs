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

/// A small checkpoint for the Python reader's fixture: the net-8 fixture's
/// document with three small tensors per section.
fn python_fixture_v1() -> ExpandedCheckpointV1 {
    let mut saved = fixture_v1();
    let tensors = |seed: f32| -> Vec<ParameterBitsV1> {
        [
            ("encoder.w", vec![2, 3]),
            ("head.b", vec![5]),
            ("empty", vec![0, 4]),
        ]
        .into_iter()
        .map(|(name, shape)| {
            let count: usize = shape.iter().product();
            ParameterBitsV1 {
                name: name.into(),
                shape,
                values: (0..count)
                    .map(|i| (seed + i as f32 * 0.37 - 1.0).to_bits())
                    .collect(),
            }
        })
        .collect()
    };
    saved.parameters = tensors(0.0);
    saved.first_moments = tensors(0.25);
    saved.second_moments = tensors(0.5);
    saved
}

/// `python/tools/tests/fixtures/checkpoint_container_v1/` holds a small v1
/// checkpoint and the container the converter makes from it; the Python
/// reader's tests decode one and compare it with the other. Set
/// `MTG_WRITE_CONTAINER_FIXTURE=1` to rewrite them.
#[test]
fn the_python_reader_fixture_is_the_converters_output() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../python/tools/tests/fixtures/checkpoint_container_v1");
    let v1 = json_v1(&python_fixture_v1());
    let container = convert_v1_bytes(&v1).unwrap();
    if std::env::var_os("MTG_WRITE_CONTAINER_FIXTURE").is_some() {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("checkpoint-v1.json"), &v1).unwrap();
        std::fs::write(dir.join("checkpoint.ckpt"), &container).unwrap();
    }
    assert_eq!(std::fs::read(dir.join("checkpoint-v1.json")).unwrap(), v1);
    assert_eq!(
        std::fs::read(dir.join("checkpoint.ckpt")).unwrap(),
        container
    );
    assert_eq!(export_v1(&container).unwrap(), v1);
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

/// `tensors` as a snapshot holds them: the `f32` values whose bits they are,
/// under the same names and shapes.
fn native_v1(tensors: &[ParameterBitsV1]) -> Vec<NativeNamedParameterV1> {
    tensors
        .iter()
        .map(|tensor| NativeNamedParameterV1 {
            name: Box::leak(tensor.name.clone().into_boxed_str()),
            shape: tensor.shape.clone(),
            values: tensor
                .values
                .iter()
                .map(|&bits| f32::from_bits(bits))
                .collect(),
        })
        .collect()
}

/// `saved`'s three sections as snapshot tensors.
fn native_sections_v1(saved: &ExpandedCheckpointV1) -> [Vec<NativeNamedParameterV1>; 3] {
    sections_of_v1(saved).map(native_v1)
}

/// `saved` with its tensors borrowed from `native`, as an update publishes it.
fn borrowed_v1<'a>(
    saved: &ExpandedCheckpointV1,
    native: &'a [Vec<NativeNamedParameterV1>; 3],
) -> ExpandedCheckpointV1<ParameterBitsRefV1<'a>> {
    with_tensors_v1(
        saved,
        native.each_ref().map(|tensors| {
            tensors
                .iter()
                .map(ParameterBitsRefV1::of_native_v1)
                .collect()
        }),
    )
}

/// The net-8 fixture with every optional member a checkpoint document can
/// carry as JSON, so the derived serialization's member presence is shared.
fn optional_members_fixture_v1() -> ExpandedCheckpointV1 {
    let mut saved = fixture_v1();
    saved.e_supported = Some(serde_json::json!({"continuation": "e", "packet": null}));
    saved.soft_q = Some(serde_json::json!({"continuation": "sq", "packet": {"path": "/p"}}));
    saved.search_targets = Some(serde_json::json!({"continuation": "st", "census": [1, 2]}));
    saved.ex1 = Some(serde_json::json!({"continuation": "ex1", "w5_next_update": 3}));
    saved
}

#[test]
fn a_checkpoint_of_borrowed_tensors_publishes_the_owned_bytes_in_both_formats() {
    for owned in [
        fixture_v1(),
        python_fixture_v1(),
        optional_members_fixture_v1(),
    ] {
        let native = native_sections_v1(&owned);
        let borrowed = borrowed_v1(&owned, &native);
        let v1 = json_v1(&owned);
        assert_eq!(serde_json::to_vec(&borrowed).unwrap(), v1);
        let container = encode_v1(&owned).unwrap();
        assert_eq!(encode_v1(&borrowed).unwrap(), container);
        assert_eq!(json_v1(&decode_v1(&container).unwrap()), v1);
        assert!(encodes_to_v1(&borrowed, &container).unwrap());
        // The writer's gate and the default's resolution read the document,
        // however the tensors are held.
        assert_eq!(
            admit_writer_v1(&borrowed).is_ok(),
            admit_writer_v1(&owned).is_ok()
        );
        assert_eq!(
            CheckpointRecordFormatV1::Auto.resolve_v1(&borrowed),
            CheckpointRecordFormatV1::Auto.resolve_v1(&owned)
        );
        for (native, owned) in native.iter().zip(sections_of_v1(&owned)) {
            assert!(same_tensors_v1(native, owned));
        }
    }
    // The checked-in V1/V2 pair, written from borrowed tensors.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../python/tools/tests/fixtures/checkpoint_container_v1");
    let owned = python_fixture_v1();
    let native = native_sections_v1(&owned);
    let borrowed = borrowed_v1(&owned, &native);
    assert_eq!(
        serde_json::to_vec(&borrowed).unwrap(),
        std::fs::read(dir.join("checkpoint-v1.json")).unwrap()
    );
    assert_eq!(
        encode_v1(&borrowed).unwrap(),
        std::fs::read(dir.join("checkpoint.ckpt")).unwrap()
    );
}

#[test]
fn a_borrowed_tensor_that_does_not_fill_its_shape_is_not_encoded() {
    let owned = python_fixture_v1();
    let mut native = native_sections_v1(&owned);
    native[1][0].values.pop();
    assert!(encode_v1(&borrowed_v1(&owned, &native))
        .unwrap_err()
        .contains("differs from its shape"));
}

/// The decode with the second guard as it was before it was streamed: a
/// re-encoded copy compared with the bytes.
fn decode_with_copy_v1(bytes: &[u8]) -> Result<ExpandedCheckpointV1, String> {
    let saved = decode_sections_v1(bytes)?;
    if encode_v1(&saved)? != bytes {
        return Err("checkpoint container is not in canonical form".into());
    }
    Ok(saved)
}

#[test]
fn the_streamed_canonical_check_refuses_exactly_what_a_reencoded_copy_refuses() {
    let container = encode_v1(&python_fixture_v1()).unwrap();
    let outcome = |bytes: &[u8]| decode_v1(bytes).map(|saved| json_v1(&saved));
    let reference = |bytes: &[u8]| decode_with_copy_v1(bytes).map(|saved| json_v1(&saved));
    let mut accepted = 0usize;
    let mut refused = 0usize;
    let mut check = |bytes: &[u8], what: &str| {
        let got = outcome(bytes);
        assert_eq!(got, reference(bytes), "{what}");
        match got {
            Ok(_) => accepted += 1,
            Err(_) => refused += 1,
        }
    };
    check(&container, "the container itself");
    // Every byte changed three ways: header, document, table, padding and
    // values alike.
    let changes: [fn(u8) -> u8; 3] = [|b| b ^ 0x01, |b| b ^ 0x80, |_| 0x20];
    for at in 0..container.len() {
        for change in changes {
            let mut bytes = container.clone();
            bytes[at] = change(bytes[at]);
            if bytes[at] != container[at] {
                check(&bytes, &format!("byte {at} changed"));
            }
        }
    }
    // Every truncation, and trailing bytes.
    for length in 0..container.len() {
        check(&container[..length], &format!("truncated to {length}"));
    }
    let tails: [&[u8]; 4] = [&[0], &[1], &[0; 4], &[0; 256]];
    for tail in tails {
        check(&[container.as_slice(), tail].concat(), "trailing bytes");
    }
    // Value bytes are data, so some changes decode; everything else is
    // refused.
    assert!(accepted > 1 && refused > container.len());
}

#[test]
fn tensors_compare_by_name_shape_and_bits_in_place() {
    let owned = python_fixture_v1().parameters;
    let native = native_v1(&owned);
    assert!(same_tensors_v1(&native, &owned) && same_tensors_v1(&owned, &native));
    assert!(same_tensors_v1(&native, &native) && same_tensors_v1(&owned, &owned));
    // A change of each kind is seen from either side and against either form.
    let differs = |change: &dyn Fn(&mut Vec<NativeNamedParameterV1>)| {
        let mut changed = native_v1(&owned);
        change(&mut changed);
        !same_tensors_v1(&changed, &owned)
            && !same_tensors_v1(&owned, &changed)
            && !same_tensors_v1(&changed, &native)
            && !same_tensors_v1(&native, &changed)
    };
    assert!(differs(&|t| t[0].name = "renamed"));
    assert!(differs(&|t| t[0].shape = vec![3, 2]));
    assert!(differs(
        &|t| t[1].values[4] = f32::from_bits(t[1].values[4].to_bits() ^ 1)
    ));
    assert!(differs(&|t| t[0].values.push(0.0)));
    assert!(differs(&|t| {
        t.pop();
    }));
    // Bits, not float equality: a signed zero or a NaN payload counts, and a
    // NaN equals itself.
    let one = |value: f32| {
        vec![NativeNamedParameterV1 {
            name: "x",
            shape: vec![1],
            values: vec![value],
        }]
    };
    assert!(!same_tensors_v1(&one(0.0), &one(-0.0)));
    let nan = |payload: u32| one(f32::from_bits(0x7fc0_0000 | payload));
    assert!(same_tensors_v1(&nan(1), &nan(1)) && !same_tensors_v1(&nan(1), &nan(2)));
    // Past the first block of the comparison: one bit deep in a large tensor.
    let large = fixture_v1().parameters;
    let mut changed = native_v1(&large);
    let tensor = changed
        .iter_mut()
        .max_by_key(|t| t.values.len())
        .expect("a tensor");
    assert!(tensor.values.len() > 5000);
    tensor.values[5000] = f32::from_bits(tensor.values[5000].to_bits() ^ 0x8000_0000);
    assert!(same_tensors_v1(&native_v1(&large), &large));
    assert!(!same_tensors_v1(&changed, &large));
}
