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

#[test]
fn paid_final_forms_preserve_enchantment_bestow_and_creature_adventure_both_seats() {
    use crate::state::Target;
    for p in [PlayerId::P0, PlayerId::P1] {
        for (name, kind, form, x, cost) in [
            ("Fang Dragon", CardType::Creature, 0, 0, 7),
            ("Sagu Wildling", CardType::Creature, 0, 0, 5),
            ("Nyxborn Hydra", CardType::Enchantment, 1, 3, 5),
        ] {
            let mut s = native::ready(p, "Island");
            let source = native::paid(&mut s, p);
            let host = put(&mut s, p, "Llanowar Elves", Zone::Hand);
            native::pay(&mut s, p, host, None, None);
            native::finish(&mut s);
            let card = put(&mut s, p, name, Zone::Graveyard);
            let before = native::tapped(&s, p);
            let row = native::pay_form(
                &mut s,
                p,
                card,
                Some((source, kind)),
                Some(Target::Object(host)),
                form,
                x,
            );
            assert_eq!(native::tapped(&s, p) - before, cost);
            assert_eq!(
                row.v4.cast_method,
                Some(if form == 1 {
                    crate::state::CastMethodV4::Bestow
                } else {
                    crate::state::CastMethodV4::Normal
                })
            );
            let facts = capture(&s, p).unwrap();
            assert_eq!(facts.budgets[0].spent, [kind]);
            let play = &facts.plays[0];
            let index = usize::from(play.selected_menu_index.unwrap())
                - usize::from(play.ordinary_available);
            assert_eq!(play.permissions[index].permanent_type, kind);
            assert_eq!(play.stage, StageV71::CastPublished);
            let restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
            assert_eq!(capture(&restored, p).unwrap(), facts);
            let mut bad = facts.clone();
            let duplicate = bad.plays[0].permissions[0].clone();
            bad.plays[0].permissions.push(duplicate);
            assert!(validate(&bad).is_err());
            let mut bad = facts.clone();
            bad.budgets[0].unused.reverse();
            assert!(validate(&bad).is_err());
            let mut value = serde_json::to_value(ActionPermissionV71 {
                occurrence: 0,
                menu_index: 0,
                permission: None,
            })
            .unwrap();
            value.as_object_mut().unwrap().remove("permission");
            assert!(serde_json::from_value::<ActionPermissionV71>(value).is_err());
        }
    }
}
