//! Genuine paid inputs; explicit wide-counter/storage operands are primitives.
use super::*;
use crate::{rl::PlayerSeatV1, tinybones_public_v72 as facts};
fn capture(s: &GameState, p: PlayerId) -> facts::FactsV72 {
    let before = s.clone();
    let f = facts::capture(s, p).unwrap();
    assert_eq!(*s, before);
    facts::validate_current(s, p, &f).unwrap();
    let restored: facts::FactsV72 =
        serde_json::from_slice(&serde_json::to_vec(&f).unwrap()).unwrap();
    assert_eq!(restored, f);
    f
}
#[test]
fn paid_discard_held_publication_stash_original_binding_and_restore_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, tiny, card) = setup(p, "Think Twice");
        activate_to_discard(&mut s, p, tiny, card);
        act(&mut s, Action::Discard(vec![card]));
        for viewer in [PlayerId::P0, PlayerId::P1] {
            let f = capture(&s, viewer);
            assert_eq!(f.discards.len(), 1);
            assert!(f.triggers.iter().any(|t| t.stage == facts::StageV72::Held));
            assert_eq!(f.discards[0].original_card.object, card.0);
            assert_eq!(f.discards[0].discarder, PlayerSeatV1::from(p.opponent()));
        }
        let mut published = false;
        for _ in 0..32 {
            let d = next(&mut s);
            if s.stack
                .iter()
                .any(|r| matches!(r.inline_effect, Some(EffectOp::TinybonesStashV72 { .. })))
            {
                published = true;
                break;
            }
            pass(&mut s, d);
        }
        assert!(published);
        let f = capture(&s, p);
        assert!(f
            .triggers
            .iter()
            .any(|t| t.stage == facts::StageV72::Published));
        finish(&mut s);
        let f = capture(&s, p);
        assert_eq!(f.stash.len(), 1);
        assert_eq!(f.stash[0].card.object, card.0);
        assert!(f.stash[0].can_play[p.index()]);
        assert!(f.stash[0].any_type_mana[p.index()]);
        assert_eq!(f.triggers[0].stage, facts::StageV72::Resolved);
        assert_eq!(f.triggers[0].stashed.as_ref().unwrap().object, card.0);
        let mut stale = f.clone();
        stale.stash[0].card.generation += 1;
        let before = s.clone();
        assert!(facts::validate_current(&s, p, &stale).is_err());
        assert_eq!(s, before);
        let mut erased = f.clone();
        erased.triggers.clear();
        assert!(facts::validate_current(&s, p, &erased).is_err());
        let mut raw = serde_json::to_value(&f).unwrap();
        raw["triggers"][0]["source"]["original_source"]["identity"]["Known"]["unknown_alias"] =
            serde_json::json!(true);
        assert!(serde_json::from_value::<facts::FactsV72>(raw).is_err());
        let restored = restore(&s);
        assert_eq!(capture(&restored, p), f);
    }
}
#[test]
fn independent_prepaid_unknown_library_and_hand_worlds_do_not_enter_v72_facts_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut all = vec![];
        for hidden in ["Forest", "Imprisoned in the Moon"] {
            let mut s = ready(p, hidden);
            let tiny = put(&mut s, p, NAME, Zone::Hand);
            let card = put(&mut s, p.opponent(), "Think Twice", Zone::Hand);
            paid(&mut s, p, tiny);
            turn(&mut s, p);
            stashed(&mut s, p, tiny, card);
            assert!(s.players[p.opponent().index()].hand.iter().any(|c| s
                .objects
                .get(*c)
                .card_def
                == id(hidden)));
            all.push([capture(&s, p), capture(&s, p.opponent())]);
        }
        assert_eq!(all[0], all[1]);
        let raw = serde_json::to_value(&all[0][0]).unwrap();
        for private in [
            "event_history",
            "original_library",
            "remaining_library",
            "world",
            "move_history_index",
            "play_history_index",
            "stack_item_id",
        ] {
            assert!(!serde_json::to_string(&raw).unwrap().contains(private));
        }
    }
}
#[test]
fn paid_source_full_eighteen_counter_storage_and_supplied_value_freshness_both_seats() {
    for p in [PlayerId::P0, PlayerId::P1] {
        let (mut s, tiny, _) = setup(p, "Think Twice");
        // Explicit native wide-counter input primitive on genuinely paid history.
        let c = &mut s.objects.get_mut(tiny).counters;
        c.charge = i32::MAX;
        c.fellowship = i32::MAX;
        c.stash = i32::MAX;
        c.bait = i32::MAX;
        c.net = i32::MAX;
        let f = capture(&s, p);
        let current = f.battlefield[p.index()]
            .iter()
            .find(|c| c.card.object == tiny.0)
            .unwrap();
        assert_eq!(current.counters.stash, i32::MAX);
        assert_eq!(current.counters.fellowship, i32::MAX);
        assert_eq!(current.counters.charge, i32::MAX);
        let value = serde_json::to_value(&current.counters).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 18);
        assert!(crate::valkyries_call_public_v62::capture(&s, p).is_err());
        let mut changed = f.clone();
        changed.battlefield[p.index()]
            .iter_mut()
            .find(|c| c.card.object == tiny.0)
            .unwrap()
            .counters
            .stash -= 1;
        let before = s.clone();
        assert!(facts::validate_current(&s, p, &changed).is_err());
        assert_eq!(s, before);
        let mut meaning = f.meaning.clone();
        meaning.ordered_rules.swap(0, 1);
        assert!(crate::rules_meaning_tinybones_v72::validate(&meaning).is_err());
    }
}
#[test]
fn genuine_foreign_paid_play_and_real_suppression_keep_printed_cost_and_current_grants_both_seats()
{
    for p in [PlayerId::P0, PlayerId::P1] {
        let mut s = ready(p, "Forest");
        let tiny = put(&mut s, p, NAME, Zone::Hand);
        let card = put(&mut s, p.opponent(), "Lightning Bolt", Zone::Hand);
        let witness = put(&mut s, p.opponent(), "Witness Protection", Zone::Hand);
        paid(&mut s, p, tiny);
        turn(&mut s, p);
        stashed(&mut s, p, tiny, card);
        let before = tapped(&s, p);
        let row = cast(&mut s, p, card, Some(Target::Player(p.opponent())));
        assert_eq!(tapped(&s, p) - before, 1);
        assert_eq!(row.v4.source_contract.unwrap().owner, p.opponent());
        let f = capture(&s, p);
        assert_eq!(f.plays.len(), 1);
        assert_eq!(f.plays[0].stage, facts::PlayStageV72::Paid);
        assert_eq!(f.plays[0].paid_form, Some(CastMethodV4::Normal));
        assert_eq!(f.plays[0].actor, PlayerSeatV1::from(p));
        assert_ne!(f.plays[0].original_card.owner, f.plays[0].actor);
        finish(&mut s);
        turn(&mut s, p.opponent());
        cast(&mut s, p.opponent(), witness, Some(Target::Object(tiny)));
        finish(&mut s);
        let f = capture(&s, p);
        let current = f.battlefield[p.index()]
            .iter()
            .find(|c| c.card.object == tiny.0)
            .unwrap();
        assert!(!current.form.printed_abilities_active);
        assert!(f.triggers.iter().all(|t| matches!(
            t.source.original_source.identity,
            crate::optional_trigger_public_v1::IdentityV1::Known { card_id: 490 }
        )));
        assert!(f
            .plays
            .iter()
            .all(|play| play.permission_source.captured_controller == PlayerSeatV1::from(p)));
        assert_eq!(capture(&restore(&s), p), f);
    }
}
