use super::*;
use crate::{effect::vivien_reid_v1::tests as native, ids::PlayerId};
#[test]
fn genuine_paid_vivien_emblem_full_counters_current_source_and_tamper_refuse_readonly_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = native::ready(p, &["Forest", "Island"], &["Vivien Reid"], "Mountain");
        let v = native::walker(&mut s, p);
        native::emblem_setup(&mut s, p, v);
        let original = s.clone();
        let facts = crate::vivien_public_v70::capture(&s, p).unwrap();
        assert_eq!(facts.emblems.len(), 1);
        assert_eq!(
            facts.activations[0].stage,
            crate::vivien_public_v70::StageV70::EmblemCreated
        );
        assert!(facts.activations[0]
            .source
            .current_original_source
            .is_none());
        assert_eq!(s, original);
        let mut bad = facts.clone();
        bad.emblems.clear();
        assert!(crate::vivien_public_v70::validate(&bad).is_err());
        let restored: GameState = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!(
            crate::vivien_public_v70::capture(&restored, p).unwrap(),
            facts
        );
        validate_state(&restored).unwrap();
    }
}
