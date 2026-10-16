//! Actual W and W+2W payments through all four sealed owners. Lands, libraries
//! and recipients are setup operands. The existing 260-target paid history is
//! unchanged; these paths add ordinary, zero-target and multi-target coverage.
use super::{
    entry_attack::{choice_guards, tap},
    *,
};
use crate::{card_def::Keywords, mana::ManaColor, state::FaceDownV1};
fn choose<O: Owner>(s: &mut O, predicate: impl Fn(&ActionSemanticV1) -> bool) {
    let index = s.actions().iter().position(&predicate).unwrap();
    choice_guards(s, index);
    step(s, predicate);
}
fn is_target(action: &ActionSemanticV1, id: ObjectId) -> bool {
    matches!(action,ActionSemanticV1::ChooseDivineTargetV44 {target,..} if target.arena_id==id.0)
}
fn run<O: Owner>(actor: PlayerId, kicked: bool, count: usize) {
    let mut raw = crate::extra_turn_v1::tests::ready(actor);
    let spell = put(&mut raw, actor, "Divine Resilience", Zone::Hand);
    let recipients = [
        put(&mut raw, actor, "Llanowar Elves", Zone::Battlefield),
        put(&mut raw, actor, "Llanowar Elves", Zone::Battlefield),
    ];
    let opposing = put(
        &mut raw,
        actor.opponent(),
        "Llanowar Elves",
        Zone::Battlefield,
    );
    let lands: Vec<_> = (0..4)
        .map(|_| put(&mut raw, actor, "Plains", Zone::Battlefield))
        .collect();
    put(&mut raw, actor.opponent(), "Forest", Zone::Battlefield);
    let mut s = O::own(raw);
    assert_eq!(
        fact_count(
            O::packet(&s.encode()).state(),
            "registered_instant_printing"
        ),
        1
    );
    for land in &lands {
        tap(&mut s, *land);
    }
    cast(&mut s, spell);
    choose(
        &mut s,
        |a| matches!(a,ActionSemanticV1::ChooseDivineKickerV44{pay,..} if *pay==kicked),
    );
    assert!(!s.actions().iter().any(|a| is_target(a, opposing)));
    assert_eq!(
        s.actions()
            .iter()
            .any(|a| matches!(a, ActionSemanticV1::FinishDivineTargetsV44 { .. })),
        kicked
    );
    let pending = s.snapshot();
    let pending_world = gameplay(&s);
    let old = s.encode();
    let mut foreign = O::own(s.world().clone());
    let before = gameplay(&foreign);
    assert!(matches!(
        foreign.restore(&pending),
        Err(Error::ForeignSnapshot)
    ));
    assert_eq!(gameplay(&foreign), before);
    let mut result = None;
    for replay in 0..2 {
        if replay != 0 {
            s.restore(&pending).unwrap();
            assert_eq!(gameplay(&s), pending_world);
            let before = gameplay(&s);
            assert!(matches!(s.apply(&old, 0), Err(Error::StaleBinding)));
            assert_eq!(gameplay(&s), before);
            assert_eq!(O::packet(&s.encode()), O::packet(&old));
        }
        for recipient in &recipients[..count] {
            choose(&mut s, |a| is_target(a, *recipient));
            assert!(!s.actions().iter().any(|a| is_target(a, *recipient)));
        }
        if kicked {
            choose(&mut s, |a| {
                matches!(a, ActionSemanticV1::FinishDivineTargetsV44 { .. })
            });
        }
        assert!(s.world().engine.pending_cast.is_none());
        assert_eq!(s.world().objects.get(spell).zone, Zone::Stack);
        let item = s.world().stack.iter().find(|i| i.source == spell).unwrap();
        assert_eq!(item.kicked, kicked);
        assert_eq!(item.controller, actor);
        assert_eq!(item.targets.len(), count);
        assert_eq!(item.v4.target_contracts.len(), count);
        assert_eq!(item.v4.source_contract.unwrap().owner, actor);
        let stack_id = item.v4.stack_item_id;
        assert!(lands.iter().all(|id| s.world().objects.get(*id).tapped));
        let mut remaining = [0; 6];
        remaining[ManaColor::W.pool_index()] = if kicked { 0 } else { 3 };
        assert_eq!(s.world().players[actor.index()].mana_pool, remaining);
        if replay == 0 {
            // Exact stack counter primitive on the genuine paid occurrence.
            let mut countered = s.world().clone();
            crate::engine::counter_stack_item_by_id(&mut countered, stack_id).unwrap();
            let countered = O::own(countered);
            assert_eq!(countered.world().objects.get(spell).zone, Zone::Graveyard);
            for recipient in recipients {
                assert!(!crate::engine::has_effective_keyword(
                    countered.world(),
                    recipient,
                    Keywords::INDESTRUCTIBLE
                ));
            }
            countered.encode();
        }
        let paid = s.snapshot();
        let paid_world = gameplay(&s);
        drain(&mut s);
        assert_eq!(s.world().objects.get(spell).zone, Zone::Graveyard);
        for (index, recipient) in recipients.into_iter().enumerate() {
            assert_eq!(
                crate::engine::has_effective_keyword(
                    s.world(),
                    recipient,
                    Keywords::INDESTRUCTIBLE
                ),
                index < count
            );
        }
        assert!(!crate::engine::has_effective_keyword(
            s.world(),
            opposing,
            Keywords::INDESTRUCTIBLE
        ));
        let after = (gameplay(&s), O::packet(&s.encode()).clone());
        s.restore(&paid).unwrap();
        assert_eq!(gameplay(&s), paid_world);
        drain(&mut s);
        assert_eq!((gameplay(&s), O::packet(&s.encode()).clone()), after);
        if let Some(old) = &result {
            assert_eq!(&after, old);
        } else {
            result = Some(after);
        }
    }
}
pub(super) fn paid<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for (kicked, count) in [(false, 1), (true, 0), (true, 2)] {
            run::<O>(actor, kicked, count);
        }
    }
}
pub(super) fn privacy<O: Owner>() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        for known in [false, true] {
            let build = |hidden: &str| {
                let mut raw = crate::extra_turn_v1::tests::ready(actor);
                if known {
                    put(&mut raw, actor, "Divine Resilience", Zone::Hand);
                }
                put(&mut raw, actor.opponent(), hidden, Zone::Hand);
                let face = put(&mut raw, actor.opponent(), hidden, Zone::Battlefield);
                raw.objects.get_mut(face).v4.face_down_v1 = Some(FaceDownV1 {
                    disguised: true,
                    lookers: 0,
                    hidden_by: None,
                    hidden_by_source: None,
                });
                O::own(raw)
            };
            let a = build("Divine Resilience");
            let b = build("Llanowar Elves");
            assert_eq!(O::packet(&a.encode()), O::packet(&b.encode()));
            assert_eq!(
                fact_count(
                    O::packet(&a.encode()).state(),
                    "registered_instant_printing"
                ),
                usize::from(known)
            );
        }
    }
}
