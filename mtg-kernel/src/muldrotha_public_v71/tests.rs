use super::*;
use crate::{
    engine::muldrotha_v1::tests as native, policy_observation_v6::tests::put, state::Zone,
};
#[test]
fn paid_public_history_budget_source_reentry_and_current_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = native::ready(p, "Island");
        let source = native::paid(&mut s, p);
        let card = put(&mut s, p, "Llanowar Elves", Zone::Graveyard);
        native::pay(&mut s, p, card, Some((source, CardType::Creature)), None);
        let f = capture(&s, p).unwrap();
        assert_eq!(f.budgets[0].spent, [CardType::Creature]);
        assert_eq!(f.plays[0].selected_menu_index, Some(0));
        let old = f.plays[0].permissions[0].source.original_source.generation;
        // Native source departure/reentry primitives preserve the paid spell's
        // original source while giving the new permanent a fresh type budget.
        crate::event::propose_and_commit(
            &mut s,
            crate::event::ProposedEvent::zone_change(source, Zone::Hand),
        );
        crate::event::propose_and_commit(
            &mut s,
            crate::event::ProposedEvent::zone_change(source, Zone::Battlefield),
        );
        let f = capture(&s, p).unwrap();
        assert!(f.budgets[0].spent.is_empty());
        assert_eq!(f.budgets[0].unused, TYPES);
        assert!(f.budgets[0].source.original_source.generation > old);
        assert!(f.plays[0].permissions[0]
            .source
            .current_original_source
            .is_none());
        let restored: GameState = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!(capture(&restored, p).unwrap(), f);
        let mut bad = f.clone();
        bad.budgets[0].unused.clear();
        assert!(validate(&bad).is_err());
        let mut bad = f.clone();
        bad.plays.clear();
        assert!(validate_current(&s, p, &bad).is_err());
        let before = s.clone();
        assert_eq!(capture(&s, p).unwrap(), f);
        assert_eq!(s, before);
    }
}
