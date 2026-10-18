use super::*;

fn rules(arm: ArchiveDistillationRepresentationArmV1) -> ArchiveDistillationRepresentationV1 {
    let (_, _, _, _, sha) = true_rows_v1().as_ref().unwrap();
    ArchiveDistillationRepresentationV1 {
        arm,
        rules_rows_sha256: Some(sha.clone()),
    }
}

#[test]
fn the_cell_names_a_table_exactly_for_the_rules_arms() {
    use ArchiveDistillationRepresentationArmV1::*;
    let sha = Some("a".repeat(64));
    for (arm, table, ok) in [
        (Rules, sha.clone(), true),
        (WrongRules, sha.clone(), true),
        (FreshStateCodes, None, true),
        (Rules, None, false),
        (FreshStateCodes, sha.clone(), false),
        (Rules, Some("A".repeat(64)), false),
        (Rules, Some("a".repeat(63)), false),
    ] {
        let r = ArchiveDistillationRepresentationV1 {
            arm,
            rules_rows_sha256: table,
        };
        assert_eq!(validate_representation_v1(&r).is_ok(), ok, "{r:?}");
    }
}

#[test]
fn rules_rows_are_the_normalized_table_and_refuse_another_table() {
    let rows = rules_rows_v1(&rules(ArchiveDistillationRepresentationArmV1::Rules)).unwrap();
    assert_eq!(rows.cards, crate::card_def::CARD_DEFS.len());
    assert_eq!(rows.rows.len(), rows.cards * rows.width);
    for c in 0..rows.cards {
        let norm: f64 = rows.rows[c * rows.width..(c + 1) * rows.width]
            .iter()
            .map(|&v| f64::from(v) * f64::from(v))
            .sum();
        assert!(norm == 0.0 || (norm - 1.0).abs() < 1e-5, "card {c}: {norm}");
        assert!(rows.rows[c * rows.width..(c + 1) * rows.width]
            .iter()
            .all(|&v| v >= 0.0));
    }
    assert!(rows.salt.is_none() && rows.mapping.is_none());
    let mut other = rules(ArchiveDistillationRepresentationArmV1::Rules);
    other.rules_rows_sha256 = Some("0".repeat(64));
    assert!(rules_rows_v1(&other).is_err());
}

#[test]
fn the_wrong_association_moves_every_card_to_another_row() {
    let truth = rules_rows_v1(&rules(ArchiveDistillationRepresentationArmV1::Rules)).unwrap();
    let wrong =
        rules_rows_v1(&rules(ArchiveDistillationRepresentationArmV1::WrongRules)).unwrap();
    let mapping = wrong.mapping.clone().unwrap();
    let w = truth.width;
    let mut targets: Vec<u16> = mapping.clone();
    targets.sort_unstable();
    assert_eq!(targets, (0..truth.cards as u16).collect::<Vec<_>>(), "a bijection");
    for c in 0..truth.cards {
        assert_ne!(usize::from(mapping[c]), c);
        assert_eq!(
            bits(&wrong.rows[c * w..(c + 1) * w]),
            bits(&truth.rows[usize::from(mapping[c]) * w..(usize::from(mapping[c]) + 1) * w])
        );
        assert_ne!(
            bits(&wrong.rows[c * w..(c + 1) * w]),
            bits(&truth.rows[c * w..(c + 1) * w])
        );
    }
    // Deterministic.
    let again =
        rules_rows_v1(&rules(ArchiveDistillationRepresentationArmV1::WrongRules)).unwrap();
    assert_eq!(again.rows_sha256, wrong.rows_sha256);
    assert_eq!(again.salt, wrong.salt);
}

#[test]
fn fresh_codes_replace_only_the_state_hash_on_a_copy() {
    let mut tensor = NativeFlatDecisionTensorV2 {
        state: (0..NATIVE_FLAT_STATE_FEATURE_DIM_V2)
            .map(|i| i as f32 / 1000.0)
            .collect(),
        action_features: vec![0.25; 8],
        ..Default::default()
    };
    tensor.object_card_ids = vec![0, 3];
    let original = tensor.clone();
    let a = fresh_state_codes_v1(&tensor, "SD1-T-s0", "g1", 7, 1).unwrap();
    let b = fresh_state_codes_v1(&tensor, "SD1-T-s0", "g1", 7, 1).unwrap();
    let c = fresh_state_codes_v1(&tensor, "SD1-T-s0", "g1", 7, 2).unwrap();
    let d = fresh_state_codes_v1(&tensor, "SD1-T-s1", "g1", 7, 1).unwrap();
    assert_eq!(tensor, original);
    assert_eq!(a, b);
    assert_ne!(a.state, c.state);
    assert_ne!(a.state, d.state);
    let begin = NATIVE_FLAT_STATE_FEATURE_DIM_V2 - NATIVE_FLAT_ACTION_HASH_FEATURE_DIM_V2;
    assert_eq!(a.state[..begin], tensor.state[..begin]);
    assert!(a.state[begin..].iter().all(|v| (-1.0..=1.0).contains(v)));
    assert!(a.state[begin..]
        .iter()
        .zip(&tensor.state[begin..])
        .any(|(x, y)| x != y));
    assert_eq!(a.action_features, tensor.action_features);
    assert_eq!(a.object_card_ids, tensor.object_card_ids);
    let short = NativeFlatDecisionTensorV2 {
        state: vec![0.0; 10],
        ..Default::default()
    };
    assert!(fresh_state_codes_v1(&short, "x", "g", 0, 1).is_err());
}

#[test]
fn coverage_refuses_a_card_past_the_table() {
    let ok = NativeFlatDecisionTensorV2 {
        object_card_ids: vec![0, 1, 5],
        ..Default::default()
    };
    let past = NativeFlatDecisionTensorV2 {
        object_card_ids: vec![6],
        ..Default::default()
    };
    let report = require_covered_v1(5, [&ok].into_iter()).unwrap();
    assert_eq!(report["distinct_card_tokens"], 2);
    assert!(require_covered_v1(5, [&ok, &past].into_iter()).is_err());
}

#[test]
fn a_rules_checkpoint_table_must_be_its_rows_plus_the_pathway() {
    use crate::native_policy_value_net_v1::{
        NativePolicyValueModelConfigV1, NativePolicyValueNetV1,
    };
    let representation = rules(ArchiveDistillationRepresentationArmV1::WrongRules);
    let rows = rules_rows_v1(&representation).unwrap();
    let model =
        NativePolicyValueNetV1::runner_fixed_v1(NativePolicyValueModelConfigV1::contract_v1())
            .unwrap();
    let base = model.parameter_snapshot_v1();
    let mut pathway = RulesPathwayV1::new_v1(rows.rows.clone(), rows.cards, rows.width).unwrap();
    let mut state = pathway.state_bits_v1();
    for (i, w) in state.weight.iter_mut().enumerate() {
        *w = (((i % 17) as f32 - 8.0) * 1e-3).to_bits();
    }
    pathway.restore_v1(&state).unwrap();
    let effective = pathway.effective_parameters_v1(&base).unwrap();
    let table = bits(&effective[0].values);
    let sidecar = sidecar_v1(representation.arm, &rows, 8, &base, &pathway);
    validate_rules_checkpoint_v1(&representation, &sidecar, 8, &table).unwrap();
    // Another step, another arm, one changed bit, or a changed weight is refused.
    assert!(validate_rules_checkpoint_v1(&representation, &sidecar, 16, &table).is_err());
    assert!(validate_rules_checkpoint_v1(
        &rules(ArchiveDistillationRepresentationArmV1::Rules),
        &sidecar,
        8,
        &table
    )
    .is_err());
    let mut tampered = table.clone();
    tampered[CARD_EMBEDDING_DIM_V1 + 3] ^= 1;
    assert!(validate_rules_checkpoint_v1(&representation, &sidecar, 8, &tampered).is_err());
    let mut moved = sidecar.clone();
    moved.weight[0] ^= 1;
    assert!(validate_rules_checkpoint_v1(&representation, &moved, 8, &table).is_err());
    // The displacement summary reads every card.
    let d = displacement_v1(&pathway, &base, &base);
    assert_eq!(d["cards"], rows.cards);
    assert_eq!(d["own_row_moved_norm"]["max"], 0.0);
    assert!(d["pathway_added_norm"]["max"].as_f64().unwrap() > 0.0);
}
