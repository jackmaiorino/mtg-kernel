use super::*;

#[test]
fn catalog_and_exact_selector_refuse_before_foreign_or_missing_definition() {
    let selector = SelectorV1 {
        schema: SCHEMA.into(),
        scope: SCOPE.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256_v1(),
    };
    let before = serde_json::to_vec(&selector).unwrap();
    let bolt = crate::card_def::card_id_by_name("Lightning Bolt").unwrap();
    if live_selector_v1().is_ok() {
        assert_eq!(
            meaning_v1(&selector, bolt),
            Err(ErrorV1::UnsupportedDefinition)
        );
        assert_eq!(
            meaning_v1(&selector, u16::MAX),
            Err(ErrorV1::UnsupportedDefinition)
        );
        for field in 0..4 {
            let mut forged = selector.clone();
            match field {
                0 => forged.schema.push('x'),
                1 => forged.scope.push('x'),
                2 => forged.card_db_hash ^= 1,
                _ => forged.producer_sha256.push('x'),
            }
            assert_eq!(meaning_v1(&forged, bolt), Err(ErrorV1::WrongSelector));
        }
    } else {
        assert_eq!(live_selector_v1(), Err(ErrorV1::UnsupportedCatalog));
        assert_eq!(
            meaning_v1(&selector, bolt),
            Err(ErrorV1::UnsupportedCatalog)
        );
        let mut forged = selector.clone();
        forged.schema.clear();
        assert_eq!(
            meaning_v1(&forged, u16::MAX),
            Err(ErrorV1::UnsupportedCatalog)
        );
    }
    assert_eq!(serde_json::to_vec(&selector).unwrap(), before);
}
#[test]
fn selector_roundtrip_rejects_unknown_fields_without_catalog_claim() {
    let selector = SelectorV1 {
        schema: SCHEMA.into(),
        scope: SCOPE.into(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        producer_sha256: producer_sha256_v1(),
    };
    let mut value = serde_json::to_value(&selector).unwrap();
    assert_eq!(
        serde_json::from_value::<SelectorV1>(value.clone()).unwrap(),
        selector
    );
    value
        .as_object_mut()
        .unwrap()
        .insert("invented_admission".into(), true.into());
    assert!(serde_json::from_value::<SelectorV1>(value).is_err());
}
